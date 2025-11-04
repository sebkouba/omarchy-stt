# Transcription Corrections Guide

## The Problem

Speech-to-text models like Parakeet and Whisper make **acoustic/phonetic errors** based on how things sound, not how they're spelled:

- "Sebastian Kouba" → transcribed as "Sebastian Tuba" (sounds similar)
- "HiQ" → transcribed as "Hay Q" (sounds identical)
- Your company name → transcribed incorrectly

**Harper's dictionary can't fix this!** Harper only sees the already-transcribed text. Since "Tuba" is a valid English word (the instrument), Harper thinks it's correct.

## The Solution

**Transcription Corrections** run BEFORE Harper and fix acoustic errors using **fuzzy matching**.

### What is Fuzzy Matching?

Instead of only matching exact strings like "Sebastian Tuba", fuzzy matching catches **variations**:

- ✅ "Sebastian Tuba" → "Sebastian Kouba"
- ✅ "Sebastian Toba" → "Sebastian Kouba"
- ✅ "Sebastian Tube" → "Sebastian Kouba"
- ✅ "Sebastian Cuba" → "Sebastian Kouba"
- ❌ "Sebastian Smith" → (too different, no match)

**How it works:**
1. Uses Levenshtein or Jaro-Winkler distance to measure similarity
2. Matches if similarity is above threshold (default 85%)
3. Short strings (<4 chars) use stricter 95% threshold automatically

**When to use fuzzy vs exact matching:**
- **Fuzzy** (default): Names, company names, proper nouns with phonetic variations
- **Exact**: Acronyms, technical terms, formatting fixes (e.g., "API's" → "APIs")

### Pipeline Order

```
1. Parakeet/Whisper transcribes → "Sebastian Tuba"
2. Transcription Corrections     → "Sebastian Kouba" ✅ (catches "Tuba", "Toba", "Tube", etc.)
3. Harper grammar/spelling       → (no change needed)
4. Add space after punctuation
5. Copy and paste
```

## Configuration

Edit `~/.config/transcribe-rs/config.toml`:

```toml
[transcription_corrections]
enabled = true
corrections_file = "/home/seb/.config/transcribe-rs/transcription_corrections.json"
```

## Corrections File Format

`~/.config/transcribe-rs/transcription_corrections.json`:

```json
[
  {
    "from": "Sebastian Tuba",
    "to": "Sebastian Kouba",
    "case_sensitive": false,
    "fuzzy_matching": true,
    "similarity_threshold": 0.85,
    "algorithm": "JaroWinkler"
  },
  {
    "from": "Hay Q",
    "to": "HiQ",
    "case_sensitive": false,
    "fuzzy_matching": true,
    "similarity_threshold": 0.70,
    "algorithm": "Levenshtein"
  },
  {
    "from": "API's",
    "to": "APIs",
    "case_sensitive": true,
    "fuzzy_matching": false
  }
]
```

### Rule Fields

- **`from`**: The incorrect transcription pattern to match
- **`to`**: The correct replacement text
- **`case_sensitive`**:
  - `false` (default): "hay q", "Hay Q", "HAY Q" all match and get replaced with "HiQ"
  - `true`: Only exact case matches are replaced
- **`fuzzy_matching`** (optional, default: `true`):
  - `true`: Use fuzzy matching to catch variations (e.g., "Tuba", "Toba", "Tube" all match)
  - `false`: Only exact string matches are replaced
- **`similarity_threshold`** (optional, default: `0.85`):
  - Range: 0.0 - 1.0 (0% - 100% similarity)
  - Recommended: 0.85 for names, 0.70 for phrases
  - Note: Short strings (<4 chars) automatically use 0.95 threshold to avoid false positives
- **`algorithm`** (optional, default: `"Auto"`):
  - `"JaroWinkler"`: Better for names and words with matching prefixes (e.g., "Sebastian Tuba" → "Sebastian Kouba")
  - `"Levenshtein"`: Better for general phrases and multi-word patterns (e.g., "Hay Q" → "HiQ")
  - `"Auto"`: Automatically choose based on pattern (single word = JaroWinkler, multi-word = Levenshtein)

## Common Use Cases

### Names (with Fuzzy Matching)

```json
{
  "from": "Sebastian Tuba",
  "to": "Sebastian Kouba",
  "case_sensitive": false,
  "fuzzy_matching": true,
  "similarity_threshold": 0.85,
  "algorithm": "JaroWinkler"
}
```

This will match:
- ✅ "Sebastian Tuba" (exact match)
- ✅ "Sebastian Toba" (one letter different)
- ✅ "Sebastian Tube" (similar sound)
- ✅ "Sebastian Cuba" (similar structure)
- ❌ "Sebastian Smith" (too different)

### Company Names

```json
{
  "from": "Hay Q",
  "to": "HiQ",
  "case_sensitive": false,
  "fuzzy_matching": true,
  "similarity_threshold": 0.70,
  "algorithm": "Levenshtein"
},
{
  "from": "Automatic",
  "to": "Automattic",
  "case_sensitive": false,
  "fuzzy_matching": true,
  "similarity_threshold": 0.85,
  "algorithm": "Auto"
}
```

### Technical Terms (Exact Matching)

```json
{
  "from": "docker hub",
  "to": "DockerHub",
  "case_sensitive": false,
  "fuzzy_matching": false
},
{
  "from": "kubernetes",
  "to": "Kubernetes",
  "case_sensitive": false,
  "fuzzy_matching": false
}
```

### Acronyms (Exact Matching Recommended)

```json
{
  "from": "L L M",
  "to": "LLM",
  "case_sensitive": false,
  "fuzzy_matching": false
},
{
  "from": "A P I",
  "to": "API",
  "case_sensitive": false,
  "fuzzy_matching": false
}
```

## How to Find Errors to Add

### Method 1: Check Debug Logs

```bash
tail -f /tmp/ptt_rust_debug.log
```

Look for lines like:
```
[2025-01-04 12:34:56] [cli] Transcription text: 'Hello my name is Sebastian Tuba'
[2025-01-04 12:34:56] [cli] Applied transcription corrections: 'Sebastian Tuba' → 'Sebastian Kouba'
```

### Method 2: Check Harper Correction Sessions

```bash
ls -lh ~/.config/transcribe-rs/harper_corrections/
cat ~/.config/transcribe-rs/harper_corrections/2025-01-04_12-34-56.json
```

Look for patterns in `original_text` that Harper couldn't fix.

### Method 3: Just Notice While Dictating

When you say "HiQ" and see "Hay Q" appear, add a rule!

## Adding New Rules

### Quick Add

```bash
cat >> ~/.config/transcribe-rs/transcription_corrections.json << 'EOF'
,
  {
    "from": "Your Wrong Text",
    "to": "Your Correct Text",
    "case_sensitive": false
  }
EOF
```

### Edit Directly

```bash
nano ~/.config/transcribe-rs/transcription_corrections.json
```

**Note:** Make sure JSON is valid (commas between entries, no trailing comma on last entry).

## Testing Your Rules

### Quick Test

Create a test file:

```bash
cat > /tmp/test_corrections.txt << 'EOF'
I work with Sebastian Tuba at Hay Q.
EOF
```

Run a quick test (TODO: add CLI command for this):

```bash
# For now, you'll see corrections in the debug log when you dictate
```

### Live Testing

1. Restart daemon: `./start-daemon.sh`
2. Dictate something with the corrected terms
3. Check logs: `tail -f /tmp/ptt_rust_debug.log`

## Difference from Harper's Dictionary

| Feature | Transcription Corrections | Harper Dictionary |
|---------|--------------------------|-------------------|
| **Purpose** | Fix acoustic/phonetic errors from speech model | Prevent spell-check false positives |
| **When it runs** | BEFORE Harper (step 2) | During Harper (step 3) |
| **Example** | "Hay Q" → "HiQ" | Marks "HiQ" as valid word |
| **Use case** | Sound-alikes, common transcription errors | Technical terms, proper nouns |
| **Format** | JSON with from/to rules | Plain text word list |
| **File** | `transcription_corrections.json` | `harper_dictionary.txt` |

## You Need BOTH

1. **Add to transcription_corrections.json** if Parakeet gets it wrong phonetically
2. **Add to harper_dictionary.txt** if Harper flags it as misspelled

Example:
- "HiQ" sounds like "Hay Q" → Add to `transcription_corrections.json`
- Harper doesn't know "HiQ" is a word → Add to `harper_dictionary.txt`

## Tuning Fuzzy Matching

### Choosing the Right Algorithm

**JaroWinkler** (best for names):
- Gives extra weight to matching prefixes
- Great for: "Sebastian Tuba" → "Sebastian Kouba" (prefix "Sebastian" matches)
- Use when: Pattern has consistent prefix/structure

**Levenshtein** (best for phrases):
- Measures edit distance (insertions, deletions, substitutions)
- Great for: "Hay Q" → "HiQ", general multi-word patterns
- Use when: Pattern has no clear prefix pattern

**Auto** (recommended default):
- Single word → JaroWinkler
- Multiple words → Levenshtein
- Use when: You want sensible defaults

### Adjusting Similarity Thresholds

**0.85 (default)** - Good for most names and proper nouns:
- Catches 1-2 character variations
- Minimal false positives

**0.70-0.80** - More lenient for phrases:
- Catches more variations
- Slightly higher false positive risk

**0.90-0.95** - Stricter matching:
- Only very close matches
- Use for short patterns or when false positives are unacceptable

**Note:** Short strings (<4 chars) automatically use 0.95 threshold regardless of your setting.

### Testing Your Rules

Create a test to see what matches:

```bash
# Add this to your corrections file
{
  "from": "Your Name",
  "to": "Correct Name",
  "fuzzy_matching": true,
  "similarity_threshold": 0.85,
  "algorithm": "JaroWinkler"
}

# Then try dictating variations and check the debug log
tail -f /tmp/ptt_rust_debug.log | grep "Applied transcription correction"
```

## Performance

- Fuzzy matching runs in **milliseconds** (typically <5ms for typical rules)
- Uses optimized rapidfuzz library (Rust implementation)
- Early termination for length differences
- No noticeable impact on transcription speed
- Processes all rules in a single pass through the text

## Troubleshooting

**Rules not being applied:**
- Check JSON syntax is valid (commas between entries, no trailing comma on last entry)
- Check `enabled = true` in config
- Restart daemon after editing corrections file: `systemctl --user restart transcribe-daemon`
- Check debug logs: `tail -f /tmp/ptt_rust_debug.log`

**Fuzzy matching too aggressive (false positives):**
- Increase `similarity_threshold` (try 0.90 or 0.95)
- Check if short strings are involved (they automatically use 0.95)
- Consider using exact matching (`fuzzy_matching: false`) for that rule

**Fuzzy matching not catching variations:**
- Decrease `similarity_threshold` (try 0.75 or 0.70)
- Try different algorithm:
  - Switch from Levenshtein to JaroWinkler for names
  - Switch from JaroWinkler to Levenshtein for phrases
- Check debug logs to see similarity scores

**Case sensitivity confusion:**
- Set `case_sensitive: false` for most rules (default behavior)
- Only use `true` for exact case-sensitive matches (like "API" vs "api")

## Future Enhancements

### Coming Soon: LLM-Generated Rules

Your LLM rule generator idea will analyze correction patterns and suggest:

```json
{
  "from": "Sebastian Tuba",
  "to": "Sebastian Kouba",
  "case_sensitive": false,
  "confidence": 0.95,
  "occurrences": 12,
  "context": ["discussing team members", "email signatures"]
}
```

You'll review and accept/reject these suggestions to build your personal dictionary.

## Examples

### Without Fuzzy Matching (Old Behavior)

```
Dictate: "Hi, I'm Sebastian Kouba from HiQ working on the API."
Transcribed: "Hi, I'm Sebastian Tuba from Hay Q working on the A P I."
Corrected:   "Hi, I'm Sebastian Kouba from HiQ working on the A P I." ✅

Dictate: "Hi, I'm Sebastian Kouba from HiQ working on the API."
Transcribed: "Hi, I'm Sebastian Toba from Hay Q working on the A P I."
Corrected:   "Hi, I'm Sebastian Toba from HiQ working on the A P I." ❌ (missed variation)
```

### With Fuzzy Matching (New Behavior)

```
Dictate: "Hi, I'm Sebastian Kouba from HiQ working on the API."
Transcribed: "Hi, I'm Sebastian Tuba from Hay Q working on the A P I."
Corrected:   "Hi, I'm Sebastian Kouba from HiQ working on the API." ✅

Dictate: "Hi, I'm Sebastian Kouba from HiQ working on the API."
Transcribed: "Hi, I'm Sebastian Toba from Hay Q working on the A P I."
Corrected:   "Hi, I'm Sebastian Kouba from HiQ working on the API." ✅ (caught variation!)

Dictate: "Hi, I'm Sebastian Kouba from HiQ working on the API."
Transcribed: "Hi, I'm Sebastian Tube from Hike working on the A P I."
Corrected:   "Hi, I'm Sebastian Kouba from Hike working on the API." ✅ (caught another variation!)
```

## Tips

1. **Start small**: Add rules as you encounter errors
2. **Be specific**: "Sebastian Tuba" is better than just "Tuba"
3. **Use full context**: "Hay Q" instead of just "Q"
4. **Enable fuzzy matching for names**: Catches variations automatically (default behavior)
5. **Use exact matching for acronyms**: Prevents false positives on short patterns
6. **Start with default threshold (0.85)**: Adjust only if you get false positives/negatives
7. **Use JaroWinkler for names**: Better for patterns with consistent prefixes
8. **Use Levenshtein for phrases**: Better for multi-word patterns
9. **Review debug logs**: See what's matching and adjust thresholds accordingly
10. **Case-insensitive by default**: Most rules should have `case_sensitive: false`
