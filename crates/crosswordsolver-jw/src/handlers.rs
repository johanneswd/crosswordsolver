use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use wordnet_db::WordNet;
use wordnet_morphy::Morphy;
use wordnet_types::{Pos, Synset, SynsetId};

use crate::index::{
    AnagramParams, MAX_WORD_LEN, QueryParams, WordIndex, parse_letter_bag, parse_letters,
    parse_pattern,
};

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<WordIndex>,
    pub wordnet: Arc<WordNet>,
    pub morphy: Arc<Morphy>,
    pub max_page_size: usize,
    pub disable_cache: bool,
}

#[derive(Deserialize)]
pub struct MatchesQuery {
    pub pattern: String,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub must_include: Option<String>,
    pub cannot_include: Option<String>,
}

#[derive(Deserialize)]
pub struct AnagramQuery {
    pub letters: String,
    pub pattern: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Serialize)]
pub struct MatchesResponse {
    pattern: String,
    page: usize,
    page_size: usize,
    total: usize,
    has_more: bool,
    items: Vec<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Deserialize)]
pub struct WordNetQuery {
    pub word: String,
    pub pos: Option<String>,
}

#[derive(Serialize, Clone)]
struct SynsetIdResponse {
    pos: char,
    offset: u32,
}

#[derive(Serialize)]
struct DictionarySynset {
    pos: String,
    synset_id: SynsetIdResponse,
    lemmas: Vec<String>,
    definition: String,
    examples: Vec<String>,
    sense_count: Option<u32>,
}

#[derive(Serialize)]
pub struct DictionaryResponse {
    word: String,
    normalized: String,
    lemmas: Vec<String>,
    results: Vec<DictionarySynset>,
    note: Option<String>,
}

#[derive(Serialize, Clone)]
struct RelatedTarget {
    pos: String,
    synset_id: SynsetIdResponse,
    lemmas: Vec<String>,
    definition: String,
    sense_count: Option<u32>,
}

#[derive(Serialize, Clone)]
struct RelationGroup {
    kind: String,
    label: String,
    symbol: String,
    targets: Vec<RelatedTarget>,
}

#[derive(Serialize, Clone)]
struct RelatedSynset {
    pos: String,
    synset_id: SynsetIdResponse,
    lemmas: Vec<String>,
    definition: String,
    examples: Vec<String>,
    sense_count: Option<u32>,
    relations: Vec<RelationGroup>,
}

#[derive(Serialize)]
struct RelatedResponse {
    word: String,
    normalized: String,
    lemmas: Vec<String>,
    synsets: Vec<RelatedSynset>,
    note: Option<String>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(frontend))
        .route("/anagrams", get(anagram_frontend))
        .route("/synonyms", get(synonyms_frontend))
        .route("/robots.txt", get(robots))
        .route("/healthz", get(healthz))
        .route("/v1/matches", get(matches))
        .route("/v1/anagrams", get(anagrams))
        .route("/v1/wordnet/dictionary", get(dictionary_lookup))
        .route("/v1/wordnet/related", get(related_words))
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    "ok"
}

async fn robots(State(state): State<AppState>) -> Response {
    let headers = axum::http::HeaderMap::from_iter([
        (
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        ),
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400, immutable"),
        ),
    ]);
    if state.disable_cache {
        return "User-agent: *\nDisallow: /".into_response();
    }
    (headers, "User-agent: *\nDisallow: /").into_response()
}

async fn frontend(State(state): State<AppState>) -> Response {
    let html = Html(index_html());
    if state.disable_cache {
        return html.into_response();
    }
    (
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=3600, immutable"),
        )],
        html,
    )
        .into_response()
}

async fn anagram_frontend(State(state): State<AppState>) -> Response {
    let html = Html(anagram_html());
    if state.disable_cache {
        return html.into_response();
    }
    (
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=3600, immutable"),
        )],
        html,
    )
        .into_response()
}

async fn synonyms_frontend(State(state): State<AppState>) -> Response {
    let html = Html(synonyms_html());
    if state.disable_cache {
        return html.into_response();
    }
    (
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=3600, immutable"),
        )],
        html,
    )
        .into_response()
}

async fn matches(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<MatchesQuery>,
) -> Result<Response, ApiError> {
    let pattern_vec =
        parse_pattern(&params.pattern).map_err(|e| ApiError::bad_request(e.to_string()))?;

    let page = params.page.unwrap_or(1);
    if page == 0 {
        return Err(ApiError::bad_request("page must be >= 1"));
    }
    let mut page_size = params.page_size.unwrap_or(50);
    if page_size == 0 {
        return Err(ApiError::bad_request("page_size must be >= 1"));
    }
    if page_size > state.max_page_size {
        page_size = state.max_page_size;
    }

    let must_include = params
        .must_include
        .map_or(Ok(Vec::new()), |s| parse_letters(&s))
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let cannot_include = params
        .cannot_include
        .map_or(Ok(Vec::new()), |s| parse_letters(&s))
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let result = state.index.query(QueryParams {
        pattern: &pattern_vec,
        must_include: &must_include,
        cannot_include: &cannot_include,
        page,
        page_size,
    });

    let response = MatchesResponse {
        pattern: params.pattern,
        page,
        page_size,
        total: result.total,
        has_more: result.has_more,
        items: result.items,
    };

    if state.disable_cache {
        Ok(Json(response).into_response())
    } else {
        Ok((
            [(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            )],
            Json(response),
        )
            .into_response())
    }
}

async fn dictionary_lookup(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<WordNetQuery>,
) -> Result<Response, ApiError> {
    let word = params.word.trim();
    if word.is_empty() {
        return Err(ApiError::bad_request("word is required"));
    }
    let normalized = word.to_ascii_lowercase();
    let pos_filter = parse_pos_filter(params.pos.as_deref())?;

    let mut seen_lemmas = HashSet::new();
    let mut lemmas = Vec::new();
    let mut synsets: HashMap<SynsetId, DictionarySynset> = HashMap::new();

    for pos in pos_filter {
        let candidates = state
            .morphy
            .lemmas_for(pos, word, |p, lemma| state.wordnet.lemma_exists(p, lemma));
        for cand in candidates {
            let lemma = cand.lemma.to_string();
            if seen_lemmas.insert(lemma.clone()) {
                lemmas.push(lemma.clone());
            }
            for sid in state.wordnet.synsets_for_lemma(pos, &lemma) {
                if let Some(syn) = state.wordnet.get_synset(*sid) {
                    let entry = synsets.entry(*sid).or_insert_with(|| DictionarySynset {
                        pos: pos_label(syn.id.pos).to_string(),
                        synset_id: synset_id_response(syn.id),
                        lemmas: syn.words.iter().map(|w| w.text.to_string()).collect(),
                        definition: syn.gloss.definition.to_string(),
                        examples: syn.gloss.examples.iter().map(|e| e.to_string()).collect(),
                        sense_count: None,
                    });
                    if let Some(count) = state.wordnet.sense_count(pos, &lemma, sid.offset) {
                        entry.sense_count = match entry.sense_count {
                            Some(existing) if existing >= count => Some(existing),
                            _ => Some(count),
                        };
                    }
                }
            }
        }
    }

    let mut results: Vec<_> = synsets.into_values().collect();
    results.sort_by(|a, b| {
        let sa = a.sense_count.unwrap_or(0);
        let sb = b.sense_count.unwrap_or(0);
        sb.cmp(&sa)
            .then_with(|| {
                pos_order(char_to_pos(a.synset_id.pos))
                    .cmp(&pos_order(char_to_pos(b.synset_id.pos)))
            })
            .then_with(|| a.synset_id.offset.cmp(&b.synset_id.offset))
    });

    let note = if results.is_empty() {
        Some(format!("no WordNet entries found for \"{word}\""))
    } else {
        None
    };

    let response = DictionaryResponse {
        word: word.to_string(),
        normalized,
        lemmas,
        results,
        note,
    };

    if state.disable_cache {
        Ok(Json(response).into_response())
    } else {
        Ok((
            [(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=3600"),
            )],
            Json(response),
        )
            .into_response())
    }
}

async fn related_words(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<WordNetQuery>,
) -> Result<Response, ApiError> {
    let word = params.word.trim();
    if word.is_empty() {
        return Err(ApiError::bad_request("word is required"));
    }
    let normalized = word.to_ascii_lowercase();
    let pos_filter = parse_pos_filter(params.pos.as_deref())?;

    let mut seen_lemmas = HashSet::new();
    let mut lemmas = Vec::new();
    let mut seen_synsets = HashSet::new();
    let mut synsets_out = Vec::new();

    for pos in pos_filter {
        let candidates = state
            .morphy
            .lemmas_for(pos, word, |p, lemma| state.wordnet.lemma_exists(p, lemma));
        for cand in candidates {
            let lemma = cand.lemma.to_string();
            if seen_lemmas.insert(lemma.clone()) {
                lemmas.push(lemma.clone());
            }
            for sid in state.wordnet.synsets_for_lemma(pos, &lemma) {
                if !seen_synsets.insert(*sid) {
                    continue;
                }
                if let Some(syn) = state.wordnet.get_synset(*sid) {
                    let sense_count = best_sense_count_for_synset(&state.wordnet, &syn, &lemmas);
                    let relations = collect_relations(&state.wordnet, &syn);
                    synsets_out.push(RelatedSynset {
                        pos: pos_label(syn.id.pos).to_string(),
                        synset_id: synset_id_response(syn.id),
                        lemmas: syn.words.iter().map(|w| w.text.to_string()).collect(),
                        definition: syn.gloss.definition.to_string(),
                        examples: syn.gloss.examples.iter().map(|e| e.to_string()).collect(),
                        sense_count,
                        relations,
                    });
                }
            }
        }
    }

    synsets_out.sort_by(|a, b| {
        let sa = a.sense_count.unwrap_or(0);
        let sb = b.sense_count.unwrap_or(0);
        sb.cmp(&sa)
            .then_with(|| {
                pos_order(char_to_pos(a.synset_id.pos))
                    .cmp(&pos_order(char_to_pos(b.synset_id.pos)))
            })
            .then_with(|| a.synset_id.offset.cmp(&b.synset_id.offset))
    });

    let note = if synsets_out.is_empty() {
        Some(format!("no WordNet entries found for \"{word}\""))
    } else {
        None
    };

    let response = RelatedResponse {
        word: word.to_string(),
        normalized,
        lemmas,
        synsets: synsets_out,
        note,
    };

    if state.disable_cache {
        Ok(Json(response).into_response())
    } else {
        Ok((
            [(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=1800"),
            )],
            Json(response),
        )
            .into_response())
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("internal server error")]
    Internal,
}

impl ApiError {
    fn bad_request<T: Into<String>>(msg: T) -> Self {
        ApiError::BadRequest(msg.into())
    }
}

async fn anagrams(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<AnagramQuery>,
) -> Result<Response, ApiError> {
    let letters = params.letters.trim();
    if letters.is_empty() {
        return Err(ApiError::bad_request("letters is required"));
    }
    if letters.len() > MAX_WORD_LEN {
        return Err(ApiError::bad_request(format!(
            "letters must be at most {MAX_WORD_LEN}"
        )));
    }

    let pattern_str = params.pattern.unwrap_or_else(|| "_".repeat(letters.len()));
    let pattern_vec =
        parse_pattern(&pattern_str).map_err(|e| ApiError::bad_request(e.to_string()))?;
    if pattern_vec.len() != letters.len() {
        return Err(ApiError::bad_request(
            "pattern length must match letters length",
        ));
    }
    let bag = parse_letter_bag(letters, letters.len())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let page = params.page.unwrap_or(1);
    if page == 0 {
        return Err(ApiError::bad_request("page must be >= 1"));
    }
    let mut page_size = params.page_size.unwrap_or(50);
    if page_size == 0 {
        return Err(ApiError::bad_request("page_size must be >= 1"));
    }
    if page_size > state.max_page_size {
        page_size = state.max_page_size;
    }

    // Reject patterns that require letters not available in the bag.
    let mut required_counts = [0u8; 26];
    for letter in pattern_vec.iter().flatten() {
        let idx = (*letter - b'a') as usize;
        required_counts[idx] = required_counts[idx].saturating_add(1);
        if required_counts[idx] > bag[idx] {
            return Err(ApiError::bad_request(
                "pattern requires letters not present in the bag",
            ));
        }
    }

    let result = state.index.query_anagram(AnagramParams {
        pattern: &pattern_vec,
        bag_counts: bag,
        page,
        page_size,
    });

    let response = MatchesResponse {
        pattern: pattern_str,
        page,
        page_size,
        total: result.total,
        has_more: result.has_more,
        items: result.items,
    };

    if state.disable_cache {
        Ok(Json(response).into_response())
    } else {
        Ok((
            [(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            )],
            Json(response),
        )
            .into_response())
    }
}

const BASE_HTML: &str = include_str!("../templates/base.html");
const STYLE_HTML: &str = include_str!("../templates/style.html");
const HEADER_HTML: &str = include_str!("../templates/header.html");
const FOOTER_HTML: &str = include_str!("../templates/footer.html");
const SOLVER_BODY_HTML: &str = include_str!("../templates/solver_body.html");
const ANAGRAM_BODY_HTML: &str = include_str!("../templates/anagram_body.html");
const SYNONYMS_BODY_HTML: &str = include_str!("../templates/synonyms_body.html");
const SOLVER_SCRIPT: &str = include_str!("../templates/solver_script.js");
const ANAGRAM_SCRIPT: &str = include_str!("../templates/anagram_script.js");
const SYNONYMS_SCRIPT: &str = include_str!("../templates/synonyms_script.js");

fn render_page(title: &str, body: &str, script: &str) -> String {
    let header = HEADER_HTML.replace("{{title}}", title);
    let base = BASE_HTML
        .replace("{{title}}", title)
        .replace("{{style}}", STYLE_HTML)
        .replace("{{header}}", &header)
        .replace("{{body}}", body)
        .replace("{{footer}}", FOOTER_HTML)
        .replace("{{scripts}}", &format!(r#"<script>{}</script>"#, script));
    base.replace("__MAX_LEN__", &MAX_WORD_LEN.to_string())
}

fn index_html() -> String {
    render_page("Crossword Solver", SOLVER_BODY_HTML, SOLVER_SCRIPT)
}

fn anagram_html() -> String {
    render_page("Anagram Solver", ANAGRAM_BODY_HTML, ANAGRAM_SCRIPT)
}

fn synonyms_html() -> String {
    render_page("Synonyms", SYNONYMS_BODY_HTML, SYNONYMS_SCRIPT)
}

fn parse_pos_filter(pos: Option<&str>) -> Result<Vec<Pos>, ApiError> {
    if let Some(p) = pos {
        let ch = p
            .chars()
            .next()
            .ok_or_else(|| ApiError::bad_request("pos is invalid"))?;
        let parsed = Pos::from_char(ch.to_ascii_lowercase())
            .ok_or_else(|| ApiError::bad_request("pos must be one of n|v|a|r"))?;
        Ok(vec![parsed])
    } else {
        Ok(vec![Pos::Noun, Pos::Verb, Pos::Adj, Pos::Adv])
    }
}

fn pos_label(pos: Pos) -> &'static str {
    match pos {
        Pos::Noun => "noun",
        Pos::Verb => "verb",
        Pos::Adj => "adj",
        Pos::Adv => "adv",
    }
}

fn pos_order(pos: Pos) -> usize {
    match pos {
        Pos::Noun => 0,
        Pos::Verb => 1,
        Pos::Adj => 2,
        Pos::Adv => 3,
    }
}

fn synset_id_response(id: SynsetId) -> SynsetIdResponse {
    SynsetIdResponse {
        pos: id.pos.to_char(),
        offset: id.offset,
    }
}

fn best_sense_count_for_synset(
    wn: &WordNet,
    synset: &Synset<'_>,
    candidate_lemmas: &[String],
) -> Option<u32> {
    let mut best = None;
    for lemma in candidate_lemmas {
        if let Some(count) = wn.sense_count(synset.id.pos, lemma, synset.id.offset) {
            best = match best {
                Some(existing) if existing >= count => Some(existing),
                _ => Some(count),
            };
        }
    }
    best
}

fn best_sense_count_from_synset(wn: &WordNet, synset: &Synset<'_>) -> Option<u32> {
    let mut best = None;
    for lemma in &synset.words {
        if let Some(count) = wn.sense_count(synset.id.pos, lemma.text, synset.id.offset) {
            best = match best {
                Some(existing) if existing >= count => Some(existing),
                _ => Some(count),
            };
        }
    }
    best
}

fn relation_label(symbol: &str) -> (&'static str, &'static str) {
    match symbol {
        "!" => ("antonyms", "Antonyms"),
        "@" | "@i" => ("hypernyms", "Hypernyms"),
        "~" | "~i" => ("hyponyms", "Hyponyms"),
        "&" => ("similar_to", "Similar to"),
        "^" => ("also_see", "Also see"),
        "+" => ("derivations", "Derivationally related"),
        "=" => ("attributes", "Attributes"),
        "<" => ("participle", "Participle of"),
        "\\" => ("pertainyms", "Pertainyms"),
        "*" => ("entails", "Entails"),
        ">" => ("causes", "Causes"),
        "$" => ("verb_group", "Verb group"),
        "#m" => ("member_holonyms", "Member holonyms"),
        "#s" => ("substance_holonyms", "Substance holonyms"),
        "#p" => ("part_holonyms", "Part holonyms"),
        "%m" => ("member_meronyms", "Member meronyms"),
        "%s" => ("substance_meronyms", "Substance meronyms"),
        "%p" => ("part_meronyms", "Part meronyms"),
        ";c" => ("topic_domain", "Topic domain"),
        "-c" => ("topic_members", "Topic members"),
        ";r" => ("region_domain", "Region domain"),
        "-r" => ("region_members", "Region members"),
        ";u" => ("usage_domain", "Usage domain"),
        "-u" => ("usage_members", "Usage members"),
        _ => ("other", "Other"),
    }
}

fn relation_order(kind: &str) -> usize {
    let order: [&str; 23] = [
        "hypernyms",
        "hyponyms",
        "similar_to",
        "antonyms",
        "derivations",
        "also_see",
        "entails",
        "causes",
        "verb_group",
        "attributes",
        "participle",
        "pertainyms",
        "member_meronyms",
        "part_meronyms",
        "substance_meronyms",
        "member_holonyms",
        "part_holonyms",
        "substance_holonyms",
        "topic_domain",
        "topic_members",
        "region_domain",
        "region_members",
        "usage_domain",
    ];
    order
        .iter()
        .position(|k| k == &kind)
        .unwrap_or(order.len() + 1)
}

fn char_to_pos(c: char) -> Pos {
    Pos::from_char(c).unwrap_or(Pos::Noun)
}

fn lemma_sort_key(lemmas: &[String]) -> String {
    lemmas
        .first()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

fn collect_relations(wn: &WordNet, synset: &Synset<'_>) -> Vec<RelationGroup> {
    let mut groups: HashMap<String, RelationGroup> = HashMap::new();
    for ptr in &synset.pointers {
        let (kind, label) = relation_label(ptr.symbol);
        let Some(target_synset) = wn.get_synset(ptr.target) else {
            continue;
        };
        let target = RelatedTarget {
            pos: pos_label(target_synset.id.pos).to_string(),
            synset_id: synset_id_response(target_synset.id),
            lemmas: target_synset
                .words
                .iter()
                .map(|w| w.text.to_string())
                .collect(),
            definition: target_synset.gloss.definition.to_string(),
            sense_count: best_sense_count_from_synset(wn, &target_synset),
        };
        let entry = groups
            .entry(kind.to_string())
            .or_insert_with(|| RelationGroup {
                kind: kind.to_string(),
                label: label.to_string(),
                symbol: ptr.symbol.to_string(),
                targets: Vec::new(),
            });
        let exists = entry.targets.iter().any(|t| {
            t.synset_id.pos == target.synset_id.pos && t.synset_id.offset == target.synset_id.offset
        });
        if !exists {
            entry.targets.push(target);
        }
    }

    let mut groups_vec: Vec<_> = groups.into_values().collect();
    for group in &mut groups_vec {
        group.targets.sort_by(|a, b| {
            let sa = a.sense_count.unwrap_or(0);
            let sb = b.sense_count.unwrap_or(0);
            sb.cmp(&sa)
                .then_with(|| lemma_sort_key(&a.lemmas).cmp(&lemma_sort_key(&b.lemmas)))
        });
    }
    groups_vec.sort_by(|a, b| {
        relation_order(&a.kind)
            .cmp(&relation_order(&b.kind))
            .then_with(|| a.label.cmp(&b.label))
    });
    groups_vec
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(msg) => {
                let body = Json(ErrorResponse { error: msg });
                (StatusCode::BAD_REQUEST, body).into_response()
            }
            ApiError::Internal => {
                let body = Json(json!({ "error": "internal server error" }));
                (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
            }
        }
    }
}
