# wordnet-types

Zero-copy Rust types that mirror WordNet's `data.*` and `index.*` records. These structs back [`wordnet-db`](https://crates.io/crates/wordnet-db) for loading dictionaries and pair with [`wordnet-morphy`](https://crates.io/crates/wordnet-morphy) to describe POS information during lemmatization.

## Why it's fast
- Text is borrowed (`&str`) from the original files; no cloning or normalization on load.
- Numeric fields stay in their raw WordNet representation (`offset`, `lex_id`, `ss_type`), avoiding conversions.
- Plain data-holding structs mean zero I/O and zero parsing overhead here; the crate is just shared layout.

## Related crates
- [`wordnet-db`](https://crates.io/crates/wordnet-db): loads WordNet dictionaries using these types.
- [`wordnet-morphy`](https://crates.io/crates/wordnet-morphy): uses `Pos` and other types while emitting lemma candidates.

## Docs
- API reference: https://docs.rs/wordnet-types
