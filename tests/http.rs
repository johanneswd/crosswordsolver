use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;

use crosswordsolver::handlers::{router, AppState};
use crosswordsolver::index::WordIndex;

fn make_state() -> AppState {
    let words = b"apple\nangle\nankle\naddle\nample\n";
    let tempdir = tempfile::tempdir().unwrap();
    let path = tempdir.path().join("words.txt");
    std::fs::write(&path, words).unwrap();
    let index = WordIndex::build_from_file(&path).unwrap();
    AppState {
        index: Arc::clone(&index),
        max_page_size: 500,
    }
}

#[tokio::test]
async fn healthz_ok() {
    let state = make_state();
    let app = router(state);
    let response = app
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn matches_endpoint_returns_results() {
    let state = make_state();
    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/matches?pattern=a__le&page=1&page_size=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["pattern"], "a__le");
    assert!(body["items"].as_array().unwrap().len() <= 2);
    assert!(body["total"].as_u64().unwrap() >= 1);
}
