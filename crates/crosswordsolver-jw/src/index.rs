use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

use bitvec::prelude::*;
use thiserror::Error;
use tracing::{info, warn};

pub const MAX_WORD_LEN: usize = 24;
const ALPHABET: usize = 26;

type BitSet = BitVec<usize, Lsb0>;

#[derive(Debug, Clone)]
pub struct WordIndex {
    lens: Vec<Option<LenIndex>>,
}

#[derive(Debug, Clone)]
struct LenIndex {
    words: Vec<String>,
    all: BitSet,
    pos_letter: Vec<[BitSet; ALPHABET]>,
    contains: [BitSet; ALPHABET],
    letter_counts: Vec<[u8; ALPHABET]>,
}

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("failed to read wordlist: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct QueryParams<'a> {
    pub pattern: &'a [Option<u8>],
    pub must_include: &'a [u8],
    pub cannot_include: &'a [u8],
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug)]
pub struct AnagramParams<'a> {
    pub pattern: &'a [Option<u8>],
    pub bag_counts: [u8; ALPHABET],
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub total: usize,
    pub items: Vec<String>,
    pub has_more: bool,
}

impl WordIndex {
    pub fn empty() -> Self {
        Self {
            lens: vec![None; MAX_WORD_LEN + 1],
        }
    }

    pub fn build_from_file<P: AsRef<Path>>(path: P) -> Result<Arc<Self>, IndexError> {
        let path_ref = path.as_ref();
        let mut buckets: Vec<Vec<String>> = vec![Vec::new(); MAX_WORD_LEN + 1];

        let file = File::open(path_ref)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let raw = line?;
            if let Some(word) = normalize_word(&raw) {
                buckets[word.len()].push(word);
            }
        }

        let mut lens = vec![None; MAX_WORD_LEN + 1];
        let mut total = 0usize;
        for (len, bucket) in buckets.into_iter().enumerate() {
            if len == 0 || bucket.is_empty() {
                continue;
            }
            let mut sorted = bucket;
            sorted.sort();
            sorted.dedup();

            let count = sorted.len();
            if let Some(len_index) = LenIndex::build(len, sorted) {
                info!("loaded {count} words of length {len}");
                total += count;
                lens[len] = Some(len_index);
            } else {
                warn!("skipped length {len} with empty bucket after normalization");
            }
        }

        info!("total words indexed: {total}");
        Ok(Arc::new(Self { lens }))
    }

    pub fn query(&self, params: QueryParams<'_>) -> QueryResult {
        let len = params.pattern.len();
        let Some(len_index) = self.lens.get(len).and_then(|o| o.as_ref()) else {
            return QueryResult {
                total: 0,
                items: Vec::new(),
                has_more: false,
            };
        };

        let mut candidates = len_index.all.clone();

        for (pos, ch) in params.pattern.iter().enumerate() {
            if let Some(letter) = ch {
                let idx = (letter - b'a') as usize;
                candidates &= &len_index.pos_letter[pos][idx];
                if candidates.not_any() {
                    break;
                }
            }
        }

        if candidates.not_any() {
            return QueryResult {
                total: 0,
                items: Vec::new(),
                has_more: false,
            };
        }

        for letter in params.must_include {
            let idx = (*letter - b'a') as usize;
            candidates &= &len_index.contains[idx];
            if candidates.not_any() {
                break;
            }
        }

        if candidates.not_any() {
            return QueryResult {
                total: 0,
                items: Vec::new(),
                has_more: false,
            };
        }

        for letter in params.cannot_include {
            let idx = (*letter - b'a') as usize;
            let mask = !len_index.contains[idx].clone();
            candidates &= &mask;
            if candidates.not_any() {
                break;
            }
        }

        let total = candidates.count_ones();
        if total == 0 {
            return QueryResult {
                total: 0,
                items: Vec::new(),
                has_more: false,
            };
        }

        let offset = params
            .page
            .saturating_sub(1)
            .saturating_mul(params.page_size);
        let mut items = Vec::with_capacity(params.page_size.min(total));
        for idx in candidates.iter_ones().skip(offset).take(params.page_size) {
            if let Some(word) = len_index.words.get(idx) {
                items.push(word.clone());
            }
        }

        let has_more = offset + items.len() < total;

        QueryResult {
            total,
            items,
            has_more,
        }
    }

    pub fn query_anagram(&self, params: AnagramParams<'_>) -> QueryResult {
        let len = params.pattern.len();
        let Some(len_index) = self.lens.get(len).and_then(|o| o.as_ref()) else {
            return QueryResult {
                total: 0,
                items: Vec::new(),
                has_more: false,
            };
        };

        let mut candidates = len_index.all.clone();
        for (pos, ch) in params.pattern.iter().enumerate() {
            if let Some(letter) = ch {
                let idx = (letter - b'a') as usize;
                candidates &= &len_index.pos_letter[pos][idx];
                if candidates.not_any() {
                    break;
                }
            }
        }

        if candidates.not_any() {
            return QueryResult {
                total: 0,
                items: Vec::new(),
                has_more: false,
            };
        }

        let offset = params
            .page
            .saturating_sub(1)
            .saturating_mul(params.page_size);
        let mut total = 0usize;
        let mut items = Vec::with_capacity(params.page_size);

        for idx in candidates.iter_ones() {
            if let Some(counts) = len_index.letter_counts.get(idx)
                && *counts == params.bag_counts {
                    total += 1;
                    if total > offset && items.len() < params.page_size
                        && let Some(word) = len_index.words.get(idx) {
                            items.push(word.clone());
                        }
                }
        }

        let has_more = offset + items.len() < total;

        QueryResult {
            total,
            items,
            has_more,
        }
    }
}

impl LenIndex {
    fn build(len: usize, words: Vec<String>) -> Option<Self> {
        let n = words.len();
        if n == 0 {
            return None;
        }

        let mut pos_letter: Vec<[BitSet; ALPHABET]> = (0..len)
            .map(|_| array_init::array_init(|_| bitvec![usize, Lsb0; 0; n]))
            .collect();
        let mut contains: [BitSet; ALPHABET] =
            array_init::array_init(|_| bitvec![usize, Lsb0; 0; n]);
        let mut letter_counts: Vec<[u8; ALPHABET]> = Vec::with_capacity(n);

        for (idx, word) in words.iter().enumerate() {
            let mut counts = [0u8; ALPHABET];
            for (pos, ch) in word.bytes().enumerate() {
                let letter_idx = (ch - b'a') as usize;
                counts[letter_idx] = counts[letter_idx].saturating_add(1);
                if let Some(bitvec) = pos_letter
                    .get_mut(pos)
                    .and_then(|arr| arr.get_mut(letter_idx))
                {
                    bitvec.set(idx, true);
                }
                if let Some(bitvec) = contains.get_mut(letter_idx) {
                    bitvec.set(idx, true);
                }
            }
            letter_counts.push(counts);
        }

        Some(Self {
            words,
            all: bitvec![usize, Lsb0; 1; n],
            pos_letter,
            contains,
            letter_counts,
        })
    }
}

fn normalize_word(raw: &str) -> Option<String> {
    let mut normalized = String::with_capacity(raw.len());
    for c in raw.chars() {
        if !c.is_ascii_alphabetic() {
            normalized.clear();
            return None;
        }
        normalized.push(c.to_ascii_lowercase());
    }
    let len = normalized.len();
    if (1..=MAX_WORD_LEN).contains(&len) {
        Some(normalized)
    } else {
        None
    }
}

pub fn parse_pattern(raw: &str) -> Result<Vec<Option<u8>>, PatternError> {
    let mut result = Vec::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '_' | '?' | '.' => result.push(None),
            letter if letter.is_ascii_alphabetic() => {
                result.push(Some(letter.to_ascii_lowercase() as u8));
            }
            other => return Err(PatternError::InvalidChar(other)),
        }
    }
    let len = result.len();
    if len == 0 || len > MAX_WORD_LEN {
        return Err(PatternError::InvalidLength(MAX_WORD_LEN, len));
    }
    Ok(result)
}

pub fn parse_letters(raw: &str) -> Result<Vec<u8>, PatternError> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for c in raw.chars() {
        if !c.is_ascii_alphabetic() {
            return Err(PatternError::InvalidChar(c));
        }
        let lower = c.to_ascii_lowercase() as u8;
        if seen.insert(lower) {
            result.push(lower);
        }
    }
    Ok(result)
}

pub fn parse_letter_bag(raw: &str, expected_len: usize) -> Result<[u8; ALPHABET], PatternError> {
    let mut counts = [0u8; ALPHABET];
    let mut seen_len = 0usize;
    for c in raw.chars() {
        if !c.is_ascii_alphabetic() {
            return Err(PatternError::InvalidChar(c));
        }
        let lower = c.to_ascii_lowercase() as u8;
        counts[(lower - b'a') as usize] = counts[(lower - b'a') as usize].saturating_add(1);
        seen_len += 1;
    }
    if seen_len != expected_len {
        return Err(PatternError::InvalidLength(expected_len, seen_len));
    }
    Ok(counts)
}

#[derive(Debug, Error)]
pub enum PatternError {
    #[error("invalid character in pattern: {0}")]
    InvalidChar(char),
    #[error("pattern length must be between 1 and {0}, got {1}")]
    InvalidLength(usize, usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_index(words: &[&str]) -> Arc<WordIndex> {
        let mut file = NamedTempFile::new().expect("temp file");
        for word in words {
            writeln!(file, "{word}").unwrap();
        }
        WordIndex::build_from_file(file.path()).expect("build index")
    }

    #[test]
    fn parses_patterns_with_blanks() {
        let parsed = parse_pattern("A__le").unwrap();
        assert_eq!(parsed.len(), 5);
        assert_eq!(parsed[0], Some(b'a'));
        assert_eq!(parsed[1], None);
        let parsed_dots = parse_pattern("a..le").unwrap();
        assert_eq!(parsed_dots[3], Some(b'l'));
        assert!(parse_pattern("").is_err());
    }

    #[test]
    fn parse_pattern_rejects_invalid_chars() {
        assert!(parse_pattern("a1b").is_err());
        assert!(parse_pattern("🙂🙂").is_err());
    }

    #[test]
    fn parse_letter_bag_enforces_length() {
        assert!(parse_letter_bag("abcd", 3).is_err());
        assert!(parse_letter_bag("abc", 3).is_ok());
    }

    #[test]
    fn matches_words_by_pattern() {
        let index = make_index(&["apple", "ample", "apply", "ankle", "angle", "addle"]);
        let pattern = parse_pattern("a__le").unwrap();
        let result = index.query(QueryParams {
            pattern: &pattern,
            must_include: &[],
            cannot_include: &[],
            page: 1,
            page_size: 10,
        });
        assert_eq!(result.total, 5);
        assert!(result.items.contains(&"apple".to_string()));
        assert!(result.items.contains(&"angle".to_string()));
    }

    #[test]
    fn enforces_must_and_cannot_include() {
        let index = make_index(&["apple", "ample", "apply", "ankle", "angle"]);
        let pattern = parse_pattern("a__le").unwrap();
        let must = parse_letters("p").unwrap();
        let result = index.query(QueryParams {
            pattern: &pattern,
            must_include: &must,
            cannot_include: &[],
            page: 1,
            page_size: 10,
        });
        assert_eq!(result.total, 2);

        let cannot = parse_letters("n").unwrap();
        let result = index.query(QueryParams {
            pattern: &pattern,
            must_include: &[],
            cannot_include: &cannot,
            page: 1,
            page_size: 10,
        });
        assert!(!result.items.iter().any(|w| w.contains('n')));
    }

    #[test]
    fn paginates_stably() {
        let index = make_index(&["apple", "ample", "apply", "ankle", "angle", "addle"]);
        let pattern = parse_pattern("a____").unwrap();
        let first_page = index.query(QueryParams {
            pattern: &pattern,
            must_include: &[],
            cannot_include: &[],
            page: 1,
            page_size: 2,
        });
        let second_page = index.query(QueryParams {
            pattern: &pattern,
            must_include: &[],
            cannot_include: &[],
            page: 2,
            page_size: 2,
        });
        assert!(first_page.has_more);
        assert_eq!(first_page.items.len(), 2);
        assert_eq!(second_page.items.len(), 2);
        assert_ne!(first_page.items, second_page.items);
    }

    #[test]
    fn finds_anagrams_with_pattern() {
        let index = make_index(&["listen", "silent", "enlist", "tinsel", "inlets", "tile"]);
        let pattern = parse_pattern("______").unwrap();
        let bag = parse_letter_bag("listen", 6).unwrap();
        let result = index.query_anagram(AnagramParams {
            pattern: &pattern,
            bag_counts: bag,
            page: 1,
            page_size: 10,
        });
        assert!(result.items.contains(&"silent".to_string()));
        assert!(result.items.contains(&"listen".to_string()));
        assert_eq!(result.total, 5);
    }

    #[test]
    fn finds_specific_anagram_with_fixed_letters() {
        let index = make_index(&["manchego", "megachon", "comehang", "mango", "chemo"]);
        let pattern = parse_pattern("m______o").unwrap(); // first letter m, last letter o
        let bag = parse_letter_bag("comehang", 8).unwrap();
        let result = index.query_anagram(AnagramParams {
            pattern: &pattern,
            bag_counts: bag,
            page: 1,
            page_size: 10,
        });
        assert!(result.items.contains(&"manchego".to_string()));
        assert_eq!(result.total, 1);
    }
}
