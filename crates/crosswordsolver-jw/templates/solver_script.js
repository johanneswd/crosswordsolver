const maxLen = __MAX_LEN__;
const lengthSelect = document.getElementById('lengthSelect');
const charRow = document.getElementById('charRow');
const solveBtn = document.getElementById('solveBtn');
const resetBtn = document.getElementById('resetBtn');
const statusEl = document.getElementById('status');
const resultsList = document.getElementById('resultsList');
const firstBtn = document.getElementById('firstBtn');
const prevBtn = document.getElementById('prevBtn');
const nextBtn = document.getElementById('nextBtn');
const lastBtn = document.getElementById('lastBtn');
const pageInfo = document.getElementById('pageInfo');
const recentEl = document.getElementById('recentPatterns');
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

function renderDefinitions(word, entries) {
  const defs = [];
  entries.forEach(entry => {
    (entry.meanings || []).forEach(meaning => {
      (meaning.definitions || []).forEach(def => {
        if (defs.length < 5) {
          defs.push({
            partOfSpeech: meaning.partOfSpeech || '',
            definition: def.definition || '',
            example: def.example || '',
          });
        }
      });
    });
  });
  if (!defs.length) {
    return `<h3>${escapeHtml(word)}</h3><div class="source mb-2">dictionaryapi.dev</div><div class="text-muted">No definition found.</div>`;
  }
  const items = defs.map(d => {
    const pos = d.partOfSpeech ? `<span class="text-muted">${escapeHtml(d.partOfSpeech)}</span> ` : '';
    const example = d.example ? `<div class="text-muted small mt-1">“${escapeHtml(d.example)}”</div>` : '';
    return `<li>${pos}${escapeHtml(d.definition)}${example}</li>`;
  }).join('');
  return `<h3>${escapeHtml(word)}</h3><div class="source mb-2">dictionaryapi.dev</div><ol>${items}</ol>`;
}

async function fetchDefinition(word) {
  if (definitionCache.has(word)) return definitionCache.get(word);
  const resp = await fetch(`https://api.dictionaryapi.dev/api/v2/entries/en/${encodeURIComponent(word)}`);
  if (!resp.ok) {
    throw new Error('Definition lookup failed.');
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

function buildLengthOptions() {
  for (let i = 1; i <= maxLen; i++) {
    const opt = document.createElement('option');
    opt.value = i;
    opt.textContent = i + ' letter' + (i === 1 ? '' : 's');
    lengthSelect.appendChild(opt);
  }
  lengthSelect.value = 7;
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
  state.total = 0;
  clearResults();
  rememberPattern(pattern);
  renderRecent();
  await fetchPage();
}

function resetAll() {
  lengthSelect.value = 7;
  const len = 7;
  buildInputs(len);
  statusEl.textContent = 'Enter a pattern to search.';
  clearResults();
  state.pattern = '';
  state.page = 1;
  state.hasMore = false;
  state.total = 0;
  updatePagerControls();
}

function clearResults() {
  resultsList.innerHTML = '';
  pageInfo.textContent = '';
  hidePopover();
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
  state.total = 0;
  clearResults();
  fetchPage();
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
  if (!state.pattern || state.loading) return;
  state.loading = true;
  hidePopover();
  statusEl.classList.remove('text-danger');
  statusEl.classList.add('text-muted');
  statusEl.textContent = `Loading page ${state.page}...`;
  resultsList.innerHTML = '';
  try {
    const resp = await fetch(`/v1/matches?pattern=${encodeURIComponent(state.pattern)}&page=${state.page}&page_size=${state.pageSize}`);
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

lengthSelect.addEventListener('change', (e) => {
  const len = parseInt(e.target.value, 10) || 5;
  buildInputs(len);
});
charRow.addEventListener('input', handleInput);
charRow.addEventListener('keydown', handleKeydown);
solveBtn.addEventListener('click', runSolve);
resetBtn.addEventListener('click', resetAll);
resultsList.addEventListener('click', handleResultClick);
resultsList.addEventListener('keydown', handleResultKeydown);
firstBtn.addEventListener('click', () => {
  if (state.loading || state.page === 1) return;
  state.page = 1;
  fetchPage();
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

buildLengthOptions();
buildInputs(parseInt(lengthSelect.value, 10));
renderRecent();
updatePagerControls();
