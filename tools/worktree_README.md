# Claude Worktree Tool

Tool definitions for voice-controlled git worktree creation with Claude Code.

## What This Does

When you speak a command like **"create worktree for adding LED status"**, your dictation system will:

1. **Generate a branch name** (e.g., `add-led-status`)
2. **Create a git worktree** in a sibling directory (e.g., `transcribe-rs-v2-worktrees/add-led-status/`)
3. **Create a task file** (`.claude-task.md`) with your description
4. **Launch Claude Code** in a new terminal window
5. Claude Code reads the task and starts implementing it autonomously

This lets you **continue working in your main repo** while Claude Code works on a feature in isolation.

## Tool Definitions

Choose one based on your needs:

### 1. Simple (Single Project) - `claude_worktree_simple.json`

**Use if:** You only work on transcribe-rs-v2

- Hardcoded to work in `/home/seb/code/cloned/transcribe-rs-v2`
- Simpler parameter set
- No project selection needed

**Voice commands:**
- "create worktree for adding timestamps"
- "new branch for fixing audio clipping"
- "claude work on LED status feature"

### 2. Multi-Project - `claude_worktree_multi.json`

**Use if:** You want to create worktrees in different projects

- Supports any project path
- Can specify project in voice command
- Falls back to transcribe-rs-v2 if not specified

**Voice commands:**
- "create worktree in transcribe for adding timestamps"
- "new branch in transcribe-v1 for fixing recording"
- "claude work on my other project for feature X"

### 3. Original - `claude_worktree.json`

Basic version without the wrapper script. Requires running from within the git repo.

## Installation

1. **Copy the tool definition** to your dictation app's tools directory:
   ```bash
   # For simple version
   cp tools/claude_worktree_simple.json ~/path/to/your/dictation-config/tools/

   # OR for multi-project version
   cp tools/claude_worktree_multi.json ~/path/to/your/dictation-config/tools/
   ```

2. **Make sure the wrapper script is executable** (already done):
   ```bash
   chmod +x tools/claude_worktree_wrapper.sh
   ```

3. **Customize projects** (optional for multi-project):
   - Edit `tools/projects.md` to add more projects
   - Update paths in the JSON tool definition

4. **Reload your dictation app** to pick up the new tool

## How It Works

### Script Flow

```
Voice: "create worktree for adding LED support"
  ↓
LLM generates parameters:
  - branch_name: "add-led-support"
  - task_description: "Add LED indicator that shows recording status"
  - workspace: "3" (optional)
  ↓
Runs: claude_worktree_wrapper.sh
  ↓
Changes to project directory
  ↓
Calls: claude-worktree.sh "add-led-support" "Add LED indicator..."
  ↓
Creates: ../transcribe-rs-v2-worktrees/add-led-support/
  ↓
Creates: .claude-task.md with task description
  ↓
Opens new terminal (kitty/alacritty/foot/etc.) on workspace 3
  ↓
Launches: claude-code
  ↓
Claude Code reads .claude-task.md and starts working
```

### Directory Structure

```
/home/seb/code/cloned/
├── transcribe-rs-v2/              # Your main repo
│   ├── claude-worktree.sh         # Core script
│   └── tools/
│       ├── claude_worktree_wrapper.sh
│       └── *.json                 # Tool definitions
│
└── transcribe-rs-v2-worktrees/    # Worktrees go here
    ├── add-led-support/
    │   ├── .claude-task.md        # Task for Claude
    │   └── [full repo checkout]
    └── fix-audio-bug/
        └── ...
```

## Customization

### Change Workspace Behavior

To always open in a specific workspace, add it to the voice command:
- "create worktree for X on workspace 5"

Or modify the JSON to have a default workspace value.

### Change Terminal Behavior

The script auto-detects terminals in this priority:
1. kitty
2. alacritty
3. foot
4. wezterm
5. gnome-terminal
6. konsole

Edit `claude-worktree.sh` lines 82-98 to change detection order.

### Add Background Mode

To launch Claude Code without a terminal (runs in background):
- Modify the wrapper script to add `--background` flag
- Claude Code will run headless

## Cleanup

When Claude Code finishes and you've merged the branch:

```bash
# Remove the worktree
git worktree remove transcribe-rs-v2-worktrees/add-led-support

# Delete the branch (if merged)
git branch -d add-led-support
```

Or add a "cleanup worktree" tool that does this via voice command!

## Troubleshooting

**"Not in a git repository"**
- The wrapper script needs to cd to the project first
- Make sure project_path is correct in the tool definition

**"No terminal emulator detected"**
- Install one of the supported terminals (kitty recommended)
- Or modify the script to use your terminal

**"Worktree already exists"**
- Clean up old worktree: `git worktree remove path/to/worktree`
- Or use a different branch name

**Claude Code doesn't see the task**
- Check that `.claude-task.md` was created
- Make sure Claude Code is running in the worktree directory

## Advanced: Chaining Tool Calls

Your dictation LLM **cannot currently chain tool calls** (select project → create worktree). Instead:

**Option 1:** Use multi-project tool with project as parameter
**Option 2:** Create separate project-specific tools:
- `claude_worktree_transcribe.json`
- `claude_worktree_other_project.json`

**Option 3:** Add a "list projects" tool that helps the LLM understand available projects:

```json
{
  "name": "list_projects",
  "description": "List available projects for worktree creation",
  "command": "cat",
  "args": ["/home/seb/code/cloned/transcribe-rs-v2/tools/projects.md"]
}
```

Then the LLM can call `list_projects` first (if unsure), then call `claude_worktree` with the right path.

## Future Enhancements

- Add "merge worktree" tool to auto-merge and cleanup
- Add "list worktrees" tool to see what's in progress
- Add "kill worktree" tool to stop Claude Code and remove worktree
- Parse `projects.md` automatically instead of hardcoding paths
- Support custom Claude Code configurations per project
