// The community page (spellfx.html): every spell the server knows, filtered
// by class, and every visual on the player's own disc, tried in the preview
// (fxview.html beside this script, the GM panel's: a human casting a visual as
// a cast's 0x00B1 plays it in the game), and a form that sends the server the
// visual a player says a spell had. No sign-in: GMs compare the picks in the
// GM panel and set the right one. The disc image is read here and never sent.
//
// The server (go-eqoa-server's account service): GET <api>/spells answers
// {spells: [{id, name, level, classes, line, harmful, target, cast, visual,
// standin}]}; POST <api>/suggest {spell, visual, name, note} answers {ok,
// count} or {error}. ?api=<base> overrides <api> (a test's).

import { canRemember, keptHandle, keepHandle } from './discstore.js';

const $ = (id) => document.getElementById(id);
const params = new URLSearchParams(location.search);
const API = (params.get('api') || '/eqoa/account/api/spellfx').replace(/\/$/, '');
const CLASSES = ['Warrior', 'Ranger', 'Paladin', 'Shadow Knight', 'Monk', 'Bard', 'Rogue', 'Druid', 'Shaman', 'Cleric', 'Magician', 'Necromancer', 'Enchanter', 'Wizard', 'Alchemist'];
const SHORT = ['WAR', 'RAN', 'PAL', 'SK', 'MNK', 'BRD', 'RGE', 'DRD', 'SHA', 'CL', 'MAG', 'NEC', 'ENC', 'WIZ', 'ALC'];
// the body every training dummy wears (CHAR.ESF) and its rows' size, as the GM panel's preview has it
const DUMMY = { model: '0x66262F1C', size: 0.25 };
const store = {
  get(k) { try { return localStorage.getItem('eqoa.' + k) || ''; } catch { return ''; } },
  set(k, v) { try { localStorage.setItem('eqoa.' + k, v); } catch { /* private window */ } },
  drop(k) { try { localStorage.removeItem('eqoa.' + k); } catch { /* as above */ } },
};

// a DictID however it is written ('0xE78024D9', 'e78024d9', a number) as
// '0xE78024D9', or '' when it is none
function hex(v) {
  if (v === null || v === undefined || v === '') return '';
  const n = typeof v === 'number' ? v : parseInt(String(v).replace(/^0x/i, ''), 16);
  return Number.isFinite(n) && n > 0 ? '0x' + (n >>> 0).toString(16).toUpperCase().padStart(8, '0') : '';
}
const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
};
const status = (t, bad) => { $('status').textContent = t || ''; $('status').className = bad ? 'warn' : ''; };

const state = {
  spells: [], // as the server sends them, visual normalised
  byId: new Map(),
  visuals: [], // every visual on the disc, in its order
  spell: null, // the spell picked
  visual: '', // the visual tried last (the pick)
  playing: '', // what plays now
  disc: null, // the disc image (a File)
  frameReady: false,
  started: false,
  sent: '', // the visual the preview was started with
};

// ---- the spells
async function loadSpells() {
  try {
    const r = await fetch(API + '/spells', { cache: 'no-cache' });
    if (!r.ok) throw new Error(r.status + ' ' + r.statusText);
    const body = await r.json();
    state.spells = (body.spells || []).map((s) => ({ ...s, visual: hex(s.visual), classes: Array.isArray(s.classes) ? s.classes : [] }));
    state.byId = new Map(state.spells.map((s) => [s.id, s]));
    status('');
  } catch (e) {
    state.spells = [];
    status(`The spells could not be loaded (${e.message || e}): try again in a while.`, true);
  }
  drawSpells();
  drawVisuals();
}

function spellMatches(s) {
  const c = $('class').value;
  if (c !== '' && !s.classes.includes(Number(c))) return false;
  if ($('unknown').checked && !s.standin) return false;
  const q = $('spell-search').value.trim().toLowerCase();
  if (!q) return true;
  return s.name.toLowerCase().includes(q) || (s.line || '').toLowerCase().includes(q) || (q.length >= 3 && s.visual.toLowerCase().includes(q));
}

function drawSpells() {
  const list = $('spells');
  list.replaceChildren();
  const shown = state.spells.filter(spellMatches).sort((a, b) => a.level - b.level || a.name.localeCompare(b.name));
  $('spells-count').textContent = state.spells.length ? `${shown.length} of ${state.spells.length}` : '';
  if (!shown.length) {
    list.append(el('li', 'empty', state.spells.length ? 'No spell matches.' : 'No spells yet.'));
    return;
  }
  for (const s of shown) {
    const li = el('li');
    li.dataset.id = s.id;
    if (state.spell && state.spell.id === s.id) li.classList.add('on');
    const b = el('button');
    b.type = 'button';
    const top = el('div');
    top.append(el('span', 'name', s.name));
    top.append(el('span', 'tag ' + (s.standin ? 'stand' : 'known'), s.standin ? (s.kind ? `stand-in · ${s.kind}` : 'stand-in') : 'known'));
    b.append(top);
    b.append(el('div', 'meta', `Level ${s.level} · ${s.classes.map((c) => SHORT[c] || c).join(', ') || 'no class'} · plays ${s.visual || 'nothing'}`));
    if (s.line) b.append(el('div', 'line', s.line));
    b.title = s.line || s.name;
    b.addEventListener('click', () => pickSpell(s.id));
    li.append(b);
    list.append(li);
  }
}

// ---- the visuals
// the spells a visual is given to now (stand-ins apart)
function usersOf(v) {
  const known = [], stand = [];
  for (const s of state.spells) if (s.visual === v) (s.standin ? stand : known).push(s);
  return { known, stand };
}

function visualMatches(v) {
  const q = $('visual-search').value.trim().toLowerCase();
  if (!q) return true;
  if (v.toLowerCase().includes(q) || v.toLowerCase().slice(2).includes(q.replace(/^0x/, ''))) return true;
  return usersOf(v).known.some((s) => s.name.toLowerCase().includes(q));
}

function drawVisuals() {
  const list = $('visuals');
  list.replaceChildren();
  if (!state.visuals.length) {
    $('visuals-count').textContent = '';
    list.append(el('li', 'empty', state.disc ? 'Reading the visuals off your disc…' : 'Choose your disc image to list its visuals.'));
    return;
  }
  const shown = state.visuals.filter(visualMatches);
  $('visuals-count').textContent = `${shown.length} of ${state.visuals.length}`;
  if (!shown.length) {
    list.append(el('li', 'empty', 'No visual matches.'));
    return;
  }
  for (const v of shown) {
    const li = el('li');
    li.dataset.visual = v;
    if (v === state.visual) li.classList.add('on');
    const b = el('button');
    b.type = 'button';
    const top = el('div');
    top.append(el('span', 'name mono', v));
    top.append(el('span', 'meta', `  ${state.visuals.indexOf(v) + 1} of ${state.visuals.length}`));
    b.append(top);
    const { known, stand } = usersOf(v);
    const said = [];
    if (known.length) said.push('given to ' + known.slice(0, 3).map((s) => s.name).join(', ') + (known.length > 3 ? ` +${known.length - 3}` : ''));
    if (stand.length) said.push(`stand-in for ${stand.length} spell${stand.length === 1 ? '' : 's'}`);
    b.append(el('div', 'meta', said.join(' · ') || 'no spell has it yet'));
    b.addEventListener('click', () => tryVisual(v));
    li.append(b);
    list.append(li);
  }
}

// ---- the preview (fxview.html, this build's own copy, on this origin)
const frame = $('preview');
const frameUrl = new URL('fxview.html', import.meta.url);
frame.src = frameUrl.href;
const post = (m) => frame.contentWindow && frame.contentWindow.postMessage(m, location.origin);

function aimFor(spell) {
  const a = $('aim').value;
  if (a !== 'auto') return a;
  return spell ? (spell.harmful ? 'dummy' : 'caster') : 'dummy';
}
const castMs = (spell) => Math.round((spell && Number.isFinite(Number(spell.cast)) ? Number(spell.cast) : 2) * 1000);

const AIMS = { dummy: 'the training dummy', human: 'a second human', caster: 'the caster' };

function play(visual) {
  if (!visual) return;
  state.playing = visual;
  const msg = { effect: visual, castMs: castMs(state.spell), aim: aimFor(state.spell) };
  if (state.disc) {
    const as = state.spell ? ` as ${state.spell.name}'s cast (${(msg.castMs / 1000).toFixed(1)} s)` : '';
    $('caption').textContent = `Playing ${visual}${as}, landing on ${AIMS[msg.aim] || msg.aim}.`;
    $('caption').classList.remove('warn');
  }
  if (state.started) post({ type: 'fxview-play', ...msg });
  else startPreview(msg);
}

function startPreview(first) {
  if (!state.frameReady || !state.disc || state.started) return;
  const playMsg = first || { effect: state.playing || '', castMs: castMs(state.spell), aim: aimFor(state.spell) };
  state.sent = playMsg.effect;
  $('caption').textContent = 'reading your disc…';
  post({ type: 'fxview-disc', file: state.disc, play: playMsg, dummy: DUMMY });
}

addEventListener('message', (e) => {
  if (e.source !== frame.contentWindow || e.origin !== location.origin) return;
  const m = e.data || {};
  if (m.source !== 'eqoa-fxview') return;
  if (m.type === 'ready') {
    state.frameReady = true;
    startPreview();
  } else if (m.type === 'started') {
    state.started = true;
    state.visuals = (m.visuals || []).map(hex).filter(Boolean);
    drawVisuals();
    // picked while it was starting: that one now
    if (state.playing && state.playing !== state.sent) post({ type: 'fxview-play', effect: state.playing, castMs: castMs(state.spell), aim: aimFor(state.spell) });
  } else if (m.type === 'status') {
    // the preview says what plays over its own picture: here only its
    // progress while it starts, and a visual it does not have
    const t = m.text || '';
    if (!state.started || /not on this disc/.test(t)) {
      $('caption').textContent = t;
      $('caption').classList.toggle('warn', /not on this disc/.test(t));
    }
  } else if (m.type === 'error') {
    $('caption').textContent = m.text || 'The preview stopped.';
    $('caption').classList.add('warn');
  }
});

function pickSpell(id) {
  const s = state.byId.get(id);
  if (!s) return;
  state.spell = s;
  state.visual = '';
  for (const li of $('spells').children) li.classList.toggle('on', li.dataset.id === String(id));
  for (const li of $('visuals').children) li.classList.remove('on');
  drawPick();
  // what it plays now first, as its cast
  if (s.visual) play(s.visual);
}

function tryVisual(v) {
  state.visual = v;
  for (const li of $('visuals').children) li.classList.toggle('on', li.dataset.visual === v);
  drawPick();
  play(v);
}

// the visual before or after the one tried, in the list as shown
function step(by) {
  const shown = state.visuals.filter(visualMatches);
  if (!shown.length) return;
  const at = shown.indexOf(state.visual || state.playing);
  const v = shown[at < 0 ? (by > 0 ? 0 : shown.length - 1) : (at + by + shown.length) % shown.length];
  tryVisual(v);
  const li = [...$('visuals').children].find((x) => x.dataset.visual === v);
  if (li) li.scrollIntoView({ block: 'nearest' });
}

$('prev').addEventListener('click', () => step(-1));
$('next').addEventListener('click', () => step(1));
$('again').addEventListener('click', () => state.started && post({ type: 'fxview-again' }));
$('aim').addEventListener('change', () => { if (state.playing) play(state.playing); });

// ---- the form
function drawPick() {
  const s = state.spell;
  $('pick-spell').textContent = s ? `${s.name} (level ${s.level}${s.classes.length ? ', ' + s.classes.map((c) => CLASSES[c] || c).join(', ') : ''})` : 'pick one on the left';
  $('pick-visual').textContent = state.visual ? `${state.visual}${s && s.visual === state.visual ? ' (what it plays now)' : ''}` : 'try one from the list';
  $('send').disabled = !(s && state.visual);
  $('send').textContent = s && state.visual ? `This visual is ${s.name}'s` : 'Send my pick';
}

$('who').value = store.get('spellfxName');
$('suggest').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const s = state.spell;
  if (!s || !state.visual) return;
  const name = $('who').value.trim().slice(0, 32);
  if (name) store.set('spellfxName', name);
  else store.drop('spellfxName');
  const body = { spell: s.id, visual: state.visual, name, note: $('note').value.trim().slice(0, 200) };
  $('send').disabled = true;
  $('sent').className = 'note';
  $('sent').textContent = 'Sending…';
  try {
    const r = await fetch(API + '/suggest', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    let answer = {};
    try { answer = await r.json(); } catch { /* not JSON */ }
    if (!r.ok || !answer.ok) throw new Error(answer.error || `${r.status} ${r.statusText}`);
    $('sent').className = 'note ok';
    const n = Number(answer.count);
    $('sent').textContent = `Thanks! ${state.visual} for ${s.name} is sent` + (Number.isFinite(n) && n > 0 ? ` (${n} pick${n === 1 ? '' : 's'} for it so far).` : '.') + ' Send another for the same spell and it replaces this one.';
    $('note').value = '';
  } catch (e) {
    $('sent').className = 'note warn';
    $('sent').textContent = `Not sent: ${e.message || e}`;
  } finally {
    $('send').disabled = !(state.spell && state.visual);
    $('sent').scrollIntoView({ block: 'nearest' });
  }
});

// ---- the disc: picked here, or the one remembered (the game's page and
// this one share it: discstore.js)
function useDisc(file, again) {
  if (!file) return;
  state.disc = file;
  $('disc').hidden = true;
  $('disc-line').hidden = false;
  $('disc-name').textContent = `Disc: ${file.name}${again ? ' (remembered)' : ''}`;
  drawVisuals();
  if (state.started) {
    // a disc changed after the preview started: a fresh preview reads it
    state.started = false;
    state.frameReady = false;
    state.visuals = [];
    drawVisuals();
    frame.src = frameUrl.href + '?again=' + Date.now();
  } else {
    startPreview();
  }
}

$('iso').addEventListener('change', () => useDisc($('iso').files[0], false));
$('disc-change').addEventListener('click', () => { $('disc').hidden = false; $('disc-line').hidden = true; });

let kept = null; // {handle, name} remembered from an earlier visit
const rememberOn = () => canRemember && $('remember').checked;

async function takeKept(ask) {
  if (!kept) return false;
  let answer = 'prompt';
  try {
    answer = await (ask ? kept.handle.requestPermission({ mode: 'read' }) : kept.handle.queryPermission({ mode: 'read' }));
  } catch { answer = 'refused'; }
  if (answer !== 'granted') {
    if (ask) $('disc-what').textContent = `The browser did not allow reading ${kept.name}: choose it again.`;
    return false;
  }
  try {
    useDisc(await kept.handle.getFile(), true);
    return true;
  } catch {
    $('disc-what').textContent = `${kept.name} is no longer where it was: choose it again.`;
    $('iso-again').hidden = true;
    return false;
  }
}

if (canRemember) {
  $('remember-row').hidden = false;
  $('remember').checked = store.get('rememberDiscs') === '1';
  $('remember').addEventListener('change', () => {
    if ($('remember').checked) store.set('rememberDiscs', '1');
    else store.drop('rememberDiscs');
  });
  $('iso-again').addEventListener('click', () => takeKept(true));
  // ticked, the picker that can be remembered, in the click's own activation
  $('iso').addEventListener('click', (ev) => {
    if (!rememberOn()) return;
    ev.preventDefault();
    const opts = { id: 'eqoa-disc', types: [{ description: 'Disc image', accept: { 'application/octet-stream': ['.iso', '.bin', '.img'] } }] };
    window.showOpenFilePicker(opts).then(async ([handle]) => {
      const file = await handle.getFile();
      kept = { handle, name: file.name };
      await keepHandle('disc', kept);
      useDisc(file, false);
    }).catch((e) => {
      if (e && e.name === 'AbortError') return;
      $('disc-what').textContent = `The picker did not open (${e && e.message ? e.message : e}): untick Remember to choose it the browser's own way.`;
    });
  });
  (async () => {
    const r = await keptHandle('disc');
    if (!r || !r.handle || state.disc) return;
    kept = r;
    if (!(await takeKept(false))) {
      $('iso-again').textContent = `Use ${r.name} again`;
      $('iso-again').hidden = false;
      $('disc-what').textContent = 'Remembered from your last visit: press the button to use it.';
    }
  })();
}

// ---- the filters
const classSel = $('class');
CLASSES.forEach((name, i) => {
  const o = el('option', '', name);
  o.value = String(i);
  classSel.append(o);
});
classSel.value = /^\d+$/.test(params.get('class') || '') ? params.get('class') : store.get('spellfxClass');
classSel.addEventListener('change', () => { store.set('spellfxClass', classSel.value); drawSpells(); });
$('unknown').addEventListener('change', drawSpells);
let typing = 0;
const later = (fn) => () => { clearTimeout(typing); typing = setTimeout(fn, 120); };
$('spell-search').addEventListener('input', later(drawSpells));
$('visual-search').addEventListener('input', later(drawVisuals));

drawPick();
loadSpells();
window.eqoaSpellfx = state; // for the page's tests
