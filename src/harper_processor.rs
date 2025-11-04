use chrono::{DateTime, Utc};
use harper_core::linting::{LintGroup, Linter};
use harper_core::spell::{FstDictionary, MergedDictionary, MutableDictionary};
use harper_core::{DictWordMetadata, Document};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Harper's dialect enum (re-export to avoid serde issues)
#[derive(Debug, Clone, Copy)]
pub enum Dialect {
    American,
    British,
    Australian,
    Canadian,
}

impl Dialect {
    fn to_harper_dialect(self) -> harper_core::Dialect {
        match self {
            Dialect::American => harper_core::Dialect::American,
            Dialect::British => harper_core::Dialect::British,
            Dialect::Australian => harper_core::Dialect::Australian,
            Dialect::Canadian => harper_core::Dialect::Canadian,
        }
    }
}

/// A record of a single correction made by Harper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarperCorrection {
    /// Character span in the original text
    pub span: (usize, usize),
    /// The original text that was flagged
    pub original: String,
    /// The replacement that was applied
    pub replacement: String,
    /// Type of lint (Spelling, Grammar, etc.)
    pub lint_kind: String,
    /// Harper's explanation message
    pub message: String,
}

/// A session recording all corrections made to a single transcription
#[derive(Debug, Serialize, Deserialize)]
pub struct CorrectionSession {
    pub timestamp: DateTime<Utc>,
    pub original_text: String,
    pub corrected_text: String,
    pub corrections: Vec<HarperCorrection>,
}

impl CorrectionSession {
    /// Check if any corrections were made
    pub fn has_corrections(&self) -> bool {
        !self.corrections.is_empty()
    }

    /// Save this session to a JSON file
    pub fn save_to_file(&self, dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
        fs::create_dir_all(dir)?;

        let filename = format!(
            "{}.json",
            self.timestamp.format("%Y-%m-%d_%H-%M-%S")
        );
        let path = dir.join(filename);

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;

        Ok(path)
    }
}

/// Cached curated dictionary (shared across all invocations)
/// This saves ~200-300ms on each transcription by loading the dictionary only once
static CURATED_DICT: OnceLock<Arc<FstDictionary>> = OnceLock::new();

/// Get or initialize the cached curated dictionary
fn get_curated_dict() -> Arc<FstDictionary> {
    // FstDictionary::curated() returns Arc<FstDictionary>
    CURATED_DICT.get_or_init(|| FstDictionary::curated()).clone()
}

/// Load custom user dictionary from a file
fn load_user_dictionary(dict_path: &Path) -> Result<MutableDictionary, Box<dyn Error>> {
    let mut dict = MutableDictionary::new();

    if !dict_path.exists() {
        // Create empty dictionary file
        if let Some(parent) = dict_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(dict_path, "")?;
        return Ok(dict);
    }

    let contents = fs::read_to_string(dict_path)?;

    for line in contents.lines() {
        let word = line.trim();
        if !word.is_empty() && !word.starts_with('#') {
            dict.append_word_str(word, DictWordMetadata::default());
        }
    }

    Ok(dict)
}

/// Process text through Harper, auto-accepting all suggestions and tracking changes
pub fn process_with_harper(
    text: &str,
    user_dict_path: &Path,
    dialect: Dialect,
    disabled_linters: &[String],
) -> Result<CorrectionSession, Box<dyn Error>> {
    // Load dictionaries
    let user_dict = load_user_dictionary(user_dict_path)?;

    let mut merged_dict = MergedDictionary::new();
    // Use cached curated dictionary (loaded once on first invocation)
    merged_dict.add_dictionary(get_curated_dict());
    merged_dict.add_dictionary(Arc::new(user_dict));

    let merged_dict_arc = Arc::new(merged_dict);

    // Create document
    let document = Document::new_plain_english(text, &*merged_dict_arc);

    // Create lint group with all default linters
    let harper_dialect = dialect.to_harper_dialect();
    let mut linter = LintGroup::new_curated(merged_dict_arc.clone(), harper_dialect);

    // Disable linters from config
    for linter_name in disabled_linters {
        linter.config.set_rule_enabled(linter_name, false);
    }

    // Get all lints
    let mut lints = linter.lint(&document);

    // Sort lints by position (reverse order to avoid offset issues when applying)
    lints.sort_by_key(|l| std::cmp::Reverse(l.span.start));

    // Apply all corrections and track them
    let mut text_chars: Vec<char> = text.chars().collect();
    let mut corrections = Vec::new();

    for lint in lints {
        // Auto-accept if there's at least one suggestion
        if let Some(suggestion) = lint.suggestions.first() {
            let original = document.get_span_content_str(&lint.span);

            // Apply the suggestion
            suggestion.apply(lint.span, &mut text_chars);

            // Extract what the replacement was
            let replacement = match suggestion {
                harper_core::linting::Suggestion::ReplaceWith(chars) => {
                    chars.iter().collect::<String>()
                }
                harper_core::linting::Suggestion::InsertAfter(chars) => {
                    format!("{}{}", original, chars.iter().collect::<String>())
                }
                harper_core::linting::Suggestion::Remove => String::new(),
            };

            corrections.push(HarperCorrection {
                span: (lint.span.start, lint.span.end),
                original,
                replacement,
                lint_kind: format!("{:?}", lint.lint_kind),
                message: lint.message,
            });
        }
    }

    let corrected_text = text_chars.iter().collect::<String>();

    Ok(CorrectionSession {
        timestamp: Utc::now(),
        original_text: text.to_string(),
        corrected_text,
        corrections,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_basic_correction() {
        let mut dict_file = NamedTempFile::new().unwrap();
        writeln!(dict_file, "# Test dictionary").unwrap();

        let text = "This is a teh test.";
        let session = process_with_harper(
            text,
            dict_file.path(),
            super::Dialect::American,
            &["AvoidCurses".to_string()],
        ).unwrap();

        assert!(session.has_corrections());
        assert!(session.corrected_text.contains("the"));
    }

    #[test]
    fn test_no_corrections_needed() {
        let dict_file = NamedTempFile::new().unwrap();

        let text = "This is a perfect sentence.";
        let session = process_with_harper(
            text,
            dict_file.path(),
            super::Dialect::American,
            &["AvoidCurses".to_string()],
        ).unwrap();

        assert!(!session.has_corrections());
        assert_eq!(session.original_text, session.corrected_text);
    }

    #[test]
    fn test_custom_dictionary() {
        let mut dict_file = NamedTempFile::new().unwrap();
        writeln!(dict_file, "LLM").unwrap();
        writeln!(dict_file, "Parakeet").unwrap();

        let text = "I use an LLM with Parakeet.";
        let session = process_with_harper(
            text,
            dict_file.path(),
            super::Dialect::American,
            &["AvoidCurses".to_string()],
        ).unwrap();

        // Should not flag LLM or Parakeet as misspelled
        assert_eq!(session.original_text, session.corrected_text);
    }
}
