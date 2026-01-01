//! Load WordNet dictionaries with full fidelity and zero-copy text.
//!
//! This crate ingests the canonical `data.*`/`index.*` files, preserves every
//! field (`lex_id`, `ss_type`, pointer source/target indices, verb frames),
//! and exposes borrowed `&str` slices for all text. Callers choose between
//! memory-mapped files or owned buffers at runtime via [`LoadMode`].
//!
//! Public access is intentionally read-only (no `pub` fields), leaving room to
//! evolve internal storage while keeping a stable API surface.
//!
//! # Features
//! - Zero-copy text: lemmas, pointer symbols, glosses, and indices borrow from
//!   the original bytes.
//! - Full-fidelity parsing: retains raw offsets, satellite adjectives, frames,
//!   and pointer source/target indices.
//! - Runtime backing choice: switch between mmap and owned buffers with
//!   [`LoadMode::Mmap`] / [`LoadMode::Owned`].
//! - Convenience lookups: lemma existence, index entries, synset fetching,
//!   and a streaming iterator over all synsets.
//!
//! # Example
//! ```no_run
//! use wordnet_db::{LoadMode, WordNet};
//! use wordnet_types::Pos;
//!
//! # fn main() -> anyhow::Result<()> {
//! let wn = WordNet::load_with_mode("/path/to/wordnet", LoadMode::Mmap)?;
//! let dog_index = wn.index_entry(Pos::Noun, "dog").expect("dog in index");
//! println!("dog synsets: {:?}", dog_index.synset_offsets);
//!
//! for sid in wn.synsets_for_lemma(Pos::Noun, "dog") {
//!     let syn = wn.get_synset(*sid).unwrap();
//!     println!("{}: {}", syn.id.offset, syn.gloss.definition);
//! }
//! # Ok(()) }
//! ```
//!
//! For a runnable demo, see `cargo run -p wordnet-db --example stats -- <dict>`.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use memmap2::Mmap;
use wordnet_types::{
    Frame, Gloss, IndexEntry, Lemma, Pointer, Pos, Synset, SynsetId, SynsetType, decode_st,
};

/// Strategy for loading dictionary files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadMode {
    /// Memory-map each WordNet file (fast, zero-copy).
    Mmap,
    /// Read each file into an owned buffer (portable fallback).
    Owned,
}

enum Buffer {
    Mmap(Mmap),
    Owned(Vec<u8>),
}

impl Buffer {
    fn as_slice(&self) -> &[u8] {
        match self {
            Buffer::Mmap(m) => m.as_ref(),
            Buffer::Owned(v) => v.as_slice(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum FileKind {
    DataNoun,
    DataVerb,
    DataAdj,
    DataAdv,
    IndexNoun,
    IndexVerb,
    IndexAdj,
    IndexAdv,
    Frames,
    Cntlist,
}

#[derive(Clone, Copy)]
struct TextRef {
    file: FileKind,
    start: usize,
    len: usize,
}

struct DictFiles {
    data_noun: Buffer,
    data_verb: Buffer,
    data_adj: Buffer,
    data_adv: Buffer,
    index_noun: Buffer,
    index_verb: Buffer,
    index_adj: Buffer,
    index_adv: Buffer,
    frames: Option<Buffer>,
    cntlist: Option<Buffer>,
}

impl DictFiles {
    fn load(dict_dir: &Path, mode: LoadMode) -> Result<Self> {
        let data_noun = load_file(dict_dir.join("data.noun"), mode)?;
        let data_verb = load_file(dict_dir.join("data.verb"), mode)?;
        let data_adj = load_file(dict_dir.join("data.adj"), mode)?;
        let data_adv = load_file(dict_dir.join("data.adv"), mode)?;
        let index_noun = load_file(dict_dir.join("index.noun"), mode)?;
        let index_verb = load_file(dict_dir.join("index.verb"), mode)?;
        let index_adj = load_file(dict_dir.join("index.adj"), mode)?;
        let index_adv = load_file(dict_dir.join("index.adv"), mode)?;
        let frames = load_optional_file(dict_dir.join("frames.vrb"), mode)?;
        let cntlist = load_optional_file(dict_dir.join("cntlist.rev"), mode)?;

        Ok(Self {
            data_noun,
            data_verb,
            data_adj,
            data_adv,
            index_noun,
            index_verb,
            index_adj,
            index_adv,
            frames,
            cntlist,
        })
    }

    fn bytes(&self, file: FileKind) -> &[u8] {
        match file {
            FileKind::DataNoun => self.data_noun.as_slice(),
            FileKind::DataVerb => self.data_verb.as_slice(),
            FileKind::DataAdj => self.data_adj.as_slice(),
            FileKind::DataAdv => self.data_adv.as_slice(),
            FileKind::IndexNoun => self.index_noun.as_slice(),
            FileKind::IndexVerb => self.index_verb.as_slice(),
            FileKind::IndexAdj => self.index_adj.as_slice(),
            FileKind::IndexAdv => self.index_adv.as_slice(),
            FileKind::Frames => self.frames.as_ref().map(Buffer::as_slice).unwrap_or(&[]),
            FileKind::Cntlist => self.cntlist.as_ref().map(Buffer::as_slice).unwrap_or(&[]),
        }
    }

    fn text(&self, r: TextRef) -> &str {
        let bytes = self.bytes(r.file);
        let slice = &bytes[r.start..r.start + r.len];
        std::str::from_utf8(slice).expect("wordnet text is valid utf8")
    }
}

struct LemmaData {
    text: TextRef,
    lex_id: u8,
}

struct PointerData {
    symbol: TextRef,
    target: SynsetId,
    src_word: Option<u16>,
    dst_word: Option<u16>,
}

struct GlossData {
    raw: TextRef,
    definition: TextRef,
    examples: Vec<TextRef>,
}

struct SynsetData {
    id: SynsetId,
    lex_filenum: u8,
    synset_type: SynsetType,
    words: Vec<LemmaData>,
    pointers: Vec<PointerData>,
    frames: Vec<Frame>,
    gloss: GlossData,
}

struct IndexEntryData {
    lemma: TextRef,
    synset_cnt: u32,
    p_cnt: u32,
    ptr_symbols: Vec<TextRef>,
    sense_cnt: u32,
    tagsense_cnt: u32,
    synset_offsets: Vec<u32>,
}

/// In-memory view of a WordNet dictionary backed by mmap or owned buffers.
pub struct WordNet {
    files: DictFiles,
    index: HashMap<(Pos, String), IndexEntryData>,
    synsets: HashMap<SynsetId, SynsetData>,
    lemma_to_synsets: HashMap<(Pos, String), Vec<SynsetId>>,
    verb_frames_text: HashMap<u16, TextRef>,
    sense_counts: HashMap<(String, Pos, u32), u32>,
}

impl WordNet {
    /// Load WordNet from a directory containing `data.*` and `index.*` files.
    ///
    /// Defaults to memory-mapping the source files. Use [`load_with_mode`] to
    /// force owned buffers instead.
    pub fn load(dict_dir: impl AsRef<Path>) -> Result<Self> {
        Self::load_with_mode(dict_dir, LoadMode::Mmap)
    }

    /// Load WordNet choosing between mmap and owned buffers at runtime.
    pub fn load_with_mode(dict_dir: impl AsRef<Path>, mode: LoadMode) -> Result<Self> {
        let dir = dict_dir.as_ref();
        let required = [
            "data.noun",
            "data.verb",
            "data.adj",
            "data.adv",
            "index.noun",
            "index.verb",
            "index.adj",
            "index.adv",
        ];
        for name in &required {
            let path = dir.join(name);
            if !path.exists() {
                anyhow::bail!("missing required WordNet file: {}", path.display());
            }
        }

        let files = DictFiles::load(dir, mode)?;

        let mut index = HashMap::new();
        let mut lemma_to_synsets = HashMap::new();
        parse_index(
            files.bytes(FileKind::IndexNoun),
            FileKind::IndexNoun,
            Pos::Noun,
            &mut index,
            &mut lemma_to_synsets,
        )?;
        parse_index(
            files.bytes(FileKind::IndexVerb),
            FileKind::IndexVerb,
            Pos::Verb,
            &mut index,
            &mut lemma_to_synsets,
        )?;
        parse_index(
            files.bytes(FileKind::IndexAdj),
            FileKind::IndexAdj,
            Pos::Adj,
            &mut index,
            &mut lemma_to_synsets,
        )?;
        parse_index(
            files.bytes(FileKind::IndexAdv),
            FileKind::IndexAdv,
            Pos::Adv,
            &mut index,
            &mut lemma_to_synsets,
        )?;

        let mut synsets = HashMap::new();
        parse_data(
            files.bytes(FileKind::DataNoun),
            FileKind::DataNoun,
            Pos::Noun,
            &mut synsets,
        )?;
        parse_data(
            files.bytes(FileKind::DataVerb),
            FileKind::DataVerb,
            Pos::Verb,
            &mut synsets,
        )?;
        parse_data(
            files.bytes(FileKind::DataAdj),
            FileKind::DataAdj,
            Pos::Adj,
            &mut synsets,
        )?;
        parse_data(
            files.bytes(FileKind::DataAdv),
            FileKind::DataAdv,
            Pos::Adv,
            &mut synsets,
        )?;

        let verb_frames_text = parse_frames_vrb(files.bytes(FileKind::Frames));
        let sense_counts = parse_cntlist(files.bytes(FileKind::Cntlist));

        Ok(Self {
            files,
            index,
            synsets,
            lemma_to_synsets,
            verb_frames_text,
            sense_counts,
        })
    }

    /// Check whether a lemma exists for the given POS according to index files.
    pub fn lemma_exists(&self, pos: Pos, lemma: &str) -> bool {
        let key = (pos, normalize_lemma(lemma));
        self.lemma_to_synsets.contains_key(&key)
    }

    /// Fetch a raw `IndexEntry` if present.
    pub fn index_entry(&self, pos: Pos, lemma: &str) -> Option<IndexEntry<'_>> {
        let key = (pos, normalize_lemma(lemma));
        self.index.get(&key).map(|entry| IndexEntry {
            lemma: self.files.text(entry.lemma),
            pos,
            synset_cnt: entry.synset_cnt,
            p_cnt: entry.p_cnt,
            ptr_symbols: entry
                .ptr_symbols
                .iter()
                .map(|r| self.files.text(*r))
                .collect(),
            sense_cnt: entry.sense_cnt,
            tagsense_cnt: entry.tagsense_cnt,
            synset_offsets: entry.synset_offsets.as_slice(),
        })
    }

    /// Return the synsets associated with a lemma, or an empty slice.
    pub fn synsets_for_lemma(&self, pos: Pos, lemma: &str) -> &[SynsetId] {
        static EMPTY: [SynsetId; 0] = [];
        let key = (pos, normalize_lemma(lemma));
        self.lemma_to_synsets
            .get(&key)
            .map(|v| v.as_slice())
            .unwrap_or(&EMPTY)
    }

    /// Fetch a `Synset` by id if loaded.
    pub fn get_synset(&self, id: SynsetId) -> Option<Synset<'_>> {
        self.synsets.get(&id).map(|syn| self.make_synset_view(syn))
    }

    /// Iterate over all synsets as borrowed views.
    pub fn iter_synsets(&self) -> impl Iterator<Item = Synset<'_>> + '_ {
        self.synsets.values().map(|s| self.make_synset_view(s))
    }

    /// Number of index entries.
    pub fn index_count(&self) -> usize {
        self.index.len()
    }

    /// Number of lemmas tracked across all parts of speech.
    pub fn lemma_count(&self) -> usize {
        self.lemma_to_synsets.len()
    }

    /// Number of synsets.
    pub fn synset_count(&self) -> usize {
        self.synsets.len()
    }

    /// Number of verb frame template strings loaded.
    pub fn verb_frame_templates_count(&self) -> usize {
        self.verb_frames_text.len()
    }

    /// Number of sense-count entries parsed from cntlist.
    pub fn sense_count_entries(&self) -> usize {
        self.sense_counts.len()
    }

    /// Sense frequency for a given lemma/pos/synset, if present in `cntlist.rev`.
    pub fn sense_count(&self, pos: Pos, lemma: &str, synset_offset: u32) -> Option<u32> {
        let normalized = normalize_lemma(lemma);
        let entry = self.index.get(&(pos, normalized.clone()))?;
        let sense_number = entry
            .synset_offsets
            .iter()
            .position(|off| *off == synset_offset)?;
        let sense_number = sense_number as u32 + 1;
        self.sense_counts
            .get(&(normalized, pos, sense_number))
            .copied()
    }

    fn make_synset_view<'a>(&'a self, data: &'a SynsetData) -> Synset<'a> {
        let words = data
            .words
            .iter()
            .map(|w| Lemma {
                text: self.files.text(w.text),
                lex_id: w.lex_id,
            })
            .collect();
        let pointers = data
            .pointers
            .iter()
            .map(|p| Pointer {
                symbol: self.files.text(p.symbol),
                target: p.target,
                src_word: p.src_word,
                dst_word: p.dst_word,
            })
            .collect();
        let gloss = Gloss {
            raw: self.files.text(data.gloss.raw),
            definition: self.files.text(data.gloss.definition),
            examples: data
                .gloss
                .examples
                .iter()
                .map(|r| self.files.text(*r))
                .collect(),
        };

        Synset {
            id: data.id,
            lex_filenum: data.lex_filenum,
            synset_type: data.synset_type,
            words,
            pointers,
            frames: data.frames.as_slice(),
            gloss,
        }
    }
}

fn load_file(path: PathBuf, mode: LoadMode) -> Result<Buffer> {
    match mode {
        LoadMode::Mmap => {
            let file = File::open(&path).with_context(|| format!("open {}", path.display()))?;
            unsafe { Mmap::map(&file) }
                .map(Buffer::Mmap)
                .with_context(|| format!("mmap {}", path.display()))
        }
        LoadMode::Owned => {
            let mut file = File::open(&path).with_context(|| format!("open {}", path.display()))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .with_context(|| format!("read {}", path.display()))?;
            Ok(Buffer::Owned(buf))
        }
    }
}

fn load_optional_file(path: PathBuf, mode: LoadMode) -> Result<Option<Buffer>> {
    if !path.exists() {
        return Ok(None);
    }
    load_file(path, mode).map(Some)
}

fn parse_index(
    bytes: &[u8],
    file: FileKind,
    pos: Pos,
    index: &mut HashMap<(Pos, String), IndexEntryData>,
    lemma_to_synsets: &mut HashMap<(Pos, String), Vec<SynsetId>>,
) -> Result<()> {
    for (lineno, raw_line) in bytes.split(|b| *b == b'\n').enumerate() {
        let line = strip_cr(raw_line);
        if line.is_empty() || matches!(line.first(), Some(b' ' | b'\t')) {
            continue;
        }
        let line_str = std::str::from_utf8(line)?;
        let tokens: Vec<&str> = line_str.split_ascii_whitespace().collect();
        if tokens.len() < 6 {
            anyhow::bail!(
                "{:?}:{} malformed index line (too few tokens)",
                file,
                lineno + 1
            );
        }

        let lemma_token = tokens[0];
        let lemma_ref = text_ref_str(file, bytes, lemma_token);
        let lemma_key = normalize_lemma(lemma_token);

        let synset_cnt: u32 = tokens[2]
            .parse()
            .with_context(|| format!("index {:?}:{} synset_cnt", file, lineno + 1))?;
        let p_cnt: u32 = tokens[3]
            .parse()
            .with_context(|| format!("index {:?}:{} p_cnt", file, lineno + 1))?;

        let expected_ptrs = p_cnt as usize;
        let mut idx = 4;
        if tokens.len() < idx + expected_ptrs {
            anyhow::bail!("{:?}:{} pointer count mismatch", file, lineno + 1);
        }
        let ptr_symbols = tokens[idx..idx + expected_ptrs]
            .iter()
            .map(|sym| text_ref_str(file, bytes, sym))
            .collect::<Vec<_>>();
        idx += expected_ptrs;
        if tokens.len() < idx + 2 {
            anyhow::bail!("{:?}:{} missing sense counts", file, lineno + 1);
        }
        let sense_cnt: u32 = tokens[idx]
            .parse()
            .with_context(|| format!("index {:?}:{} sense_cnt", file, lineno + 1))?;
        idx += 1;
        let tagsense_cnt: u32 = tokens[idx]
            .parse()
            .with_context(|| format!("index {:?}:{} tagsense_cnt", file, lineno + 1))?;
        idx += 1;

        let offsets: Vec<u32> = tokens[idx..]
            .iter()
            .map(|t| {
                t.parse::<u32>()
                    .with_context(|| format!("index {:?}:{} synset_offsets", file, lineno + 1))
            })
            .collect::<Result<_>>()?;
        if offsets.len() != synset_cnt as usize {
            anyhow::bail!(
                "{:?}:{} synset_cnt mismatch (expected {}, got {})",
                file,
                lineno + 1,
                synset_cnt,
                offsets.len()
            );
        }

        index.insert(
            (pos, lemma_key.clone()),
            IndexEntryData {
                lemma: lemma_ref,
                synset_cnt,
                p_cnt,
                ptr_symbols,
                sense_cnt,
                tagsense_cnt,
                synset_offsets: offsets.clone(),
            },
        );
        lemma_to_synsets.insert(
            (pos, lemma_key),
            offsets
                .into_iter()
                .map(|offset| SynsetId { pos, offset })
                .collect(),
        );
    }

    Ok(())
}

fn parse_data(
    bytes: &[u8],
    file: FileKind,
    pos: Pos,
    synsets: &mut HashMap<SynsetId, SynsetData>,
) -> Result<()> {
    for (lineno, raw_line) in bytes.split(|b| *b == b'\n').enumerate() {
        let line = strip_cr(raw_line);
        if line.is_empty() || matches!(line.first(), Some(b' ' | b'\t')) {
            continue;
        }
        let line_str = std::str::from_utf8(line)?;
        let (left, gloss_part) = match line_str.split_once('|') {
            Some((l, r)) => (l.trim(), r.trim()),
            None => (line_str.trim(), ""),
        };

        let tokens: Vec<&str> = left.split_ascii_whitespace().collect();
        if tokens.len() < 4 {
            anyhow::bail!("{:?}:{} malformed data line", file, lineno + 1);
        }

        let offset: u32 = tokens[0]
            .parse()
            .with_context(|| format!("{:?}:{} offset", file, lineno + 1))?;
        let lex_filenum: u8 = tokens[1]
            .parse()
            .with_context(|| format!("{:?}:{} lex_filenum", file, lineno + 1))?;
        let ss_type_char = tokens[2]
            .chars()
            .next()
            .ok_or_else(|| anyhow::anyhow!("{:?}:{} missing ss_type", file, lineno + 1))?;
        let synset_type = SynsetType::from_char(ss_type_char).ok_or_else(|| {
            anyhow::anyhow!("{:?}:{} invalid ss_type {}", file, lineno + 1, ss_type_char)
        })?;
        let w_cnt: usize = usize::from_str_radix(tokens[3], 16)
            .with_context(|| format!("{:?}:{} w_cnt", file, lineno + 1))?;

        let mut idx = 4;
        if tokens.len() < idx + (w_cnt * 2) {
            anyhow::bail!("{:?}:{} not enough word/lex_id pairs", file, lineno + 1);
        }
        let mut words = Vec::with_capacity(w_cnt);
        for _ in 0..w_cnt {
            let text_token = tokens[idx];
            let lex_id_token = tokens[idx + 1];
            let lex_id: u8 = u8::from_str_radix(lex_id_token, 16)
                .with_context(|| format!("{:?}:{} lex_id", file, lineno + 1))?;
            words.push(LemmaData {
                text: text_ref_str(file, bytes, text_token),
                lex_id,
            });
            idx += 2;
        }

        if tokens.len() <= idx {
            anyhow::bail!("{:?}:{} missing pointer count", file, lineno + 1);
        }
        let p_cnt: usize = tokens[idx]
            .parse()
            .with_context(|| format!("{:?}:{} p_cnt", file, lineno + 1))?;
        idx += 1;

        let mut pointers = Vec::with_capacity(p_cnt);
        for _ in 0..p_cnt {
            if tokens.len() < idx + 4 {
                anyhow::bail!("{:?}:{} incomplete pointer block", file, lineno + 1);
            }
            let symbol = tokens[idx];
            let target_offset: u32 = tokens[idx + 1]
                .parse()
                .with_context(|| format!("{:?}:{} pointer target offset", file, lineno + 1))?;
            let target_pos = tokens[idx + 2]
                .chars()
                .next()
                .and_then(Pos::from_char)
                .ok_or_else(|| anyhow::anyhow!("{:?}:{} pointer target pos", file, lineno + 1))?;
            let (src_word, dst_word) = decode_st(tokens[idx + 3]);
            pointers.push(PointerData {
                symbol: text_ref_str(file, bytes, symbol),
                target: SynsetId {
                    pos: target_pos,
                    offset: target_offset,
                },
                src_word,
                dst_word,
            });
            idx += 4;
        }

        let mut frames = Vec::new();
        if matches!(pos, Pos::Verb) {
            let f_cnt: usize = if tokens.len() <= idx {
                0
            } else {
                let v: usize = tokens[idx]
                    .parse()
                    .with_context(|| format!("{:?}:{} f_cnt", file, lineno + 1))?;
                idx += 1;
                v
            };
            for _ in 0..f_cnt {
                if tokens.len() < idx + 3 {
                    anyhow::bail!("{:?}:{} incomplete frame entry", file, lineno + 1);
                }
                if tokens[idx] != "+" {
                    anyhow::bail!("{:?}:{} expected '+' before frame entry", file, lineno + 1);
                }
                let frame_number: u16 = tokens[idx + 1]
                    .parse()
                    .with_context(|| format!("{:?}:{} frame_number", file, lineno + 1))?;
                let word_number = parse_word_number(tokens[idx + 2]);
                frames.push(Frame {
                    frame_number,
                    word_number,
                });
                idx += 3;
            }
        }

        let gloss = parse_gloss(file, bytes, gloss_part)?;
        let id = SynsetId { pos, offset };
        synsets.insert(
            id,
            SynsetData {
                id,
                lex_filenum,
                synset_type,
                words,
                pointers,
                frames,
                gloss,
            },
        );
    }

    Ok(())
}

fn parse_gloss(file: FileKind, root: &[u8], gloss: &str) -> Result<GlossData> {
    let trimmed = gloss.trim();
    let gloss_raw = text_ref_str(file, root, trimmed);

    let mut examples = Vec::new();
    let mut in_quote = false;
    let mut quote_start: Option<usize> = None;
    let mut def_end = trimmed.len();
    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '"' => {
                if in_quote {
                    if let Some(start) = quote_start.take()
                        && idx > start + 1
                    {
                        let start_bytes =
                            trimmed.as_ptr() as usize + start + 1 - root.as_ptr() as usize;
                        examples.push(TextRef {
                            file,
                            start: start_bytes,
                            len: idx - start - 1,
                        });
                    }
                } else {
                    quote_start = Some(idx);
                }
                in_quote = !in_quote;
            }
            ';' if !in_quote && def_end == trimmed.len() => {
                def_end = idx;
            }
            _ => {}
        }
    }

    let definition_slice = trimmed[..def_end].trim();
    let def_start = definition_slice.as_ptr() as usize - trimmed.as_ptr() as usize;

    let definition = TextRef {
        file,
        start: trimmed.as_ptr() as usize + def_start - root.as_ptr() as usize,
        len: definition_slice.len(),
    };

    Ok(GlossData {
        raw: gloss_raw,
        definition,
        examples,
    })
}

fn parse_frames_vrb(bytes: &[u8]) -> HashMap<u16, TextRef> {
    let mut frames = HashMap::new();
    for (lineno, raw_line) in bytes.split(|b| *b == b'\n').enumerate() {
        let line = strip_cr(raw_line);
        if line.is_empty() {
            continue;
        }
        let line_str = match std::str::from_utf8(line) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut parts = line_str.splitn(2, ' ');
        let num = parts.next().and_then(|t| t.parse::<u16>().ok());
        let text = parts.next().map(str::trim).unwrap_or("");
        if let Some(n) = num {
            let start = text.as_ptr() as usize - bytes.as_ptr() as usize;
            frames.insert(
                n,
                TextRef {
                    file: FileKind::Frames,
                    start,
                    len: text.len(),
                },
            );
        } else {
            eprintln!("frames.vrb:{} invalid frame number", lineno + 1);
        }
    }
    frames
}

fn parse_cntlist(bytes: &[u8]) -> HashMap<(String, Pos, u32), u32> {
    let mut counts = HashMap::new();
    for raw_line in bytes.split(|b| *b == b'\n') {
        let line = strip_cr(raw_line);
        if line.is_empty() {
            continue;
        }
        let line_str = match std::str::from_utf8(line) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let tokens: Vec<&str> = line_str.split_ascii_whitespace().collect();
        if tokens.len() < 3 {
            continue;
        }
        let count: u32 = match tokens[0].parse() {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Real cntlist uses sense_key; here we accept `lemma pos sense` for flexibility.
        let lemma = normalize_lemma(tokens[1]);
        let pos = tokens[2]
            .chars()
            .next()
            .and_then(Pos::from_char)
            .unwrap_or(Pos::Noun);
        let sense_number: u32 = tokens.get(3).and_then(|t| t.parse().ok()).unwrap_or(1);
        counts.insert((lemma, pos, sense_number), count);
    }
    counts
}

fn text_ref_str(file: FileKind, root: &[u8], token: &str) -> TextRef {
    let start = token.as_ptr() as usize - root.as_ptr() as usize;
    TextRef {
        file,
        start,
        len: token.len(),
    }
}

fn strip_cr(line: &[u8]) -> &[u8] {
    if line.ends_with(b"\r") {
        &line[..line.len() - 1]
    } else {
        line
    }
}

fn parse_word_number(token: &str) -> Option<u16> {
    u16::from_str_radix(token, 16)
        .or_else(|_| token.parse::<u16>())
        .ok()
        .and_then(|v| if v == 0 { None } else { Some(v) })
}

fn normalize_lemma(text: &str) -> String {
    let mut s = text.trim().to_string();
    s.make_ascii_lowercase();
    s.replace(' ', "_")
}
