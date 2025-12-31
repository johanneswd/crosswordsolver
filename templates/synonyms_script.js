const wordInput = document.getElementById('wordInput');
const solveBtn = document.getElementById('solveBtn');
const resetBtn = document.getElementById('resetBtn');
const statusEl = document.getElementById('status');
const resultsList = document.getElementById('resultsList');
const definitionCache = new Map();
let popoverEl = null;
let popoverWord = '';
let loading = false;

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

function clearResults() {
  resultsList.innerHTML = '';
  hidePopover();
}

function uniqueWords(list) {
  const seen = new Set();
  const deduped = [];
  list.forEach(w => {
    const key = w.toLowerCase();
    if (key && !seen.has(key)) {
      seen.add(key);
      deduped.push(w);
    }
  });
  return deduped;
}

async function fetchWords(url) {
  const resp = await fetch(url);
  if (!resp.ok) throw new Error('Failed to fetch suggestions.');
  return resp.json();
}

function renderGroup(title, words) {
  const wrapper = document.createElement('div');
  const heading = document.createElement('h3');
  heading.className = 'h6 text-muted mb-2';
  heading.textContent = title;
  wrapper.appendChild(heading);

  const list = document.createElement('ul');
  list.className = 'list-group results-list';
  if (!words.length) {
    const li = document.createElement('li');
    li.className = 'list-group-item text-muted';
    li.textContent = 'None found.';
    list.appendChild(li);
  } else {
    words.forEach(word => {
      const li = document.createElement('li');
      li.className = 'list-group-item fs-5 word-item';
      li.textContent = word;
      li.dataset.word = word;
      li.setAttribute('role', 'button');
      li.tabIndex = 0;
      list.appendChild(li);
    });
  }
  wrapper.appendChild(list);
  resultsList.appendChild(wrapper);
}

async function runSearch() {
  const word = wordInput.value.trim().toLowerCase();
  if (!word) {
    statusEl.textContent = 'Word is required.';
    return;
  }
  if (loading) return;
  loading = true;
  statusEl.classList.remove('text-danger');
  statusEl.classList.add('text-muted');
  statusEl.textContent = 'Loading...';
  clearResults();
  try {
    const encoded = encodeURIComponent(word);
    const [synData, relData, simData] = await Promise.all([
      fetchWords(`https://api.datamuse.com/words?max=30&rel_syn=${encoded}`),
      fetchWords(`https://api.datamuse.com/words?max=30&rel_trg=${encoded}`),
      fetchWords(`https://api.datamuse.com/words?max=30&ml=${encoded}`),
    ]);
    const synonyms = uniqueWords((synData || []).map(entry => entry.word).filter(Boolean));
    const related = uniqueWords((relData || []).map(entry => entry.word).filter(Boolean));
    const similar = uniqueWords((simData || []).map(entry => entry.word).filter(Boolean));

    if (!synonyms.length && !related.length && !similar.length) {
      statusEl.textContent = 'No related words found.';
      resultsList.innerHTML = '<div class="text-muted">Try another word.</div>';
      return;
    }

    renderGroup('Synonyms', synonyms);
    renderGroup('Related words', related);
    renderGroup('Similar meaning', similar);
    statusEl.textContent = 'Click a word for a definition.';
  } catch (err) {
    statusEl.classList.remove('text-muted');
    statusEl.classList.add('text-danger');
    const message = err.message || 'Error fetching suggestions.';
    statusEl.textContent = message;
    resultsList.innerHTML = `<div class="text-danger">${escapeHtml(message)}</div>`;
  } finally {
    loading = false;
  }
}

function resetAll() {
  if (loading) return;
  wordInput.value = '';
  clearResults();
  statusEl.classList.remove('text-danger');
  statusEl.classList.add('text-muted');
  statusEl.textContent = 'Enter a word to search.';
}

solveBtn.addEventListener('click', runSearch);
resetBtn.addEventListener('click', resetAll);
resultsList.addEventListener('click', handleResultClick);
resultsList.addEventListener('keydown', handleResultKeydown);
wordInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    runSearch();
  }
});

document.addEventListener('click', (e) => {
  if (!popoverEl || popoverEl.style.display !== 'block') return;
  if (popoverEl.contains(e.target)) return;
  if (resultsList.contains(e.target)) return;
  hidePopover();
});

window.addEventListener('scroll', () => hidePopover(), true);
window.addEventListener('resize', () => hidePopover());

statusEl.textContent = 'Enter a word to search.';
