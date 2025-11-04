use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;

/// Matching algorithm for fuzzy string comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MatchingAlgorithm {
    /// Jaro-Winkler distance (better for names, gives weight to matching prefixes)
    JaroWinkler,
    /// Levenshtein edit distance (better for general text)
    Levenshtein,
    /// Automatically choose based on pattern (single word = JaroWinkler, multi-word = Levenshtein)
    Auto,
}

impl Default for MatchingAlgorithm {
    fn default() -> Self {
        MatchingAlgorithm::Auto
    }
}

/// A correction rule for fixing transcription errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionRule {
    /// The incorrect transcription pattern
    pub from: String,
    /// The correct replacement
    pub to: String,
    /// Whether to match case-sensitively
    #[serde(default)]
    pub case_sensitive: bool,
    /// Enable fuzzy matching (Levenshtein distance)
    #[serde(default = "default_fuzzy_matching")]
    pub fuzzy_matching: bool,
    /// Similarity threshold for fuzzy matching (0.0 - 1.0)
    #[serde(default = "default_threshold")]
    pub similarity_threshold: f64,
    /// Which matching algorithm to use
    #[serde(default)]
    pub algorithm: MatchingAlgorithm,
}

fn default_fuzzy_matching() -> bool {
    true  // Enable fuzzy matching by default
}

fn default_threshold() -> f64 {
    0.85  // 85% similarity
}

/// A match found in the text
#[derive(Debug, Clone)]
struct FuzzyMatch {
    start: usize,
    end: usize,
    _similarity: f64,  // Stored for potential future debugging/logging
}

/// Word with position tracking
#[derive(Debug, Clone)]
struct Word {
    text: String,
    start: usize,
    end: usize,
}

/// Manages transcription error corrections (phonetic/acoustic errors)
pub struct TranscriptionCorrector {
    rules: Vec<CorrectionRule>,
}

impl TranscriptionCorrector {
    /// Load correction rules from a JSON file
    pub fn from_file(path: &Path) -> Result<Self, Box<dyn Error>> {
        if !path.exists() {
            // Create example file with fuzzy matching enabled
            let examples = vec![
                CorrectionRule {
                    from: "Sebastian Tuba".to_string(),
                    to: "Sebastian Kouba".to_string(),
                    case_sensitive: false,
                    fuzzy_matching: true,
                    similarity_threshold: 0.85,
                    algorithm: MatchingAlgorithm::JaroWinkler,
                },
                CorrectionRule {
                    from: "Hay Q".to_string(),
                    to: "HiQ".to_string(),
                    case_sensitive: false,
                    fuzzy_matching: true,
                    similarity_threshold: 0.70,
                    algorithm: MatchingAlgorithm::Levenshtein,
                },
            ];

            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let json = serde_json::to_string_pretty(&examples)?;
            fs::write(path, json)?;

            return Ok(Self { rules: examples });
        }

        let contents = fs::read_to_string(path)?;
        let rules: Vec<CorrectionRule> = serde_json::from_str(&contents)?;

        Ok(Self { rules })
    }

    /// Apply all correction rules to the text
    pub fn correct(&self, text: &str) -> String {
        let mut result = text.to_string();

        for rule in &self.rules {
            result = if rule.fuzzy_matching {
                self.fuzzy_replace(&result, rule)
            } else if rule.case_sensitive {
                result.replace(&rule.from, &rule.to)
            } else {
                self.replace_case_insensitive(&result, &rule.from, &rule.to)
            };
        }

        result
    }

    /// Apply fuzzy matching replacement
    fn fuzzy_replace(&self, text: &str, rule: &CorrectionRule) -> String {
        let matches = self.find_fuzzy_matches(text, rule);

        if matches.is_empty() {
            return text.to_string();
        }

        // Apply replacements in reverse order to preserve positions
        let mut result = text.to_string();
        for fuzzy_match in matches.iter().rev() {
            result.replace_range(fuzzy_match.start..fuzzy_match.end, &rule.to);
        }

        result
    }

    /// Find all fuzzy matches in the text
    fn find_fuzzy_matches(&self, text: &str, rule: &CorrectionRule) -> Vec<FuzzyMatch> {
        let words = self.tokenize(text);
        let pattern_words: Vec<&str> = rule.from.split_whitespace().collect();

        if pattern_words.is_empty() {
            return Vec::new();
        }

        // Choose algorithm
        let algorithm = match &rule.algorithm {
            MatchingAlgorithm::Auto => {
                if pattern_words.len() == 1 {
                    MatchingAlgorithm::JaroWinkler
                } else {
                    MatchingAlgorithm::Levenshtein
                }
            }
            other => other.clone(),
        };

        let mut matches = Vec::new();
        let mut last_match_end = 0;

        // Sliding window of size = pattern word count
        for window_start in 0..words.len() {
            let window_end = (window_start + pattern_words.len()).min(words.len());

            // Skip if wrong number of words or overlaps with previous match
            if window_end - window_start != pattern_words.len() || window_start < last_match_end {
                continue;
            }

            let window = &words[window_start..window_end];

            // Adjust threshold for short strings
            let effective_threshold = if rule.from.len() < 4 {
                0.95  // Very strict for short strings like "HiQ"
            } else {
                rule.similarity_threshold
            };

            // Early termination: check length difference
            let pattern_len: usize = pattern_words.iter().map(|w| w.len()).sum();
            let window_len: usize = window.iter().map(|w| w.text.len()).sum();
            let max_diff = ((pattern_len as f64) * (1.0 - effective_threshold)).ceil() as usize;
            if (pattern_len as i32 - window_len as i32).abs() > max_diff as i32 {
                continue;
            }

            // Compute similarity
            let similarity = self.compute_similarity(window, &pattern_words, &algorithm, rule.case_sensitive);

            if similarity >= effective_threshold {
                matches.push(FuzzyMatch {
                    start: window[0].start,
                    end: window[window.len() - 1].end,
                    _similarity: similarity,
                });
                last_match_end = window_end;
            }
        }

        matches
    }

    /// Compute similarity between window and pattern
    fn compute_similarity(
        &self,
        window: &[Word],
        pattern_words: &[&str],
        algorithm: &MatchingAlgorithm,
        case_sensitive: bool,
    ) -> f64 {
        use rapidfuzz::distance::{jaro_winkler, levenshtein};

        match algorithm {
            MatchingAlgorithm::JaroWinkler => {
                // Average similarity across all words
                let total: f64 = pattern_words
                    .iter()
                    .zip(window.iter())
                    .map(|(pattern, word)| {
                        let p = if case_sensitive {
                            pattern.to_string()
                        } else {
                            pattern.to_lowercase()
                        };
                        let w = if case_sensitive {
                            word.text.clone()
                        } else {
                            word.text.to_lowercase()
                        };

                        jaro_winkler::normalized_similarity(p.chars(), w.chars())
                    })
                    .sum();

                total / pattern_words.len() as f64
            }
            MatchingAlgorithm::Levenshtein | MatchingAlgorithm::Auto => {
                // Average similarity across all words (same as JaroWinkler for consistency)
                let total: f64 = pattern_words
                    .iter()
                    .zip(window.iter())
                    .map(|(pattern, word)| {
                        let p = if case_sensitive {
                            pattern.to_string()
                        } else {
                            pattern.to_lowercase()
                        };
                        let w = if case_sensitive {
                            word.text.clone()
                        } else {
                            word.text.to_lowercase()
                        };

                        levenshtein::normalized_similarity(p.chars(), w.chars())
                    })
                    .sum();

                total / pattern_words.len() as f64
            }
        }
    }

    /// Tokenize text into words with position tracking
    fn tokenize(&self, text: &str) -> Vec<Word> {
        let mut words = Vec::new();
        let mut current_word = String::new();
        let mut word_start = 0;

        for (i, ch) in text.char_indices() {
            if ch.is_alphanumeric() || ch == '\'' {
                if current_word.is_empty() {
                    word_start = i;
                }
                current_word.push(ch);
            } else if !current_word.is_empty() {
                words.push(Word {
                    text: current_word.clone(),
                    start: word_start,
                    end: i,
                });
                current_word.clear();
            }
        }

        // Don't forget the last word
        if !current_word.is_empty() {
            words.push(Word {
                text: current_word,
                start: word_start,
                end: text.len(),
            });
        }

        words
    }

    /// Case-insensitive replace - just uses the replacement as-is
    fn replace_case_insensitive(&self, text: &str, from: &str, to: &str) -> String {
        let from_lower = from.to_lowercase();
        let mut result = String::new();
        let mut last_end = 0;

        // Find all occurrences (case-insensitive)
        let text_lower = text.to_lowercase();

        while let Some(pos) = text_lower[last_end..].find(&from_lower) {
            let actual_pos = last_end + pos;

            // Add text before match
            result.push_str(&text[last_end..actual_pos]);

            // Add replacement as-is (user controls the casing in their config)
            result.push_str(to);

            last_end = actual_pos + from.len();
        }

        // Add remaining text
        result.push_str(&text[last_end..]);
        result
    }

    /// Check if any corrections would be applied
    pub fn would_correct(&self, text: &str) -> bool {
        self.correct(text) != text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_fuzzy_matching_name() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"[
            {{
                "from": "Sebastian Tuba",
                "to": "Sebastian Kouba",
                "case_sensitive": false,
                "fuzzy_matching": true,
                "similarity_threshold": 0.85,
                "algorithm": "JaroWinkler"
            }}
        ]"#).unwrap();

        let corrector = TranscriptionCorrector::from_file(file.path()).unwrap();

        // Should match exact
        let input1 = "Hello, my name is Sebastian Tuba.";
        let output1 = corrector.correct(input1);
        assert_eq!(output1, "Hello, my name is Sebastian Kouba.");

        // Should match similar variations
        let input2 = "Hello, my name is Sebastian Toba.";
        let output2 = corrector.correct(input2);
        assert_eq!(output2, "Hello, my name is Sebastian Kouba.");

        let input3 = "Hello, my name is Sebastian Tube.";
        let output3 = corrector.correct(input3);
        assert_eq!(output3, "Hello, my name is Sebastian Kouba.");
    }

    #[test]
    fn test_fuzzy_no_false_positives() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"[
            {{
                "from": "Sebastian Tuba",
                "to": "Sebastian Kouba",
                "case_sensitive": false,
                "fuzzy_matching": true,
                "similarity_threshold": 0.85,
                "algorithm": "JaroWinkler"
            }}
        ]"#).unwrap();

        let corrector = TranscriptionCorrector::from_file(file.path()).unwrap();

        // Should NOT match - too different
        let input = "Hello, my name is Sebastian Smith.";
        assert_eq!(corrector.correct(input), input);
    }

    #[test]
    fn test_literal_matching_still_works() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"[
            {{
                "from": "Hay Q",
                "to": "HiQ",
                "case_sensitive": false,
                "fuzzy_matching": false
            }}
        ]"#).unwrap();

        let corrector = TranscriptionCorrector::from_file(file.path()).unwrap();

        // Exact match should work
        assert_eq!(corrector.correct("I work at Hay Q."), "I work at HiQ.");

        // Non-exact should NOT match with fuzzy disabled
        assert_eq!(corrector.correct("I work at Hayq."), "I work at Hayq.");
    }

    #[test]
    fn test_short_string_high_threshold() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"[
            {{
                "from": "Hi",
                "to": "HiQ",
                "case_sensitive": false,
                "fuzzy_matching": true,
                "similarity_threshold": 0.70
            }}
        ]"#).unwrap();

        let corrector = TranscriptionCorrector::from_file(file.path()).unwrap();

        // Short string should use stricter threshold (0.95) automatically
        // "Hi" vs "Hug" = ~66% similar, should NOT match
        assert_eq!(corrector.correct("Hug there"), "Hug there");

        // Exact match should still work
        assert_eq!(corrector.correct("Hi there"), "HiQ there");
    }
}
