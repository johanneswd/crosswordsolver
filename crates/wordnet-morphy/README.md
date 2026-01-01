# wordnet-morphy

WordNet-style morphological processing (morphy) for lemmatization. It checks exception lists, applies POS-specific suffix rules, and verifies candidates through a caller-supplied predicate (typically backed by [`wordnet-db`](https://crates.io/crates/wordnet-db)) while sharing POS types from [`wordnet-types`](https://crates.io/crates/wordnet-types).

## Why it's fast
- Pre-parsed exception files (`*.exc`) cached in memory for O(1) lookups.
- Fixed suffix rule tables per POS keep candidate generation tight and branch-light.
- No allocations for unchanged surface forms; minimal cloning otherwise, with normalization kept simple.

## What you can do
- Lemmatize inflected forms into canonical WordNet lemmas (e.g., “running” → “run”).
- Carry provenance for each candidate (surface, exception, or rule-based) to inform downstream ranking.
- Plug into any loader via the `lemma_exists` callback—commonly `WordNet::lemma_exists` from `wordnet-db`.

## Related crates
- [`wordnet-db`](https://crates.io/crates/wordnet-db): supplies the lemma existence predicate used to confirm morphy candidates.
- [`wordnet-types`](https://crates.io/crates/wordnet-types): provides the shared `Pos` enum and other types used in morphy APIs.

## Docs
- API reference: https://docs.rs/wordnet-morphy
