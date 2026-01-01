use std::path::PathBuf;

use wordnet_db::WordNet;
use wordnet_types::{Pos, SynsetId, SynsetType};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("wn")
}

#[test]
fn parses_index_with_full_fields() {
    let wn = WordNet::load(fixture_dir()).expect("load fixtures");
    let entry = wn
        .index_entry(Pos::Noun, "dog")
        .expect("dog index entry present");
    assert_eq!(entry.lemma, "dog");
    assert_eq!(entry.synset_cnt, 1);
    assert_eq!(entry.p_cnt, 1);
    assert_eq!(entry.ptr_symbols, vec!["@"]);
    assert_eq!(entry.sense_cnt, 1);
    assert_eq!(entry.tagsense_cnt, 1);
    assert_eq!(entry.synset_offsets, &[1740]);
}

#[test]
fn parses_synset_with_pointers_frames_and_gloss() {
    let wn = WordNet::load(fixture_dir()).expect("load fixtures");
    let synset = wn
        .get_synset(SynsetId {
            pos: Pos::Noun,
            offset: 1740,
        })
        .expect("synset present");

    assert_eq!(synset.lex_filenum, 3);
    assert_eq!(synset.synset_type, SynsetType::Noun);
    assert_eq!(synset.words.len(), 2);
    assert_eq!(synset.words[0].text, "dog");
    assert_eq!(synset.words[1].lex_id, 0x01);

    assert_eq!(synset.pointers.len(), 1);
    let ptr = &synset.pointers[0];
    assert_eq!(ptr.symbol, "@");
    assert_eq!(ptr.target.offset, 2140);
    assert_eq!(ptr.src_word, Some(1));
    assert_eq!(ptr.dst_word, Some(1));

    assert!(synset.gloss.raw.contains("domestic animal"));
    assert!(synset.gloss.definition.starts_with("domestic animal"));
    assert_eq!(synset.gloss.examples, vec!["a pet dog"]);
}

#[test]
fn parses_verb_frames() {
    let wn = WordNet::load(fixture_dir()).expect("load fixtures");
    let synset = wn
        .get_synset(SynsetId {
            pos: Pos::Verb,
            offset: 2500,
        })
        .expect("verb synset");
    assert_eq!(synset.frames.len(), 2);
    assert_eq!(synset.frames[0].frame_number, 1);
    assert_eq!(synset.frames[0].word_number, Some(1));
    assert_eq!(synset.frames[1].word_number, None);
}
