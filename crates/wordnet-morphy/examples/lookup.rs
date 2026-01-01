use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use wordnet_db::{LoadMode, WordNet};
use wordnet_morphy::Morphy;
use wordnet_types::Pos;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let dict_dir = args.next().map(PathBuf::from).context(
        "usage: cargo run -p wordnet-morphy --example lookup -- <dict-dir> [--demo | <word>]",
    )?;
    let next = args.next();
    let mode = args.next();
    if mode.is_some() {
        bail!("too many arguments");
    }

    let demo_words: Vec<String> = if let Some(arg) = next {
        if arg == "--demo" {
            vec![
                "running".into(),
                "better".into(),
                "children".into(),
                "dogs".into(),
                "happiest".into(),
            ]
        } else {
            vec![arg]
        }
    } else {
        bail!(
            "usage: cargo run -p wordnet-morphy --example lookup -- <dict-dir> [--demo | <word>]"
        );
    };

    let wn = WordNet::load_with_mode(&dict_dir, LoadMode::Mmap)
        .with_context(|| format!("loading WordNet from {}", dict_dir.display()))?;
    let morph = Morphy::load(&dict_dir)
        .with_context(|| format!("loading exceptions from {}", dict_dir.display()))?;

    println!("Dictionary: {}", dict_dir.display());

    for word in demo_words {
        println!("\nSurface: {}", word);
        for pos in [Pos::Noun, Pos::Verb, Pos::Adj, Pos::Adv] {
            let candidates = morph.lemmas_for(pos, &word, |p, lemma| wn.lemma_exists(p, lemma));
            if candidates.is_empty() {
                continue;
            }
            println!("  {:?}:", pos);
            for cand in candidates {
                println!("    {:<10} [{:?}]", cand.lemma, cand.source);
            }
        }
    }

    Ok(())
}
