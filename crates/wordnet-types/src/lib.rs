//! Shared, zero-copy types that mirror WordNet's dictionary format.
//!
//! The goal is to expose the exact fields found in `data.*`/`index.*` while
//! making it cheap to build higher-level tooling. Text fields borrow from a
//! backing buffer (`&str`); numeric fields keep their raw representation
//! (`offset`, `lex_id`, `ss_type`, pointer source/target indices).
//!
//! Use [`Pos`] and [`SynsetId`] to key into a database, [`Synset`] and
//! [`IndexEntry`] to inspect parsed records, and helpers like [`decode_st`] to
//! interpret pointer source/target pairs.
//!
//! ```rust
//! use wordnet_types::{Pos, SynsetId, decode_st};
//!
//! let pos = Pos::from_char('n').unwrap();
//! let id = SynsetId { pos, offset: 1740 };
//! assert_eq!(decode_st("0a0b"), (Some(10), Some(11)));
//! ```

use std::fmt;

/// Part-of-speech marker as used by WordNet files (`n`, `v`, `a`/`s`, `r`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Pos {
    Noun,
    Verb,
    Adj,
    Adv,
}

impl Pos {
    /// Parse a WordNet POS character into an enum.
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'n' => Some(Pos::Noun),
            'v' => Some(Pos::Verb),
            'a' | 's' => Some(Pos::Adj),
            'r' => Some(Pos::Adv),
            _ => None,
        }
    }

    /// Emit the POS character used in `index.*`/`data.*`.
    pub fn to_char(self) -> char {
        match self {
            Pos::Noun => 'n',
            Pos::Verb => 'v',
            Pos::Adj => 'a',
            Pos::Adv => 'r',
        }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Pos::Noun => "noun",
            Pos::Verb => "verb",
            Pos::Adj => "adj",
            Pos::Adv => "adv",
        })
    }
}

/// `(offset, pos)` pair uniquely identifying a synset within the WordNet files.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SynsetId {
    pub pos: Pos,
    pub offset: u32,
}

/// Raw `ss_type` marker from `data.*`, including adjective satellites.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SynsetType {
    Noun,
    Verb,
    Adj,
    Adv,
    AdjSatellite,
}

impl SynsetType {
    /// Parse the `ss_type` character from a data line.
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'n' => Some(SynsetType::Noun),
            'v' => Some(SynsetType::Verb),
            'a' => Some(SynsetType::Adj),
            's' => Some(SynsetType::AdjSatellite),
            'r' => Some(SynsetType::Adv),
            _ => None,
        }
    }
}

/// A lemma string and its per-synset `lex_id`.
#[derive(Clone, Debug)]
pub struct Lemma<'a> {
    pub text: &'a str,
    pub lex_id: u8,
}

/// Verb frame (`f_cnt`) entry describing example template applicability.
#[derive(Clone, Debug)]
pub struct Frame {
    pub frame_number: u16,
    pub word_number: Option<u16>,
}

/// Pointer metadata from `p_cnt` section.
#[derive(Clone, Debug)]
pub struct Pointer<'a> {
    pub symbol: &'a str,
    pub target: SynsetId,
    pub src_word: Option<u16>,
    pub dst_word: Option<u16>,
}

/// Parsed gloss with convenience helpers while keeping the raw text intact.
#[derive(Clone, Debug)]
pub struct Gloss<'a> {
    pub raw: &'a str,
    pub definition: &'a str,
    pub examples: Vec<&'a str>,
}

/// Complete synset record with all semantic fields preserved.
#[derive(Clone, Debug)]
pub struct Synset<'a> {
    pub id: SynsetId,
    pub lex_filenum: u8,
    pub synset_type: SynsetType,
    pub words: Vec<Lemma<'a>>,
    pub pointers: Vec<Pointer<'a>>,
    pub frames: &'a [Frame],
    pub gloss: Gloss<'a>,
}

/// Index record from `index.*`, including sense and tagsense counts.
#[derive(Clone, Debug)]
pub struct IndexEntry<'a> {
    pub lemma: &'a str,
    pub pos: Pos,
    pub synset_cnt: u32,
    pub p_cnt: u32,
    pub ptr_symbols: Vec<&'a str>,
    pub sense_cnt: u32,
    pub tagsense_cnt: u32,
    pub synset_offsets: &'a [u32],
}

/// Decode the four-hex source/target field used in pointer blocks.
///
/// High byte is the source word number, low byte is the target word number.
/// Zero indicates "not specified" per WordNet conventions.
pub fn decode_st(hex4: &str) -> (Option<u16>, Option<u16>) {
    if hex4.len() != 4 {
        return (None, None);
    }

    match u16::from_str_radix(hex4, 16) {
        Ok(val) => {
            let src = (val >> 8) as u16;
            let dst = (val & 0x00FF) as u16;
            let src = if src == 0 { None } else { Some(src) };
            let dst = if dst == 0 { None } else { Some(dst) };
            (src, dst)
        }
        Err(_) => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_source_target() {
        assert_eq!(decode_st("0000"), (None, None));
        assert_eq!(decode_st("0100"), (Some(1), None));
        assert_eq!(decode_st("00ff"), (None, Some(255)));
        assert_eq!(decode_st("0a0b"), (Some(10), Some(11)));
        assert_eq!(decode_st("bad"), (None, None));
    }
}
