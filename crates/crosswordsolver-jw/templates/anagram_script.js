const maxLen = __MAX_LEN__;
const lettersInput = document.getElementById('lettersInput');
const patternRow = document.getElementById('patternRow');
const solveBtn = document.getElementById('solveBtn');
const resetBtn = document.getElementById('resetBtn');
const statusEl = document.getElementById('status');
const resultsList = document.getElementById('resultsList');
const firstBtn = document.getElementById('firstBtn');
const prevBtn = document.getElementById('prevBtn');
const nextBtn = document.getElementById('nextBtn');
const lastBtn = document.getElementById('lastBtn');
const pageInfo = document.getElementById('pageInfo');
const recentEl = document.getElementById('recentAnagrams');
const definitionCache = new Map();
let popoverEl = null;
let popoverWord = '';

async function readError(resp) {
  const text = await resp.text();
  try {
    const data = JSON.parse(text);
    if (data && data.error) return data.error;
  } catch (_) {}
  return text || `Request failed (${resp.status})`;
}

const state = {
  letters: '',
  pattern: '',
  page: 1,
  pageSize: 50,
  hasMore: false,
  loading: false,
  total: 0,
};

function escapeHtml(str) {
  if (str == null) return '';
  str = String(str);
  return str.replace(/[&<>"']/g, ch => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[ch]));
}

function prettyLemma(lemma) {
  return (lemma || '').replace(/_/g, ' ');
}

function ensurePopover() {
  if (popoverEl) return popoverEl;
  popoverEl = document.createElement('div');
  popoverEl.className = 'definition-popover';
  popoverEl.style.display = 'none';
  document.body.appendChild(popoverEl);
  return popoverEl;
}

function hidePopover() {
  if (!popoverEl) return;
  popoverEl.style.display = 'none';
  popoverWord = '';
}

function positionPopover(target) {
  const pop = ensurePopover();
  pop.style.visibility = 'hidden';
  pop.style.display = 'block';
  const rect = target.getBoundingClientRect();
  const popRect = pop.getBoundingClientRect();
  let top = rect.bottom + window.scrollY + 8;
  if (top + popRect.height > window.scrollY + window.innerHeight) {
    top = rect.top + window.scrollY - popRect.height - 8;
  }
  let left = rect.left + window.scrollX + (rect.width / 2) - (popRect.width / 2);
  left = Math.max(12, Math.min(left, window.scrollX + window.innerWidth - popRect.width - 12));
  pop.style.top = `${top}px`;
  pop.style.left = `${left}px`;
  pop.style.visibility = 'visible';
}

function renderDefinitions(word, data) {
  const results = (data && data.results) || [];
  const defs = results.slice(0, 5).map(res => ({
    pos: res.pos || '',
    definition: res.definition || '',
    example: (res.examples && res.examples[0]) || '',
    lemmas: res.lemmas || [],
  }));
  if (!defs.length) {
    return `<h3>${escapeHtml(word)}</h3><div class="source mb-2">WordNet</div><div class="text-muted">No definition found.</div>`;
  }
  const items = defs.map(d => {
    const pos = d.pos ? `<span class="text-muted">${escapeHtml(d.pos)}</span> ` : '';
    const example = d.example ? `<div class="text-muted small mt-1">“${escapeHtml(d.example)}”</div>` : '';
    const lemmas = d.lemmas && d.lemmas.length
      ? `<div class="text-muted small">${escapeHtml(d.lemmas.map(prettyLemma).join(', '))}</div>`
      : '';
    return `<li>${pos}${escapeHtml(d.definition)}${lemmas}${example}</li>`;
  }).join('');
  return `<h3>${escapeHtml(word)}</h3><div class="source mb-2">WordNet</div><ol>${items}</ol>`;
}

async function fetchDefinition(word) {
  if (definitionCache.has(word)) return definitionCache.get(word);
  const resp = await fetch(`/v1/wordnet/dictionary?word=${encodeURIComponent(word)}`);
  if (!resp.ok) {
    const text = await resp.text();
    throw new Error(text || 'Definition lookup failed.');
  }
  const data = await resp.json();
  definitionCache.set(word, data);
  return data;
}

function showPopover(target, html, word) {
  const pop = ensurePopover();
  pop.innerHTML = html;
  pop.style.display = 'block';
  popoverWord = word;
  positionPopover(target);
}

function handleDefinitionRequest(word, target) {
  if (popoverWord === word && popoverEl && popoverEl.style.display === 'block') {
    hidePopover();
    return;
  }
  showPopover(target, `<div class="text-muted small">Loading definition for <strong>${escapeHtml(word)}</strong>...</div>`, word);
  fetchDefinition(word)
    .then(entries => {
      showPopover(target, renderDefinitions(word, entries), word);
    })
    .catch(err => {
      const message = err.message || 'Unable to load definition.';
      showPopover(target, `<div class="text-danger small">${escapeHtml(message)}</div>`, word);
    });
}

function handleResultClick(e) {
  const li = e.target.closest('li[data-word]');
  if (!li) return;
  const word = li.dataset.word;
  handleDefinitionRequest(word, li);
}

function handleResultKeydown(e) {
  if (e.key !== 'Enter' && e.key !== ' ') return;
  const li = e.target.closest('li[data-word]');
  if (!li) return;
  e.preventDefault();
  const word = li.dataset.word;
  handleDefinitionRequest(word, li);
}

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
  pageInfo.textContent = '';
  hidePopover();
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
  state.total = 0;
  clearResults();
  fetchPage();
}

function resetAll() {
  lettersInput.value = '';
  state.letters = '';
  state.pattern = '';
  state.page = 1;
  state.hasMore = false;
  state.total = 0;
  buildPatternInputs(0);
  clearResults();
  statusEl.textContent = 'Enter letters to search.';
}

function updatePagerControls() {
  const totalPages = state.total > 0 ? Math.ceil(state.total / state.pageSize) : 0;
  const atFirst = state.page <= 1;
  const atLast = totalPages === 0 || state.page >= totalPages;
  firstBtn.disabled = state.loading || atFirst;
  prevBtn.disabled = state.loading || atFirst;
  nextBtn.disabled = state.loading || atLast;
  lastBtn.disabled = state.loading || atLast;
  if (totalPages > 0) {
    pageInfo.textContent = `Page ${state.page} of ${totalPages}`;
  } else {
    pageInfo.textContent = '';
  }
}

async function fetchPage() {
  if (!state.letters || state.loading) return;
  state.loading = true;
  hidePopover();
  statusEl.classList.remove('text-danger');
  statusEl.classList.add('text-muted');
  statusEl.textContent = `Loading page ${state.page}...`;
  resultsList.innerHTML = '';
  try {
    const url = `/v1/anagrams?letters=${encodeURIComponent(state.letters)}&pattern=${encodeURIComponent(state.pattern)}&page=${state.page}&page_size=${state.pageSize}`;
    const resp = await fetch(url);
    if (!resp.ok) {
      const msg = await readError(resp);
      throw new Error(msg);
    }
    const data = await resp.json();
    state.total = data.total;
    const totalPages = data.total > 0 ? Math.ceil(data.total / state.pageSize) : 0;
    state.hasMore = totalPages > 0 && state.page < totalPages;
    statusEl.textContent = data.total === 0 ? 'No matches found.' : `${data.total} results`;
    if (data.items.length === 0) {
      resultsList.innerHTML = '<li class="list-group-item text-muted">No matches found.</li>';
    } else {
      data.items.forEach(word => {
        const li = document.createElement('li');
        li.className = 'list-group-item fs-5 word-item';
        li.textContent = word;
        li.dataset.word = word;
        li.setAttribute('role', 'button');
        li.tabIndex = 0;
        resultsList.appendChild(li);
      });
    }
  } catch (err) {
    statusEl.classList.remove('text-muted');
    statusEl.classList.add('text-danger');
    const message = err.message || 'Error fetching results.';
    statusEl.textContent = message;
    resultsList.innerHTML = `<li class="list-group-item text-danger">${message}</li>`;
  } finally {
    state.loading = false;
    updatePagerControls();
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
  state.total = 0;
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
resultsList.addEventListener('click', handleResultClick);
resultsList.addEventListener('keydown', handleResultKeydown);
firstBtn.addEventListener('click', () => {
  if (state.loading || state.page === 1) return;
  state.page = 1;
  fetchPage();
});
[lettersInput].forEach(el => {
  el.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      runSolve();
    }
  });
});
nextBtn.addEventListener('click', () => {
  if (state.loading) return;
  state.page += 1;
  fetchPage();
});
prevBtn.addEventListener('click', () => {
  if (state.loading || state.page <= 1) return;
  state.page -= 1;
  fetchPage();
});
lastBtn.addEventListener('click', () => {
  if (state.loading || state.total === 0) return;
  const totalPages = Math.ceil(state.total / state.pageSize);
  if (state.page === totalPages) return;
  state.page = totalPages;
  fetchPage();
});

document.addEventListener('click', (e) => {
  if (!popoverEl || popoverEl.style.display !== 'block') return;
  if (popoverEl.contains(e.target)) return;
  if (resultsList.contains(e.target)) return;
  hidePopover();
});

window.addEventListener('scroll', () => hidePopover(), true);
window.addEventListener('resize', () => hidePopover());
buildPatternInputs(0, false);
renderRecentAnagrams();
updatePagerControls();
