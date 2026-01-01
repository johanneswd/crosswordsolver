//! WordNet-style morphological processing (morphy).
//!
//! Faithful to the classic morphy algorithm: check exceptions, apply suffix
//! rules, and verify candidates via a caller-provided lemma existence
//! predicate. The crate is intentionally decoupled from any particular loader;
//! it only depends on `Pos` and the callback you supply.
//!
//! # How it works
//! 1. Emit the surface form if it exists.
//! 2. Check exceptions (`*.exc` files).
//! 3. Apply POS-specific suffix rules.
//! 4. Deduplicate while preserving provenance (`Surface`, `Exception`, `Rule`).
//!
//! # Example
//! ```no_run
//! use wordnet_db::{LoadMode, WordNet};
//! use wordnet_morphy::Morphy;
//! use wordnet_types::Pos;
//!
//! # fn main() -> anyhow::Result<()> {
//! let dict = "/path/to/wordnet";
//! let wn = WordNet::load_with_mode(dict, LoadMode::Mmap)?;
//! let morph = Morphy::load(dict)?;
//! let exists = |pos, lemma: &str| wn.lemma_exists(pos, lemma);
//!
//! let cands = morph.lemmas_for(Pos::Verb, "running", exists);
//! for cand in cands {
//!     println!("{:?}: {}", cand.source, cand.lemma);
//! }
//! # Ok(()) }
//! ```
//!
//! For a runnable demo, see `cargo run -p wordnet-morphy --example lookup -- <dict> [--demo|<word>]`.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use std::borrow::Cow;

use anyhow::{Context, Result};
use wordnet_types::Pos;

/// Where a candidate lemma originated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateSource {
    Surface,
    Exception,
    Rule {
        suffix: &'static str,
        replacement: &'static str,
    },
}

/// A lemma candidate paired with its POS and provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LemmaCandidate<'a> {
    pub pos: Pos,
    pub lemma: Cow<'a, str>,
    pub source: CandidateSource,
}

/// Minimal morphy implementation parameterised by caller-provided existence checks.
pub struct Morphy {
    exceptions: HashMap<Pos, HashMap<String, Vec<String>>>,
}

impl Morphy {
    /// Load morphy exception lists (`*.exc`) from a WordNet dict directory.
    ///
    /// Files are optional; missing ones are treated as empty.
    pub fn load(dict_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dict_dir.as_ref();
        Ok(Self {
            exceptions: HashMap::from([
                (Pos::Noun, load_exc(dir.join("noun.exc"))?),
                (Pos::Verb, load_exc(dir.join("verb.exc"))?),
                (Pos::Adj, load_exc(dir.join("adj.exc"))?),
                (Pos::Adv, load_exc(dir.join("adv.exc"))?),
            ]),
        })
    }

    /// Generate lemmas for a surface form, returning enriched provenance.
    ///
    /// The callback `lemma_exists` typically delegates to `WordNet::lemma_exists`
    /// so this crate stays ignorant of any concrete database layout.
    pub fn lemmas_for<'a, F>(
        &'a self,
        pos: Pos,
        surface: &str,
        lemma_exists: F,
    ) -> Vec<LemmaCandidate<'a>>
    where
        F: Fn(Pos, &str) -> bool,
    {
        let mut seen: HashSet<Cow<'a, str>> = HashSet::new();
        let mut out: Vec<LemmaCandidate<'a>> = Vec::new();
        let norm_surface = normalize(surface);

        // Surface form first if it exists.
        if lemma_exists(pos, &norm_surface) {
            push_unique(
                &mut out,
                &mut seen,
                LemmaCandidate {
                    pos,
                    lemma: Cow::Owned(norm_surface.clone()),
                    source: CandidateSource::Surface,
                },
            );
        }

        // Exceptions: may include multiple lemmas per surface form.
        if let Some(exc_map) = self.exceptions.get(&pos)
            && let Some(entries) = exc_map.get(&norm_surface)
        {
            for lemma in entries {
                if lemma_exists(pos, lemma) {
                    push_unique(
                        &mut out,
                        &mut seen,
                        LemmaCandidate {
                            pos,
                            lemma: Cow::Borrowed(lemma.as_str()),
                            source: CandidateSource::Exception,
                        },
                    );
                }
            }
        }

        // Rule-based guesses.
        for (suffix, replacement) in rules_for(pos) {
            if let Some(candidate) = apply_rule(&norm_surface, suffix, replacement)
                && lemma_exists(pos, &candidate)
            {
                push_unique(
                    &mut out,
                    &mut seen,
                    LemmaCandidate {
                        pos,
                        lemma: Cow::Owned(candidate),
                        source: CandidateSource::Rule {
                            suffix,
                            replacement,
                        },
                    },
                );
            }
        }

        out
    }
}

fn load_exc(path: PathBuf) -> Result<HashMap<String, Vec<String>>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let file =
        File::open(&path).with_context(|| format!("open exception file {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut map = HashMap::new();
    for (lineno, line) in reader.lines().enumerate() {
        let line =
            line.with_context(|| format!("read line {} in {}", lineno + 1, path.display()))?;
        let mut parts = line.split_whitespace();
        let surface = match parts.next() {
            Some(s) => normalize(s),
            None => continue,
        };
        let lemmas: Vec<String> = parts.map(normalize).collect();
        if !lemmas.is_empty() {
            map.insert(surface, lemmas);
        }
    }
    Ok(map)
}

fn normalize(text: &str) -> String {
    text.trim().to_lowercase().replace(' ', "_")
}

fn push_unique<'a>(
    out: &mut Vec<LemmaCandidate<'a>>,
    seen: &mut HashSet<Cow<'a, str>>,
    candidate: LemmaCandidate<'a>,
) {
    if seen.insert(candidate.lemma.clone()) {
        out.push(candidate);
    }
}

fn apply_rule(surface: &str, suffix: &str, replacement: &str) -> Option<String> {
    surface.strip_suffix(suffix).map(|stem| {
        let mut candidate = if replacement.is_empty() {
            stem.to_string()
        } else {
            format!("{stem}{replacement}")
        };

        // Handle doubled consonants from inflected forms (e.g. "running" -> "runn").
        if replacement.is_empty() && candidate.len() >= 2 {
            let mut chars = candidate.chars();
            let prev = chars.next_back();
            let last = chars.next_back();
            if let (Some(a), Some(b)) = (prev, last)
                && a == b
            {
                candidate.pop();
            }
        }

        candidate
    })
}

fn rules_for(pos: Pos) -> &'static [(&'static str, &'static str)] {
    match pos {
        Pos::Noun => &[
            ("s", ""),
            ("ses", "s"),
            ("xes", "x"),
            ("zes", "z"),
            ("ches", "ch"),
            ("shes", "sh"),
            ("men", "man"),
            ("ies", "y"),
        ],
        Pos::Verb => &[
            ("s", ""),
            ("ies", "y"),
            ("es", "e"),
            ("es", ""),
            ("ed", "e"),
            ("ed", ""),
            ("ing", "e"),
            ("ing", ""),
        ],
        Pos::Adj | Pos::Adv => &[("er", ""), ("er", "e"), ("est", ""), ("est", "e")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_exists(targets: &[(&str, Pos)]) -> impl Fn(Pos, &str) -> bool {
        let set: HashSet<(Pos, String)> = targets
            .iter()
            .map(|(lemma, pos)| (*pos, normalize(lemma)))
            .collect();
        move |pos, lemma| set.contains(&(pos, normalize(lemma)))
    }

    #[test]
    fn uses_exceptions_and_rules() {
        let mut morph = Morphy {
            exceptions: HashMap::new(),
        };
        morph.exceptions.insert(
            Pos::Noun,
            HashMap::from([("children".into(), vec!["child".into()])]),
        );

        let candidates =
            morph.lemmas_for(Pos::Noun, "children", fake_exists(&[("child", Pos::Noun)]));
        assert_eq!(candidates.len(), 1);
        assert!(matches!(candidates[0].source, CandidateSource::Exception));
        assert_eq!(candidates[0].lemma, "child");
    }

    #[test]
    fn includes_surface_and_rule_hits() {
        let morph = Morphy {
            exceptions: HashMap::new(),
        };
        let candidates = morph.lemmas_for(
            Pos::Verb,
            "running",
            fake_exists(&[("running", Pos::Verb), ("run", Pos::Verb)]),
        );
        assert_eq!(candidates.len(), 2);
        assert!(matches!(candidates[0].source, CandidateSource::Surface));
        assert!(matches!(candidates[1].source, CandidateSource::Rule { .. }));
    }
}
