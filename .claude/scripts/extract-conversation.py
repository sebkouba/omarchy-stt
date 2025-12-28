#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""
Extract readable conversation from a Claude Code transcript JSONL file.

Usage:
  ./extract-conversation.py <transcript.jsonl>                    # Conversation overview
  ./extract-conversation.py <transcript.jsonl> --max-lines 60     # Limit output
  ./extract-conversation.py <transcript.jsonl> --tools            # Include tool activity summary
  ./extract-conversation.py <transcript.jsonl> --line 150         # Deep dive: show full content around line 150
"""

import json
import sys
from pathlib import Path


def extract_text_content(content):
    """Extract text from message content (string or array)."""
    if isinstance(content, str):
        # Skip system/meta messages
        if content.startswith("Caveat:") or content.startswith("<"):
            return None
        return content[:2000]  # Truncate very long messages

    if isinstance(content, list):
        texts = []
        for item in content:
            if isinstance(item, dict):
                if item.get("type") == "text":
                    text = item.get("text", "")
                    if text and not text.startswith("<"):
                        texts.append(text[:1000])
                # Skip tool_use and tool_result
        return "\n".join(texts) if texts else None

    return None


def extract_tool_info(content):
    """Extract tool usage info from message content."""
    tools = []
    if isinstance(content, list):
        for item in content:
            if isinstance(item, dict):
                if item.get("type") == "tool_use":
                    tool_name = item.get("name", "unknown")
                    tool_input = item.get("input", {})
                    # Extract key info based on tool type
                    if tool_name == "Edit":
                        file_path = tool_input.get("file_path", "")
                        tools.append(f"Edit: {file_path.split('/')[-1] if file_path else 'unknown'}")
                    elif tool_name == "Write":
                        file_path = tool_input.get("file_path", "")
                        tools.append(f"Write: {file_path.split('/')[-1] if file_path else 'unknown'}")
                    elif tool_name == "Read":
                        file_path = tool_input.get("file_path", "")
                        tools.append(f"Read: {file_path.split('/')[-1] if file_path else 'unknown'}")
                    elif tool_name == "Bash":
                        cmd = tool_input.get("command", "")[:50]
                        tools.append(f"Bash: {cmd}")
                    else:
                        tools.append(tool_name)
                elif item.get("type") == "tool_result":
                    is_error = item.get("is_error", False)
                    if is_error:
                        content_preview = str(item.get("content", ""))[:100]
                        tools.append(f"ERROR: {content_preview}")
    return tools


def deep_dive(transcript_path, target_line, context=5):
    """Show full content around a specific line number."""
    print(f"=== Deep Dive: Lines {target_line - context} to {target_line + context} ===\n")

    with open(transcript_path) as f:
        lines = f.readlines()

    start = max(0, target_line - context - 1)
    end = min(len(lines), target_line + context)

    for i in range(start, end):
        line = lines[i].strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
            entry_type = entry.get("type", "unknown")

            print(f"--- Line {i + 1} [{entry_type}] ---")

            if entry_type in ("user", "assistant"):
                message = entry.get("message", {})
                content = message.get("content")

                if isinstance(content, str):
                    print(content[:2000])
                elif isinstance(content, list):
                    for item in content:
                        if isinstance(item, dict):
                            item_type = item.get("type")
                            if item_type == "text":
                                print(f"[TEXT] {item.get('text', '')[:1500]}")
                            elif item_type == "tool_use":
                                print(f"[TOOL_USE] {item.get('name')}: {json.dumps(item.get('input', {}))[:500]}")
                            elif item_type == "tool_result":
                                is_error = "ERROR " if item.get("is_error") else ""
                                print(f"[TOOL_RESULT {is_error}] {str(item.get('content', ''))[:1500]}")
            print()
        except json.JSONDecodeError:
            print(f"[Invalid JSON at line {i + 1}]")


def main():
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        sys.exit(1)

    transcript_path = Path(sys.argv[1])
    max_lines = 200
    include_tools = "--tools" in sys.argv
    deep_dive_line = None

    if "--max-lines" in sys.argv:
        idx = sys.argv.index("--max-lines")
        max_lines = int(sys.argv[idx + 1])

    if "--line" in sys.argv:
        idx = sys.argv.index("--line")
        deep_dive_line = int(sys.argv[idx + 1])

    if not transcript_path.exists():
        print(f"Error: {transcript_path} not found", file=sys.stderr)
        sys.exit(1)

    # Deep dive mode
    if deep_dive_line:
        deep_dive(transcript_path, deep_dive_line)
        return

    messages = []
    tool_activity = []  # (line_num, tools_list)
    errors = []  # (line_num, error_preview)

    with open(transcript_path) as f:
        for line_num, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue

            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue

            # Skip non-message entries
            if entry.get("type") not in ("user", "assistant"):
                continue

            message = entry.get("message", {})
            role = message.get("role", entry.get("type", "unknown"))
            content = message.get("content")

            if content is None:
                continue

            # Track tool activity
            tools = extract_tool_info(content)
            if tools:
                tool_activity.append((line_num, tools))
                for t in tools:
                    if t.startswith("ERROR"):
                        errors.append((line_num, t))

            # Skip internal/system messages for conversation
            if entry.get("userType") not in ("external", None):
                if entry.get("type") != "assistant":
                    continue

            text = extract_text_content(content)
            if text:
                messages.append({
                    "role": role.upper(),
                    "text": text,
                    "line": line_num
                })

    # Output the conversation
    print(f"=== Conversation Extract ({len(messages)} messages, {len(tool_activity)} tool calls) ===\n")

    # Show errors summary first if any
    if errors:
        print("### ERRORS ENCOUNTERED ###")
        for line_num, err in errors[:10]:  # Show first 10 errors
            print(f"  Line {line_num}: {err[:80]}")
        print(f"\nUse --line N to deep dive into any of these.\n")

    # If too many messages, sample from beginning, middle, end
    if len(messages) > max_lines:
        sample_size = max_lines // 3
        beginning = messages[:sample_size]
        middle_start = len(messages) // 2 - sample_size // 2
        middle = messages[middle_start:middle_start + sample_size]
        end = messages[-sample_size:]

        print("--- BEGINNING ---")
        for msg in beginning:
            print(f"\n[{msg['role']}] (line {msg['line']})")
            print(msg['text'][:500])

        print(f"\n--- MIDDLE (around message {middle_start}) ---")
        for msg in middle:
            print(f"\n[{msg['role']}] (line {msg['line']})")
            print(msg['text'][:500])

        print("\n--- END ---")
        for msg in end:
            print(f"\n[{msg['role']}] (line {msg['line']})")
            print(msg['text'][:500])
    else:
        for msg in messages:
            print(f"\n[{msg['role']}] (line {msg['line']})")
            print(msg['text'])

    # Tool activity summary
    if include_tools:
        print("\n\n=== Tool Activity Summary ===")
        for line_num, tools in tool_activity[-30:]:  # Last 30 tool activities
            print(f"Line {line_num}: {', '.join(tools)}")
        print(f"\nTotal: {len(tool_activity)} tool calls")


if __name__ == "__main__":
    main()
