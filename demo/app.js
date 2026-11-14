const $ = (id) => document.getElementById(id);
const UNIT = 10n ** 24n;
let manifest, active, cursor = -1, timer = null, loadSequence = 0, proofSequence = 0;
let candidates = [];
const names = {
  publish_dataset: 'Specialist publishes the dataset root',
  open_job: 'Buyer escrows the agreed price',
  accept_job: 'Specialist accepts the job',
  deliver: 'Specialist submits a fact opening',
  expire_job: 'Observer closes the expired job',
  cancel_job: 'Buyer cancels before acceptance',
  close_job: 'Specialist closes the remaining work',
  withdraw: 'Participant withdraws available credit',
};

function amount(raw, signed = false) {
  const value = BigInt(raw ?? '0'), absolute = value < 0n ? -value : value;
  const whole = absolute / UNIT;
  const fraction = (absolute % UNIT).toString().padStart(24, '0').slice(0, 6).replace(/0+$/, '');
  if (absolute && !whole && !fraction) return `${value < 0n ? '−' : signed ? '+' : ''}<0.000001`;
  return `${value < 0n ? '−' : signed && value ? '+' : ''}${whole}${fraction ? `.${fraction}` : ''}`;
}
function setAmount(id, raw) { $(id).textContent = `${amount(raw)} NEAR`; $(id).title = `${raw ?? '0'} yoctoNEAR`; }
function short(text) { return `${text.slice(0, 9)}…${text.slice(-6)}`; }
function showError(error) { $('load-error').hidden = false; $('load-error').textContent = `Could not load verified execution records: ${error.message}. Serve this folder over HTTP; see Reproduce.`; }
function clear(element) { element.replaceChildren(); }
function node(tag, content, className) {
  const element = document.createElement(tag); element.textContent = content;
  if (className) element.className = className;
  return element;
}
async function sha(bytes) { return new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)); }
function hex(bytes) { return [...bytes].map((v) => v.toString(16).padStart(2, '0')).join(''); }
function unhex(text) {
  if (!/^[0-9a-f]{64}$/.test(text)) throw new Error('A proof hash must have 64 lowercase hex characters');
  return Uint8Array.from(text.match(/../g), (p) => parseInt(p, 16));
}
function concat(...parts) {
  const result = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0; for (const part of parts) { result.set(part, offset); offset += part.length; }
  return result;
}
const utf8 = (text) => new TextEncoder().encode(text);
function canonicalFact(entry) {
  return `{${Object.keys(entry).sort().map((key) => `${JSON.stringify(key)}:${JSON.stringify(entry[key])}`).join(',')}}`;
}
async function proofRoot(key, opening) {
  if (!Array.isArray(opening.path) || opening.path.length !== 32) throw new Error('Proof needs exactly 32 siblings');
  const entry = opening.entry;
  if (entry && (entry.kind !== 'fact' || entry.topic.toLowerCase() !== key)) throw new Error('Entry is for a different requested key');
  let hash = entry ? await sha(concat(Uint8Array.of(0), unhex(opening.salt), utf8(canonicalFact(entry)))) : await sha(Uint8Array.of(2));
  let empty = await sha(Uint8Array.of(2));
  const keyHash = await sha(concat(Uint8Array.of(3), utf8(JSON.stringify(['fact', key]))));
  let index = new DataView(keyHash.buffer).getUint32(0);
  for (const siblingHash of opening.path) {
    const sibling = siblingHash === null ? empty : unhex(siblingHash);
    hash = await sha(index % 2 === 0 ? concat(Uint8Array.of(1), hash, sibling) : concat(Uint8Array.of(1), sibling, hash));
    index = Math.floor(index / 2);
    empty = await sha(concat(Uint8Array.of(1), empty, empty));
  }
  return hex(hash);
}

function stopPlayback() {
  if (timer !== null) clearInterval(timer);
  timer = null; $('play').textContent = 'Replay run ↗';
}
function renderCursor() {
  const steps = active.steps;
  const job = steps.slice(0, cursor + 1).reverse().find((step) => step.job)?.job;
  const state = job?.close_reason ?? 'Not opened';
  const labels = { Open: job?.accepted ? 'ACCEPTED' : 'AWAITING SELLER', Fulfilled: 'FULFILLED', ResolvedWithAbsence: 'RESOLVED', WorkExpired: 'PARTIAL / EXPIRED', UnacceptedExpired: 'REFUNDED', BuyerCancelled: 'CANCELLED', SellerClosed: 'CLOSED' };
  $('job-status').textContent = labels[state] ?? 'BEFORE THE JOB';
  $('job-status').classList.toggle('pending', state === 'Open' || !job);
  setAmount('seller-paid', job?.paid); setAmount('buyer-refund', job?.refunded); setAmount('remaining', job?.remaining);
  $('step-label').textContent = cursor === steps.length - 1 ? 'Final state' : cursor < 0 ? 'Before the first call' : `Call ${cursor + 1} of ${steps.length}`;
  $('step-hint').textContent = cursor < 0 ? 'No job has opened yet.' : `${steps[cursor].method} · block ${steps[cursor].block_height} · ${steps[cursor].success ? 'success' : 'rejected'}`;
  $('previous').disabled = cursor < 0;
  $('next').disabled = cursor >= steps.length - 1;
  for (const [i, button] of [...$('timeline').querySelectorAll('button')].entries()) {
    if (i === cursor) button.setAttribute('aria-current', 'step'); else button.removeAttribute('aria-current');
    const dot = button.querySelector('.step-dot');
    dot.classList.toggle('done', i <= cursor && steps[i].success);
    dot.classList.toggle('failed', i <= cursor && !steps[i].success);
    dot.textContent = i <= cursor ? steps[i].success ? '✓' : '×' : '·';
  }
  $('checks').textContent = cursor === steps.length - 1 ? '✓ Ledger conserved · withdrawals confirmed' : cursor < 0 ? 'No price has been deposited yet.' : '✓ Ledger conserved at this call';
}

function renderTimeline() {
  clear($('timeline'));
  active.steps.forEach((step, i) => {
    const item = document.createElement('li'), button = document.createElement('button');
    button.className = 'step-button';
    const text = node('span', names[step.method] ?? step.method);
    if (!step.success) text.append(node('code', ' · REJECTED'));
    button.append(node('span', '·', 'step-dot'), text, node('span', step.signer.replace('.testnet', '')));
    button.title = `Block ${step.block_height}; ${step.gas_burnt} gas. ${step.hash}`;
    button.addEventListener('click', () => { stopPlayback(); cursor = i; renderCursor(); });
    item.append(button); $('timeline').append(item);
  });
}

function renderWallets() {
  clear($('wallet-rows'));
  for (const row of active.wallet_reconciliation) {
    if (row.account === 'observer.testnet' && row.wallet_delta_yocto === '0') continue;
    const tr = document.createElement('tr');
    tr.append(node('td', row.account === 'buyer.testnet' ? 'Buyer' : row.account === 'specialist.testnet' ? 'Specialist' : 'Observer'));
    for (const [key, signed] of [['economic_delta_yocto', true], ['gas_paid_yocto', false], ['retained_storage_paid_yocto', false], ['wallet_delta_yocto', true]]) {
      const td = node('td', amount(row[key], signed)); td.title = `${row[key]} yoctoNEAR`;
      if (signed) td.className = BigInt(row[key]) > 0n ? 'positive' : BigInt(row[key]) < 0n ? 'negative' : '';
      tr.append(td);
    }
    $('wallet-rows').append(tr);
  }
  $('exit-status').textContent = '✓ All available credits withdrawn. No job price or withdrawal is pending.';
}

function selectProof() {
  proofSequence++;
  const candidate = candidates[Number($('proof-choice').value)];
  $('proof-value').disabled = !candidate?.opening.entry;
  $('verify').disabled = !candidate;
  $('proof-value').value = candidate?.opening.entry?.value ?? (candidate ? '(proven empty slot)' : 'No delivery submitted');
  $('published-root').textContent = active.dataset.root;
  $('computed-root').textContent = '—';
  $('proof-source').textContent = candidate?.opening.entry?.source ?? 'An empty slot earns no seller payment. Dataset samples are constructed public fixtures.';
  if (candidate) void checkProof();
  else {
    $('proof-result').textContent = 'No opening was submitted in this run. The unfulfilled price was refunded.';
    $('proof-result').classList.remove('invalid');
  }
}

async function checkProof() {
  const token = ++proofSequence;
  const candidate = candidates[Number($('proof-choice').value)]; if (!candidate) return;
  const opening = structuredClone(candidate.opening);
  if (opening.entry) opening.entry.value = $('proof-value').value;
  $('proof-result').textContent = 'Recomputing 32 proof levels…';
  try {
    const root = await proofRoot(candidate.key, opening); if (token !== proofSequence) return;
    const valid = root === active.dataset.root;
    $('computed-root').textContent = root;
    $('proof-result').textContent = valid ? opening.entry ? '✓ Proof matches the published root.' : '✓ Empty slot proven. This fact earns no seller payment.' : '× Proof mismatch. This value does not open to the pinned root.';
    $('proof-result').classList.toggle('invalid', !valid);
  } catch (error) {
    if (token !== proofSequence) return;
    $('proof-result').textContent = `× ${error.message}`; $('proof-result').classList.add('invalid');
  }
}

async function loadCase(item) {
  stopPlayback(); const token = ++loadSequence; proofSequence++;
  try {
    if (!/^[a-z0-9-]+\.json$/.test(item.file)) throw new Error('Invalid evidence filename');
    const response = await fetch(`evidence/${item.file}`); if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (hex(await sha(bytes)) !== item.sha256) throw new Error('Execution file hash does not match manifest');
    const record = JSON.parse(new TextDecoder().decode(bytes));
    if (record.wasm_sha256 !== manifest.wasm_sha256 || !record.checks.ledger_conserved || !record.checks.wallets_reconciled || !record.checks.credits_withdrawn) throw new Error('Execution integrity checks are incomplete');
    if (token !== loadSequence) return;
    active = record; cursor = record.steps.length - 1;
    $('load-error').hidden = true;
    $('case-number').textContent = `CASE ${String(manifest.cases.indexOf(item) + 1).padStart(2, '0')}`;
    $('case-title').textContent = item.title; $('case-description').textContent = item.description;
    setAmount('job-price', active.job.price); $('key-count').textContent = `${active.job.keys.length} fact${active.job.keys.length === 1 ? '' : 's'} requested`;
    $('raw-record').href = `evidence/${item.file}`;
    for (const button of $('case-list').querySelectorAll('button')) button.setAttribute('aria-pressed', String(button.dataset.id === item.id));
    history.replaceState(null, '', `#${item.id}`);
    renderTimeline(); renderCursor(); renderWallets();
    candidates = active.steps.filter((step) => step.method === 'deliver').map((step) => ({ key: active.job.keys[step.args.index], opening: step.args.opening, accepted: step.success }));
    clear($('proof-choice'));
    candidates.forEach((proof, i) => { const option = node('option', `${proof.key} · ${proof.accepted ? 'accepted' : 'rejected'}`); option.value = String(i); $('proof-choice').append(option); });
    if (!candidates.length) { const option = node('option', 'No opening submitted'); option.value = '0'; $('proof-choice').append(option); }
    $('proof-choice').disabled = !candidates.length; selectProof();
  } catch (error) { if (token === loadSequence) showError(error); }
}

$('previous').addEventListener('click', () => { stopPlayback(); cursor = Math.max(-1, cursor - 1); renderCursor(); });
$('next').addEventListener('click', () => { stopPlayback(); cursor = Math.min(active.steps.length - 1, cursor + 1); renderCursor(); });
$('play').addEventListener('click', () => {
  if (!active) return;
  if (timer !== null) { stopPlayback(); return; }
  cursor = -1; renderCursor(); $('play').textContent = 'Pause replay Ⅱ';
  timer = setInterval(() => {
    cursor++; renderCursor();
    if (cursor === active.steps.length - 1) stopPlayback();
  }, 1000);
});
$('proof-choice').addEventListener('change', selectProof);
$('verify').addEventListener('click', () => void checkProof());
$('proof-value').addEventListener('keydown', (event) => { if (event.key === 'Enter') void checkProof(); });
$('proof-value').addEventListener('input', () => { proofSequence++; $('proof-result').textContent = 'Value changed. Check the proof again.'; $('computed-root').textContent = '—'; $('proof-result').classList.remove('invalid'); });

try {
  const response = await fetch('evidence/manifest.json'); if (!response.ok) throw new Error(`HTTP ${response.status}`);
  manifest = await response.json();
  if (manifest.schema_version !== 1 || !manifest.cases?.length) throw new Error('Unsupported execution manifest');
  clear($('case-list'));
  manifest.cases.forEach((item, i) => {
    const button = document.createElement('button'); button.className = 'case-button'; button.dataset.id = item.id; button.setAttribute('aria-pressed', 'false');
    button.append(node('span', String(i + 1).padStart(2, '0')), node('span', item.title), node('span', '↗', 'arrow'));
    button.addEventListener('click', () => void loadCase(item)); $('case-list').append(button);
  });
  await loadCase(manifest.cases.find((item) => `#${item.id}` === location.hash) ?? manifest.cases[0]);
} catch (error) { showError(error); }
