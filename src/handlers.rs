use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::index::{
    parse_letter_bag, parse_letters, parse_pattern, AnagramParams, QueryParams, WordIndex,
    MAX_WORD_LEN,
};

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<WordIndex>,
    pub max_page_size: usize,
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

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(frontend))
        .route("/anagrams", get(anagram_frontend))
        .route("/robots.txt", get(robots))
        .route("/healthz", get(healthz))
        .route("/v1/matches", get(matches))
        .route("/v1/anagrams", get(anagrams))
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    "ok"
}

async fn robots() -> impl IntoResponse {
    (
        axum::http::HeaderMap::from_iter([(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
        )]),
        "User-agent: *\nDisallow: /",
    )
}

async fn frontend() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn anagram_frontend() -> Html<&'static str> {
    Html(ANAGRAM_HTML)
}

async fn matches(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<MatchesQuery>,
) -> Result<Json<MatchesResponse>, ApiError> {
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

    Ok(Json(response))
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
) -> Result<Json<MatchesResponse>, ApiError> {
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
    let pattern_vec = parse_pattern(&pattern_str).map_err(|e| ApiError::bad_request(e.to_string()))?;
    if pattern_vec.len() != letters.len() {
        return Err(ApiError::bad_request("pattern length must match letters length"));
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

    Ok(Json(response))
}

const INDEX_HTML: &str = r#"
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Crossword Solver</title>
  <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet">
  <style>
    body { background: #f8f9fa; }
    .char-box { width: 2.75rem; height: 2.75rem; text-align: center; font-size: 1.5rem; }
    .char-row { gap: 0.5rem; }
    .results { min-height: 4rem; }
    .results-list { max-height: 60vh; overflow-y: auto; }
    .sticky-header { position: sticky; top: 0; z-index: 1020; background: #f8f9fa; padding-bottom: 0.75rem; }
    .nav-links a { text-decoration: none; }
  </style>
</head>
<body class="py-4">
  <div class="container">
    <div class="sticky-header">
      <div class="d-flex justify-content-between align-items-center">
        <div>
          <h1 class="h4 mb-0">Crossword Solver</h1>
          <div class="nav-links small">
            <a href="/">Solver</a> · <a href="/anagrams">Anagram</a>
          </div>
        </div>
        <button id="resetBtn" class="btn btn-outline-danger btn-lg">Reset</button>
      </div>
    </div>

    <div class="card shadow-sm">
      <div class="card-body">
        <div class="mb-3">
          <label class="form-label fw-semibold">Word length</label>
          <select id="lengthSelect" class="form-select form-select-lg" aria-label="Word length"></select>
        </div>

        <div class="mb-3">
          <label class="form-label fw-semibold">Pattern</label>
          <div id="charRow" class="d-flex flex-wrap char-row"></div>
          <div class="form-text">Type letters; leave blanks empty. Non-letters are ignored.</div>
        </div>

        <div class="d-grid">
          <button id="solveBtn" class="btn btn-primary btn-lg">Solve</button>
        </div>

        <div class="mt-3">
          <h2 class="h6 text-muted mb-2">Recent patterns</h2>
          <div id="recentPatterns" class="d-flex flex-wrap gap-2"></div>
        </div>
      </div>
    </div>

    <div class="mt-4">
      <h2 class="h5">Candidates</h2>
      <div id="status" class="text-muted mb-2">Enter a pattern to search.</div>
      <div id="results" class="results">
        <ul id="resultsList" class="list-group results-list"></ul>
        <div id="sentinel" class="py-2 text-center text-muted d-none">Loading more…</div>
      </div>
    </div>
  </div>

  <script>
    const maxLen = 24;
    const lengthSelect = document.getElementById('lengthSelect');
    const charRow = document.getElementById('charRow');
    const solveBtn = document.getElementById('solveBtn');
    const resetBtn = document.getElementById('resetBtn');
    const statusEl = document.getElementById('status');
    const resultsEl = document.getElementById('results');
    const resultsList = document.getElementById('resultsList');
    const sentinel = document.getElementById('sentinel');
    const recentEl = document.getElementById('recentPatterns');

    const state = {
      pattern: '',
      page: 1,
      pageSize: 50,
      hasMore: false,
      loading: false,
    };

    function buildLengthOptions() {
      for (let i = 1; i <= maxLen; i++) {
        const opt = document.createElement('option');
        opt.value = i;
        opt.textContent = i + ' letter' + (i === 1 ? '' : 's');
        lengthSelect.appendChild(opt);
      }
      lengthSelect.value = 5;
    }

    function buildInputs(len) {
      charRow.innerHTML = '';
      for (let i = 0; i < len; i++) {
        const input = document.createElement('input');
        input.type = 'text';
        input.inputMode = 'text';
        input.maxLength = 1;
        input.className = 'form-control char-box';
        input.dataset.idx = i;
        charRow.appendChild(input);
      }
      focusInput(0);
    }

    function focusInput(idx) {
      const target = charRow.querySelector(`input[data-idx="${idx}"]`);
      if (target) target.focus();
    }

    function handleInput(e) {
      const target = e.target;
      if (!target.dataset.idx) return;
      const idx = parseInt(target.dataset.idx, 10);
      const val = target.value;
      if (!val) return;
      const ch = val.slice(-1);
      if (/^[A-Za-z]$/.test(ch)) {
        target.value = ch.toLowerCase();
      } else {
        target.value = '';
      }
      focusInput(idx + 1);
    }

    function handleKeydown(e) {
      const target = e.target;
      if (!target.dataset.idx) return;
      const idx = parseInt(target.dataset.idx, 10);
      if (e.key === 'Backspace' && !target.value && idx > 0) {
        const prev = charRow.querySelector(`input[data-idx="${idx - 1}"]`);
        if (prev) {
          e.preventDefault();
          prev.focus();
        }
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        runSolve();
      }
    }

    function patternFromInputs() {
      const inputs = Array.from(charRow.querySelectorAll('input'));
      return inputs.map(inp => inp.value ? inp.value.toLowerCase() : '_').join('');
    }

    async function runSolve() {
      const pattern = patternFromInputs();
      if (!pattern) return;
      state.pattern = pattern;
      state.page = 1;
      state.hasMore = false;
      clearResults();
      rememberPattern(pattern);
      renderRecent();
      await fetchPage();
    }

    function resetAll() {
      const len = parseInt(lengthSelect.value, 10) || 5;
      buildInputs(len);
      statusEl.textContent = 'Enter a pattern to search.';
      clearResults();
      state.pattern = '';
      state.page = 1;
      state.hasMore = false;
    }

    function clearResults() {
      resultsList.innerHTML = '';
      sentinel.classList.add('d-none');
    }

    function rememberPattern(pattern) {
      const key = 'recentPatterns';
      const stored = JSON.parse(localStorage.getItem(key) || '[]');
      const filtered = stored.filter(p => p !== pattern);
      filtered.unshift(pattern);
      const trimmed = filtered.slice(0, 5);
      localStorage.setItem(key, JSON.stringify(trimmed));
    }

    function getRecentPatterns() {
      try {
        return JSON.parse(localStorage.getItem('recentPatterns') || '[]');
      } catch (_) {
        return [];
      }
    }

    function renderRecent() {
      const recents = getRecentPatterns();
      recentEl.innerHTML = '';
      if (!recents.length) {
        recentEl.innerHTML = '<span class="text-muted">No recent patterns yet.</span>';
        return;
      }
      recents.forEach(pat => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'btn btn-outline-secondary btn-sm';
        btn.textContent = pat;
        btn.addEventListener('click', () => loadPattern(pat));
        recentEl.appendChild(btn);
      });
    }

    function loadPattern(pattern) {
      const len = Math.min(Math.max(pattern.length, 1), maxLen);
      lengthSelect.value = len;
      buildInputs(len);
      Array.from(charRow.querySelectorAll('input')).forEach((inp, idx) => {
        inp.value = pattern[idx] && /[a-z]/i.test(pattern[idx]) ? pattern[idx] : '';
      });
      state.pattern = pattern;
      state.page = 1;
      state.hasMore = false;
      clearResults();
      fetchPage();
    }

    async function fetchPage() {
      if (!state.pattern || state.loading) return;
      state.loading = true;
      sentinel.classList.remove('d-none');
      statusEl.textContent = `Loading page ${state.page}...`;
      try {
        const resp = await fetch(`/v1/matches?pattern=${encodeURIComponent(state.pattern)}&page=${state.page}&page_size=${state.pageSize}`);
        if (!resp.ok) throw new Error(`Request failed (${resp.status})`);
        const data = await resp.json();
        state.hasMore = data.has_more;
        statusEl.textContent = `${data.total} results${state.hasMore ? ' (scroll for more)' : ''}`;
        if (data.items.length === 0 && state.page === 1) {
          resultsList.innerHTML = '<li class="list-group-item text-muted">No matches found.</li>';
        } else {
          data.items.forEach(word => {
            const li = document.createElement('li');
            li.className = 'list-group-item fs-5';
            li.textContent = word;
            resultsList.appendChild(li);
          });
        }
        state.page += 1;
        if (!state.hasMore) {
          sentinel.classList.add('d-none');
        }
      } catch (err) {
        statusEl.textContent = 'Error fetching results.';
        resultsList.innerHTML = `<li class="list-group-item text-danger">${err.message}</li>`;
        sentinel.classList.add('d-none');
      } finally {
        state.loading = false;
      }
    }

    const observer = new IntersectionObserver((entries) => {
      entries.forEach(entry => {
        if (entry.isIntersecting && state.hasMore && !state.loading) {
          fetchPage();
        }
      });
    }, { root: resultsList, rootMargin: '0px', threshold: 1.0 });

    lengthSelect.addEventListener('change', (e) => {
      const len = parseInt(e.target.value, 10) || 5;
      buildInputs(len);
    });
    charRow.addEventListener('input', handleInput);
    charRow.addEventListener('keydown', handleKeydown);
    solveBtn.addEventListener('click', runSolve);
    resetBtn.addEventListener('click', resetAll);
    observer.observe(sentinel);

    buildLengthOptions();
    buildInputs(parseInt(lengthSelect.value, 10));
    renderRecent();
  </script>
</body>
</html>
"#;

const ANAGRAM_HTML: &str = r#"
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Anagram Solver</title>
  <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet">
  <style>
    body { background: #f8f9fa; }
    .results-list { max-height: 60vh; overflow-y: auto; }
    .sticky-header { position: sticky; top: 0; z-index: 1020; background: #f8f9fa; padding-bottom: 0.75rem; }
    .nav-links a { text-decoration: none; }
    .char-box { width: 2.75rem; height: 2.75rem; text-align: center; font-size: 1.5rem; }
    .char-row { gap: 0.5rem; }
  </style>
</head>
<body class="py-4">
  <div class="container">
    <div class="sticky-header mb-3">
      <div class="d-flex justify-content-between align-items-center">
        <div>
          <h1 class="h4 mb-0">Anagram Solver</h1>
          <div class="nav-links small">
            <a href="/">Solver</a> · <a href="/anagrams">Anagram</a>
          </div>
        </div>
        <button id="resetBtn" class="btn btn-outline-danger btn-lg">Reset</button>
      </div>
    </div>

    <div class="card shadow-sm">
      <div class="card-body">
        <div class="mb-3">
          <label class="form-label fw-semibold">Letters (bag)</label>
          <input id="lettersInput" type="text" class="form-control form-control-lg" placeholder="e.g. listen" autocomplete="off">
          <div class="form-text">Enter all letters to permute; use only A-Z.</div>
        </div>

        <div class="mb-3">
          <label class="form-label fw-semibold">Pattern (optional)</label>
          <div id="patternRow" class="d-flex flex-wrap char-row"></div>
          <div class="form-text">Type letters; leave blanks empty. Length matches the letters above.</div>
        </div>

        <div class="d-grid">
          <button id="solveBtn" class="btn btn-primary btn-lg">Find anagrams</button>
        </div>

        <div class="mt-3">
          <h2 class="h6 text-muted mb-2">Recent anagrams</h2>
          <div id="recentAnagrams" class="d-flex flex-wrap gap-2"></div>
        </div>
      </div>
    </div>

    <div class="mt-4">
      <h2 class="h5">Candidates</h2>
      <div id="status" class="text-muted mb-2">Enter letters to search.</div>
      <ul id="resultsList" class="list-group results-list"></ul>
      <div id="sentinel" class="py-2 text-center text-muted d-none">Loading more…</div>
    </div>
  </div>

  <script>
    const maxLen = 24;
    const lettersInput = document.getElementById('lettersInput');
    const patternRow = document.getElementById('patternRow');
    const solveBtn = document.getElementById('solveBtn');
    const resetBtn = document.getElementById('resetBtn');
    const statusEl = document.getElementById('status');
    const resultsList = document.getElementById('resultsList');
    const sentinel = document.getElementById('sentinel');
    const recentEl = document.getElementById('recentAnagrams');

    const state = {
      letters: '',
      pattern: '',
      page: 1,
      pageSize: 50,
      hasMore: false,
      loading: false,
    };

    function buildPatternInputs(len, focusFirst = false) {
      patternRow.innerHTML = '';
      if (len <= 0) return;
      for (let i = 0; i < len; i++) {
        const input = document.createElement('input');
        input.type = 'text';
        input.inputMode = 'text';
        input.maxLength = 1;
        input.className = 'form-control char-box';
        input.dataset.idx = i;
        patternRow.appendChild(input);
      }
      if (focusFirst) {
        focusPatternInput(0);
      }
    }

    function focusPatternInput(idx) {
      const target = patternRow.querySelector(`input[data-idx="${idx}"]`);
      if (target) target.focus();
    }

    function handlePatternInput(e) {
      const target = e.target;
      if (!target.dataset.idx) return;
      const idx = parseInt(target.dataset.idx, 10);
      const val = target.value;
      if (!val) return;
      const ch = val.slice(-1);
      if (/^[A-Za-z]$/.test(ch)) {
        target.value = ch.toLowerCase();
      } else {
        target.value = '';
      }
      focusPatternInput(idx + 1);
    }

    function handlePatternKeydown(e) {
      const target = e.target;
      if (!target.dataset.idx) return;
      const idx = parseInt(target.dataset.idx, 10);
      if (e.key === 'Backspace' && !target.value && idx > 0) {
        const prev = patternRow.querySelector(`input[data-idx="${idx - 1}"]`);
        if (prev) {
          e.preventDefault();
          prev.focus();
        }
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        runSolve();
      }
    }

    function patternFromInputs() {
      const inputs = Array.from(patternRow.querySelectorAll('input'));
      if (!inputs.length) return '';
      return inputs.map(inp => inp.value ? inp.value.toLowerCase() : '_').join('');
    }

    function clearResults() {
      resultsList.innerHTML = '';
      sentinel.classList.add('d-none');
    }

    function rememberAnagram(letters, pattern) {
        const key = 'recentAnagrams';
        const stored = JSON.parse(localStorage.getItem(key) || '[]');
        const entry = { letters, pattern };
        const filtered = stored.filter(e => !(e.letters === letters && e.pattern === pattern));
        filtered.unshift(entry);
        const trimmed = filtered.slice(0, 5);
        localStorage.setItem(key, JSON.stringify(trimmed));
    }

    function getRecentAnagrams() {
        try {
            return JSON.parse(localStorage.getItem('recentAnagrams') || '[]');
        } catch (_) {
            return [];
        }
    }

    function renderRecentAnagrams() {
        const recents = getRecentAnagrams();
        recentEl.innerHTML = '';
        if (!recents.length) {
            recentEl.innerHTML = '<span class="text-muted">No recent anagrams yet.</span>';
            return;
        }
        recents.forEach(entry => {
            const btn = document.createElement('button');
            btn.type = 'button';
            btn.className = 'btn btn-outline-secondary btn-sm';
            const patDisplay = entry.pattern.replace(/_/g, '·');
            btn.textContent = `${entry.letters} (${patDisplay})`;
            btn.addEventListener('click', () => loadAnagram(entry));
            recentEl.appendChild(btn);
        });
    }

    function loadAnagram(entry) {
      lettersInput.value = entry.letters;
      const len = entry.letters.length;
      buildPatternInputs(len, false);
      Array.from(patternRow.querySelectorAll('input')).forEach((inp, idx) => {
        const ch = entry.pattern[idx] || '_';
        inp.value = ch === '_' ? '' : ch;
      });
      state.letters = entry.letters;
      state.pattern = entry.pattern;
      state.page = 1;
      state.hasMore = false;
      clearResults();
      fetchPage();
    }

    function resetAll() {
      lettersInput.value = '';
      state.letters = '';
      state.pattern = '';
      state.page = 1;
      state.hasMore = false;
      buildPatternInputs(0);
      clearResults();
      statusEl.textContent = 'Enter letters to search.';
    }

    async function fetchPage() {
      if (!state.letters || state.loading) return;
      state.loading = true;
      sentinel.classList.remove('d-none');
      statusEl.textContent = `Loading page ${state.page}...`;
      try {
        const url = `/v1/anagrams?letters=${encodeURIComponent(state.letters)}&pattern=${encodeURIComponent(state.pattern)}&page=${state.page}&page_size=${state.pageSize}`;
        const resp = await fetch(url);
        if (!resp.ok) throw new Error(`Request failed (${resp.status})`);
        const data = await resp.json();
        state.hasMore = data.has_more;
        statusEl.textContent = `${data.total} results${state.hasMore ? ' (scroll for more)' : ''}`;
        if (data.items.length === 0 && state.page === 1) {
          resultsList.innerHTML = '<li class="list-group-item text-muted">No matches found.</li>';
        } else {
          data.items.forEach(word => {
            const li = document.createElement('li');
            li.className = 'list-group-item fs-5';
            li.textContent = word;
            resultsList.appendChild(li);
          });
        }
        state.page += 1;
        if (!state.hasMore) {
          sentinel.classList.add('d-none');
        }
      } catch (err) {
        statusEl.textContent = 'Error fetching results.';
        resultsList.innerHTML = `<li class="list-group-item text-danger">${err.message}</li>`;
        sentinel.classList.add('d-none');
      } finally {
        state.loading = false;
      }
    }

    async function runSolve() {
      const letters = lettersInput.value.trim().toLowerCase();
      if (!letters) {
        statusEl.textContent = 'Letters are required.';
        return;
      }
      if (!/^[a-z]+$/.test(letters)) {
        statusEl.textContent = 'Letters must be A-Z only.';
        return;
      }
      if (letters.length > maxLen) {
        statusEl.textContent = `Letters must be at most ${maxLen}.`;
        return;
      }
      const pattern = patternFromInputs() || '_'.repeat(letters.length);
      state.letters = letters;
      state.pattern = pattern;
      state.page = 1;
      state.hasMore = false;
      clearResults();
      rememberAnagram(letters, pattern);
      renderRecentAnagrams();
      await fetchPage();
    }

    lettersInput.addEventListener('input', () => {
      const len = lettersInput.value.trim().length;
      if (len > 0 && len <= maxLen) {
        buildPatternInputs(len, false);
      } else {
        buildPatternInputs(0, false);
      }
    });
    patternRow.addEventListener('input', handlePatternInput);
    patternRow.addEventListener('keydown', handlePatternKeydown);
    solveBtn.addEventListener('click', runSolve);
    resetBtn.addEventListener('click', resetAll);
    [lettersInput].forEach(el => {
      el.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          runSolve();
        }
      });
    });

    const observer = new IntersectionObserver((entries) => {
      entries.forEach(entry => {
        if (entry.isIntersecting && state.hasMore && !state.loading) {
          fetchPage();
        }
      });
    }, { root: resultsList, rootMargin: '0px', threshold: 1.0 });
    observer.observe(sentinel);
    buildPatternInputs(0, false);
    renderRecentAnagrams();
  </script>
</body>
</html>
"#;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, msg).into_response()
            }
            ApiError::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}
