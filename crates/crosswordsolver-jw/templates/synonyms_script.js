const wordInput = document.getElementById('wordInput');
const solveBtn = document.getElementById('solveBtn');
const resetBtn = document.getElementById('resetBtn');
const statusEl = document.getElementById('status');
const dictionaryList = document.getElementById('dictionaryList');
const relatedList = document.getElementById('relatedList');
const simpleList = document.getElementById('simpleList');
const simpleView = document.getElementById('simpleView');
const advancedView = document.getElementById('advancedView');
const simpleViewBtn = document.getElementById('simpleViewBtn');
const advancedViewBtn = document.getElementById('advancedViewBtn');
const dictionarySection = document.getElementById('dictionarySection');
const relatedSection = document.getElementById('relatedSection');
const relatedHeader = document.getElementById('relatedHeader');
const definitionCache = new Map();
let popoverEl = null;
let popoverWord = '';
let loading = false;
let viewMode = 'advanced';
let lastResult = null;

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

function getStoredViewMode() {
  try {
    const stored = localStorage.getItem('synonymsViewMode');
    if (stored === 'simple' || stored === 'advanced') {
      return stored;
    }
  } catch (_) {
    /* ignored */
  }
  return 'advanced';
}

function storeViewMode(mode) {
  try {
    localStorage.setItem('synonymsViewMode', mode);
  } catch (_) {
    /* ignored */
  }
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

function applyViewMode() {
  if (viewMode === 'simple') {
    simpleView.classList.remove('d-none');
    advancedView.classList.add('d-none');
    dictionarySection.classList.add('d-none');
    relatedHeader.classList.add('d-none');
    relatedSection.classList.remove('col-lg-7');
    relatedSection.classList.add('col-12');
    simpleViewBtn.classList.add('active');
    advancedViewBtn.classList.remove('active');
    simpleViewBtn.setAttribute('aria-pressed', 'true');
    advancedViewBtn.setAttribute('aria-pressed', 'false');
    if (lastResult) {
      renderSimpleView(lastResult.synsets || [], lastResult.normalized || '');
    }
  } else {
    simpleView.classList.add('d-none');
    advancedView.classList.remove('d-none');
    dictionarySection.classList.remove('d-none');
    relatedHeader.classList.remove('d-none');
    relatedSection.classList.remove('col-12');
    relatedSection.classList.add('col-lg-7');
    advancedViewBtn.classList.add('active');
    simpleViewBtn.classList.remove('active');
    simpleViewBtn.setAttribute('aria-pressed', 'false');
    advancedViewBtn.setAttribute('aria-pressed', 'true');
  }
  hidePopover();
}

function setViewMode(mode) {
  if (mode !== 'simple' && mode !== 'advanced') return;
  viewMode = mode;
  storeViewMode(mode);
  applyViewMode();
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
  const el = e.target.closest('[data-word]');
  if (!el) return;
  const word = el.dataset.word;
  if (!word) return;
  handleDefinitionRequest(word, el);
}

function handleResultKeydown(e) {
  if (e.key !== 'Enter' && e.key !== ' ') return;
  const el = e.target.closest('[data-word]');
  if (!el) return;
  e.preventDefault();
  const word = el.dataset.word;
  if (!word) return;
  handleDefinitionRequest(word, el);
}

function clearResults() {
  dictionaryList.innerHTML = '';
  relatedList.innerHTML = '';
  simpleList.innerHTML = '';
  lastResult = null;
  hidePopover();
}

function relationOrder(kind) {
  const order = [
    'hypernyms',
    'hyponyms',
    'similar_to',
    'antonyms',
    'derivations',
    'also_see',
    'entails',
    'causes',
    'verb_group',
    'attributes',
    'participle',
    'pertainyms',
    'member_meronyms',
    'part_meronyms',
    'substance_meronyms',
    'member_holonyms',
    'part_holonyms',
    'substance_holonyms',
    'topic_domain',
    'topic_members',
    'region_domain',
    'region_members',
    'usage_domain',
    'usage_members',
  ];
  const idx = order.indexOf(kind);
  return idx === -1 ? order.length : idx;
}

function renderDictionary(synsets) {
  dictionaryList.innerHTML = '';
  if (!synsets || !synsets.length) {
    dictionaryList.innerHTML = '<div class="text-muted">No definitions found.</div>';
    return;
  }
  synsets.slice(0, 8).forEach(s => {
    const card = document.createElement('div');
    card.className = 'card shadow-sm';
    const body = document.createElement('div');
    body.className = 'card-body';
    const header = document.createElement('div');
    header.className = 'd-flex justify-content-between align-items-center mb-2';
    const lemmas = document.createElement('div');
    lemmas.className = 'fw-semibold';
    lemmas.textContent = (s.lemmas || []).map(prettyLemma).join(', ');
    lemmas.dataset.word = (s.lemmas && s.lemmas[0]) || '';
    lemmas.setAttribute('role', 'button');
    lemmas.tabIndex = 0;
    const pos = document.createElement('span');
    pos.className = 'badge text-bg-light text-dark';
    pos.textContent = s.pos || '';
    header.appendChild(lemmas);
    header.appendChild(pos);

    const def = document.createElement('div');
    def.textContent = s.definition || '';
    body.appendChild(header);
    body.appendChild(def);
    if (s.examples && s.examples.length) {
      const example = document.createElement('div');
      example.className = 'text-muted small mt-2';
      example.textContent = `“${s.examples[0]}”`;
      body.appendChild(example);
    }
    card.appendChild(body);
    dictionaryList.appendChild(card);
  });
}

function aggregateRelations(synsets) {
  const map = new Map();
  (synsets || []).forEach(s => {
    (s.relations || []).forEach(group => {
      const key = group.kind || group.label;
      if (!map.has(key)) {
        map.set(key, {
          kind: group.kind || key,
          label: group.label || key,
          symbol: group.symbol || '',
          targets: [],
        });
      }
      const entry = map.get(key);
      (group.targets || []).forEach(t => {
        const id = `${t.synset_id.pos}-${t.synset_id.offset}`;
        if (!entry.targets.some(existing => `${existing.synset_id.pos}-${existing.synset_id.offset}` === id)) {
          entry.targets.push(t);
        }
      });
    });
  });
  const groups = Array.from(map.values());
  groups.sort((a, b) => relationOrder(a.kind) - relationOrder(b.kind) || a.label.localeCompare(b.label));
  groups.forEach(group => {
    group.targets.sort((a, b) => {
      const sa = a.sense_count || 0;
      const sb = b.sense_count || 0;
      if (sa !== sb) return sb - sa;
      const la = (a.lemmas && a.lemmas[0]) ? a.lemmas[0].toLowerCase() : '';
      const lb = (b.lemmas && b.lemmas[0]) ? b.lemmas[0].toLowerCase() : '';
      return la.localeCompare(lb);
    });
  });
  return groups;
}

function collectWordsForSynset(synset, normalizedWord) {
  const words = new Map();
  const normalized = (normalizedWord || '').toLowerCase();
  const addWord = (lemma, kind) => {
    if (!lemma) return;
    const text = prettyLemma(lemma).trim();
    if (!text) return;
    const key = text.toLowerCase();
    if (key === normalized) return;
    if (!words.has(key)) {
      words.set(key, { word: text, antonym: kind === 'antonyms' });
    } else if (kind === 'antonyms') {
      const entry = words.get(key);
      entry.antonym = true;
      words.set(key, entry);
    }
  };

  (synset.lemmas || []).forEach(lemma => addWord(lemma, 'lemma'));
  (synset.relations || []).forEach(group => {
    (group.targets || []).forEach(target => {
      (target.lemmas || []).forEach(lemma => addWord(lemma, group.kind || 'other'));
    });
  });

  return Array.from(words.values()).sort((a, b) => a.word.localeCompare(b.word));
}

function renderSimpleView(synsets, normalizedWord) {
  simpleList.innerHTML = '';
  if (!synsets || !synsets.length) {
    simpleList.innerHTML = '<div class="text-muted">No related words found.</div>';
    return;
  }

  synsets.forEach(synset => {
    const card = document.createElement('div');
    card.className = 'card shadow-sm';
    const body = document.createElement('div');
    body.className = 'card-body';

    const header = document.createElement('div');
    header.className = 'd-flex align-items-start justify-content-between gap-3 mb-2';
    const def = document.createElement('div');
    def.className = 'fw-semibold';
    def.textContent = synset.definition || 'No definition available.';
    const pos = document.createElement('span');
    pos.className = 'badge text-bg-light text-dark';
    pos.textContent = synset.pos || '';
    header.appendChild(def);
    header.appendChild(pos);
    body.appendChild(header);

    const words = collectWordsForSynset(synset, normalizedWord);
    if (!words.length) {
      const empty = document.createElement('div');
      empty.className = 'text-muted mt-2';
      empty.textContent = 'No related words for this meaning.';
      body.appendChild(empty);
    } else {
      const grid = document.createElement('div');
      grid.className = 'row row-cols-1 row-cols-sm-2 g-2 mt-2';
      words.forEach(entry => {
        const col = document.createElement('div');
        col.className = 'col';
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'btn btn-outline-secondary word-chip w-100 text-start';
        if (entry.antonym) {
          btn.classList.add('antonym', 'btn-outline-danger');
        }
        btn.dataset.word = entry.word;
        btn.setAttribute('role', 'button');
        btn.tabIndex = 0;
        btn.textContent = entry.word;
        col.appendChild(btn);
        grid.appendChild(col);
      });
      body.appendChild(grid);
    }

    card.appendChild(body);
    simpleList.appendChild(card);
  });
}

function renderRelations(groups) {
  relatedList.innerHTML = '';
  if (!groups || !groups.length) {
    relatedList.innerHTML = '<div class="text-muted">No related words found.</div>';
    return;
  }
  groups.forEach(group => {
    const card = document.createElement('div');
    card.className = 'card shadow-sm';
    const body = document.createElement('div');
    body.className = 'card-body';
    const title = document.createElement('div');
    title.className = 'd-flex justify-content-between align-items-center mb-2';
    const label = document.createElement('h3');
    label.className = 'h6 mb-0';
    label.textContent = group.label;
    const symbol = document.createElement('span');
    symbol.className = 'badge text-bg-light text-dark';
    symbol.textContent = group.symbol || '';
    title.appendChild(label);
    title.appendChild(symbol);
    body.appendChild(title);

    if (!group.targets.length) {
      const empty = document.createElement('div');
      empty.className = 'text-muted';
      empty.textContent = 'None found.';
      body.appendChild(empty);
    } else {
      const list = document.createElement('ul');
      list.className = 'list-group results-list';
      group.targets.forEach(target => {
        const li = document.createElement('li');
        li.className = 'list-group-item';
        li.dataset.word = (target.lemmas && target.lemmas[0]) || '';
        li.setAttribute('role', 'button');
        li.tabIndex = 0;

        const header = document.createElement('div');
        header.className = 'd-flex justify-content-between align-items-center';
        const wordSpan = document.createElement('span');
        wordSpan.className = 'fw-semibold word-item';
        wordSpan.textContent = (target.lemmas || []).map(prettyLemma).join(', ');
        const pos = document.createElement('span');
        pos.className = 'badge text-bg-light text-dark';
        pos.textContent = target.pos || '';
        header.appendChild(wordSpan);
        header.appendChild(pos);
        li.appendChild(header);

        if (target.definition) {
          const def = document.createElement('div');
          def.className = 'text-muted small mt-1';
          def.textContent = target.definition;
          li.appendChild(def);
        }
        list.appendChild(li);
      });
      body.appendChild(list);
    }
    card.appendChild(body);
    relatedList.appendChild(card);
  });
}

async function runSearch() {
  const word = wordInput.value.trim().toLowerCase();
  if (!word) {
    statusEl.textContent = 'Word is required.';
    statusEl.classList.remove('text-danger');
    statusEl.classList.add('text-muted');
    return;
  }
  if (loading) return;
  loading = true;
  statusEl.classList.remove('text-danger');
  statusEl.classList.add('text-muted');
  statusEl.textContent = 'Loading...';
  clearResults();
  try {
    const resp = await fetch(`/v1/wordnet/related?word=${encodeURIComponent(word)}`);
    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(text || 'Failed to fetch related words.');
    }
    const data = await resp.json();
    lastResult = data;
    renderDictionary(data.synsets || []);
    const groups = aggregateRelations(data.synsets || []);
    renderRelations(groups);
    renderSimpleView(data.synsets || [], data.normalized || word);
    applyViewMode();
    if (data.note) {
      statusEl.textContent = data.note;
    } else {
      statusEl.textContent = 'Click any word to see definitions.';
    }
  } catch (err) {
    lastResult = null;
    statusEl.classList.remove('text-muted');
    statusEl.classList.add('text-danger');
    const message = err.message || 'Error fetching suggestions.';
    statusEl.textContent = message;
    relatedList.innerHTML = `<div class="text-danger">${escapeHtml(message)}</div>`;
    dictionaryList.innerHTML = '';
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
dictionaryList.addEventListener('click', handleResultClick);
relatedList.addEventListener('click', handleResultClick);
simpleList.addEventListener('click', handleResultClick);
dictionaryList.addEventListener('keydown', handleResultKeydown);
relatedList.addEventListener('keydown', handleResultKeydown);
simpleList.addEventListener('keydown', handleResultKeydown);
wordInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    runSearch();
  }
});
simpleViewBtn.addEventListener('click', () => setViewMode('simple'));
advancedViewBtn.addEventListener('click', () => setViewMode('advanced'));

document.addEventListener('click', (e) => {
  if (!popoverEl || popoverEl.style.display !== 'block') return;
  if (popoverEl.contains(e.target)) return;
  if (relatedList.contains(e.target) || dictionaryList.contains(e.target) || simpleList.contains(e.target)) return;
  hidePopover();
});

window.addEventListener('scroll', () => hidePopover(), true);
window.addEventListener('resize', () => hidePopover());

viewMode = getStoredViewMode();
applyViewMode();
statusEl.textContent = 'Enter a word to search.';
