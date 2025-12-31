use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use serde_json::json;

use crate::index::{
    AnagramParams, MAX_WORD_LEN, QueryParams, WordIndex, parse_letter_bag, parse_letters,
    parse_pattern,
};

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<WordIndex>,
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

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(frontend))
        .route("/anagrams", get(anagram_frontend))
        .route("/synonyms", get(synonyms_frontend))
        .route("/robots.txt", get(robots))
        .route("/healthz", get(healthz))
        .route("/v1/matches", get(matches))
        .route("/v1/anagrams", get(anagrams))
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
        .replace(
            "{{scripts}}",
            &format!(r#"<script>{}</script>"#, script),
        );
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
