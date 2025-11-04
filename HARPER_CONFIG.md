# Harper Configuration Guide

Harper is integrated into transcribe-rs-v2 to automatically correct grammar and spelling errors in your transcriptions.

## Configuration File

Edit `~/.config/transcribe-rs/config.toml`:

```toml
[harper]
enabled = true
dictionary_path = "/home/seb/.config/transcribe-rs/harper_dictionary.txt"
dialect = "American"
corrections_dir = "/home/seb/.config/transcribe-rs/harper_corrections"
disabled_linters = [
    "AvoidCurses",
]
```

## Available Dialects

Harper only supports **English** with these dialect variants:
- `"American"` - US English (default)
- `"British"` - UK English
- `"Australian"` - Australian English
- `"Canadian"` - Canadian English

**Note:** For other languages like German, you'll need to disable Harper and use a different grammar checker.

## Disabling Specific Linters

Add linter names to the `disabled_linters` array to turn them off:

```toml
disabled_linters = [
    "AvoidCurses",      # No censorship
    "FillerWords",      # Keep "basically", "actually", etc.
    "Hedging",          # Keep "sort of", "kind of"
    "LongSentences",    # Allow long sentences
]
```

## Common Linters You Might Want to Disable

### Style Linters (Opinionated)
- **`AvoidCurses`** - Censors swear words with asterisks (disabled by default)
- **`FillerWords`** - Flags conversational words: "basically", "actually", "literally"
- **`Hedging`** - Flags hedging language: "sort of", "kind of", "maybe"
- **`BoringWords`** - Suggests "better" vocabulary (disabled by default)
- **`DiscourseMarkers`** - Flags conversational phrases
- **`LongSentences`** - Flags sentences over a certain length

### Grammar Linters (Usually Helpful)
- **`SpellCheck`** - Spell checking (keep enabled!)
- **`RepeatedWords`** - Detects "the the"
- **`SentenceCapitalization`** - Capitalizes sentence starts
- **`AnA`** - Fixes "a apple" → "an apple"
- **`ItsContraction`** / **`ItsPossessive`** - Fixes its/it's confusion
- **`YourYoure`** - Fixes your/you're confusion

## Full List of Linters

Harper has 100+ linters. Here are the major ones:

**Spelling & Typos:**
- SpellCheck, Misspell, DotInitialisms, InitialismLinter

**Grammar:**
- AnA, RepeatedWords, SentenceCapitalization, ProperNounCapitalization
- ItsContraction, ItsPossessive, Cant, Didnt
- SubjectVerbAgreement, PronounInflectionBe

**Common Errors:**
- ThenThan, ThereTheir, ToTwoToo
- EffectAffect, LooseLose, AcceptExcept

**Punctuation:**
- UnclosedQuotes, CommaFixes, EllipsisLength, NoFrenchSpaces

**Style (often disabled for dictation):**
- AvoidCurses, FillerWords, Hedging, BoringWords
- LongSentences, DiscourseMarkers

## Custom Dictionary

Add technical terms, names, or specialized vocabulary to prevent false corrections:

```bash
echo "LLM" >> ~/.config/transcribe-rs/harper_dictionary.txt
echo "Parakeet" >> ~/.config/transcribe-rs/harper_dictionary.txt
echo "Automattic" >> ~/.config/transcribe-rs/harper_dictionary.txt
```

Comments are supported:
```
# My custom dictionary
LLM
API
GPU
MyCompanyName
```

## Reviewing Corrections

All corrections are saved to `~/.config/transcribe-rs/harper_corrections/` in JSON format:

```bash
ls -lh ~/.config/transcribe-rs/harper_corrections/
cat ~/.config/transcribe-rs/harper_corrections/2025-01-04_12-34-56.json
```

Each file shows:
- Original text
- Corrected text
- List of all changes with explanations
- Timestamp

## Disabling Harper Completely

Set `enabled = false` in your config:

```toml
[harper]
enabled = false
```

## Language Support Limitation

**Harper only supports English.** If you're dictating in German or another language:

1. Disable Harper: `enabled = false`
2. Consider alternatives:
   - LanguageTool (supports 30+ languages including German)
   - Hunspell (multilingual spell checker)
   - Language-specific grammar checkers

## Troubleshooting

**Harper is censoring my words:**
- Add `"AvoidCurses"` to `disabled_linters`

**Harper is changing my technical terms:**
- Add them to `~/.config/transcribe-rs/harper_dictionary.txt`

**Too many style corrections:**
- Disable opinionated linters: `FillerWords`, `Hedging`, `BoringWords`, `LongSentences`

**Want to see what changed:**
- Check debug log: `tail -f /tmp/ptt_rust_debug.log`
- Review correction files: `~/.config/transcribe-rs/harper_corrections/`

## Future: LLM Rule Generation

Coming soon: An LLM will analyze your correction history and suggest dictionary additions based on patterns it finds in your speech.
