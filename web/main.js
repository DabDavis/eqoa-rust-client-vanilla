// The page around the client (crates/eqoa/src/web): the login or a new
// account, the character screen (select, delete, create) the session drives,
// then the game on the canvas. Query parameters override the defaults:
// ?relay=ws://127.0.0.1:8790&server=203.0.113.10 for a local server, and
// ?account= for the account service (default: /eqoa/account on this host).
import init, { play, view, choose_character, create_character, delete_character, random_name } from './eqoa.js';

const RACES = ['Human', 'Elf', 'Dark Elf', 'Gnome', 'Dwarf', 'Troll', 'Barbarian', 'Halfling', 'Erudite', 'Ogre', 'Froglok'];
const CITIES = { 1: 'Freeport', 2: 'Qeynos' };
const CLASSES = ['Warrior', 'Ranger', 'Paladin', 'Shadowknight', 'Monk', 'Bard', 'Rogue', 'Druid', 'Shaman',
  'Cleric', 'Magician', 'Necromancer', 'Enchanter', 'Wizard', 'Alchemist'];
const STATS = ['Strength', 'Stamina', 'Agility', 'Dexterity', 'Wisdom', 'Intelligence', 'Charisma'];
const POINTS = 20;

const $ = (id) => document.getElementById(id);
const status = (t) => { $('status').textContent = t; };
window.eqoaStatus = status;

// ---- sound: an AudioContext at 48 kHz, made on the click that starts a
// session (a page may only start sound from its user's gesture), playing
// through an AudioWorklet (sound.js) what the client mixes on its own thread
// a little ahead of this context's clock (crates/eqoa/src/audio.rs, `feed`).

let audio = null;

function startAudio() {
  if (audio) {
    if (audio.ctx.state !== 'running') audio.ctx.resume().catch(() => {});
    return;
  }
  let ctx;
  try {
    ctx = new AudioContext({ sampleRate: 48000, latencyHint: 'interactive' });
  } catch {
    try {
      ctx = new AudioContext(); // the device's rate: the worklet resamples
    } catch {
      return;
    }
  }
  audio = { ctx, node: null, stats: {} };
  ctx.audioWorklet.addModule(new URL('sound.js', import.meta.url)).then(() => {
    const node = new AudioWorkletNode(ctx, 'eqoa-out', { numberOfInputs: 0, outputChannelCount: [2] });
    node.port.onmessage = ({ data }) => { audio.stats = data; };
    node.connect(ctx.destination);
    audio.node = node;
  }).catch((e) => console.log('no sound: ' + e));
}

window.eqoaAudio = {
  // 48 kHz frames this context has played, -1 while it is not running
  clock: () => (audio && audio.node && audio.ctx.state === 'running' ? audio.ctx.currentTime * 48000 : -1),
  push: (pcm) => { if (audio && audio.node) audio.node.port.postMessage(pcm, [pcm.buffer]); },
  stats: () => (audio ? { state: audio.ctx.state, rate: audio.ctx.sampleRate, ...audio.stats } : { state: 'none' }),
};

// a context the browser kept suspended starts on the next key or click
for (const ev of ['pointerdown', 'keydown']) {
  addEventListener(ev, () => { if (audio && audio.ctx.state === 'suspended') audio.ctx.resume().catch(() => {}); });
}

// F11 is the browser's own full screen. The canvas cancels every key it is
// given (winit's listener), so F11 is kept from it, caught on its way down
// before the canvas sees it, and left to the browser; the canvas follows the
// window's new size.
for (const ev of ['keydown', 'keyup']) {
  addEventListener(ev, (e) => { if (e.key === 'F11') e.stopImmediatePropagation(); }, true);
}

// The gamepads' motors (the client reads the pads through gilrs, which drives
// no motors in a page): the Gamepad API's dual-rumble, where the browser has
// it. `strong` is the DualShock 2's large motor's level, `weak` its small
// one, each 0..1, for `ms`; both 0 stops them.
const rumblers = () => [...(navigator.getGamepads ? navigator.getGamepads() : [])].filter((g) => g && g.connected && g.vibrationActuator);
window.eqoaCanRumble = () => rumblers().length > 0;
window.eqoaRumble = (strong, weak, ms) => {
  for (const g of rumblers()) {
    const a = g.vibrationActuator;
    if (strong === 0 && weak === 0) {
      if (a.reset) a.reset().catch(() => {});
    } else {
      a.playEffect('dual-rumble', { startDelay: 0, duration: ms, strongMagnitude: strong, weakMagnitude: weak }).catch(() => {});
    }
  }
};

// The client's wasm, compiled once: the page runs it, and so does the worker
// that unpacks CLIENT off the disc (unpack.js), from the same compiled module.
const wasmModule = fetch(new URL('eqoa_bg.wasm', import.meta.url)).then((r) => {
  if (!r.ok) throw new Error(`the client did not load (${r.status})`);
  return (r.headers.get('Content-Type') || '').startsWith('application/wasm')
    ? WebAssembly.compileStreaming(r)
    : r.arrayBuffer().then((b) => WebAssembly.compile(b));
});

const params = new URLSearchParams(location.search);
const store = {
  get(k) { try { return localStorage.getItem('eqoa.' + k) || ''; } catch { return ''; } },
  set(k, v) { try { localStorage.setItem('eqoa.' + k, v); } catch { /* private window */ } },
};
const wsBase = (location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host;
const accountBase = (params.get('account') || '/eqoa/account').replace(/\/+$/, '');
for (const k of ['user', 'relay', 'server', 'viewargs']) $(k).value = params.get(k) || store.get(k);
if (!$('relay').value) $('relay').value = wsBase + '/eqoa/relay';

// ---- views

function show(id) {
  $('panel').hidden = id === null;
  for (const v of ['login', 'chars', 'create']) $(v).hidden = v !== id;
}

function error(where, text) {
  $(where + '-error').textContent = text || '';
}

// ---- log in, or a new account first

let mode = 'login';
function setMode(m) {
  mode = m;
  $('tab-login').classList.toggle('on', m === 'login');
  $('tab-new').classList.toggle('on', m === 'new');
  $('newonly').hidden = m !== 'new';
  $('pass2').required = m === 'new';
  $('pass').autocomplete = m === 'new' ? 'new-password' : 'current-password';
  $('go').textContent = m === 'new' ? 'Create account' : 'Log in';
  error('login', '');
}
$('tab-login').onclick = () => setMode('login');
$('tab-new').onclick = () => setMode('new');

// The game server's IPv4 address: given, or this page's host as the relay
// resolves it (the relay reaches only the server's own ports anyway).
async function serverAddress(relay) {
  const given = $('server').value.trim();
  if (given) return given;
  const host = location.hostname;
  if (/^\d+\.\d+\.\d+\.\d+$/.test(host)) return host;
  const r = await fetch(relay.replace(/^ws/, 'http') + '/resolve?name=' + encodeURIComponent(host));
  if (!r.ok) throw new Error('the relay could not resolve ' + host);
  return (await r.text()).trim();
}

async function register(user, pass) {
  const r = await fetch(accountBase + '/api/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ username: user, password: pass }),
  });
  if (!r.ok) throw new Error('the account service answered ' + r.status);
  const j = await r.json();
  if (!j.success) throw new Error(j.error || 'the account was not made');
}

function busy(on) {
  for (const b of ['go', 'view']) $(b).disabled = on;
}

$('login').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  startAudio(); // in the gesture itself, before anything is awaited
  const file = $('iso').files[0];
  const user = $('user').value.trim();
  const pass = $('pass').value;
  if (!file) return error('login', 'Pick your disc image first.');
  if (mode === 'new' && pass !== $('pass2').value) return error('login', 'The passwords differ.');
  for (const k of ['user', 'relay', 'server', 'viewargs']) store.set(k, $(k).value.trim());
  busy(true);
  error('login', '');
  try {
    if (mode === 'new') {
      status('making the account');
      await register(user, pass);
      status('account made; logging in');
    }
    const relay = $('relay').value.trim().replace(/\/+$/, '');
    const server = await serverAddress(relay);
    // Resolves once the game is running; meanwhile the session calls the
    // character screen's hooks below.
    await play(file, user, pass, { relay, server, character: params.get('character') || '' });
  } catch (e) {
    show('login');
    busy(false);
    error('login', String(e && e.message ? e.message : e));
  }
});

$('view').addEventListener('click', async () => {
  const file = $('iso').files[0];
  if (!file) return error('login', 'Pick your disc image first.');
  store.set('viewargs', $('viewargs').value.trim());
  busy(true);
  try {
    show(null);
    $('eqoa').focus();
    await view(file, $('viewargs').value.trim() || 'CHAR.ESF 0x70D898C6');
  } catch (e) {
    show('login');
    busy(false);
    error('login', String(e && e.message ? e.message : e));
  }
});

// ---- the character screen

let characters = [];
let creating = null;   // the name sent, until the list or a refusal answers

window.eqoaCharacters = (list) => {
  characters = list;
  if (creating !== null) {
    const made = list.some((c) => c.name.toLowerCase() === creating.toLowerCase());
    creating = null;
    $('c-make').disabled = false;
    if (!made) return; // a refusal said why; stay on the form
  }
  const ul = $('charlist');
  ul.replaceChildren();
  for (const c of list) {
    const li = document.createElement('li');
    const who = document.createElement('div');
    who.className = 'who';
    const name = document.createElement('div');
    name.className = 'name';
    name.textContent = c.name;
    const what = document.createElement('div');
    what.className = 'what';
    what.textContent = `Level ${c.level} ${RACES[c.race] || 'race ' + c.race} ${CLASSES[c.class] || 'class ' + c.class}`;
    who.append(name, what);
    const playB = document.createElement('button');
    playB.className = 'primary';
    playB.textContent = 'Play';
    playB.onclick = () => { error('chars', ''); choose_character(c.serverId); };
    const del = document.createElement('button');
    del.textContent = 'Delete';
    del.onclick = () => {
      if (confirm(`Delete ${c.name}? This cannot be undone.`)) delete_character(c.serverId);
    };
    li.append(who, del, playB);
    ul.append(li);
  }
  $('nochars').hidden = list.length > 0;
  $('new').disabled = list.length >= 8;
  show('chars');
};

window.eqoaRefused = (text) => {
  if (!$('create').hidden) {
    creating = null;
    $('c-make').disabled = false;
    error('create', text);
  } else {
    error('chars', text);
  }
};

window.eqoaRandomName = (name) => { $('c-name').value = name; };
window.eqoaEntering = (name) => { show(null); status('entering the world as ' + name); };
window.eqoaInWorld = () => { show(null); $('eqoa').focus(); };

$('logout').onclick = () => location.reload();

// ---- character creation: what the server's templates allow
// (GET /api/creation: one entry per race, city and class, with its stats)

let combos = null;
let spent = new Array(7).fill(0);

async function loadCombos() {
  if (combos) return combos;
  const r = await fetch(accountBase + '/api/creation');
  if (!r.ok) throw new Error('character creation is not offered here (' + r.status + ')');
  combos = (await r.json()).combos;
  return combos;
}

function homeKey(c) { return c.race + ':' + c.city; }

function fillSelect(sel, items) {
  sel.replaceChildren(...items.map(([v, t]) => Object.assign(document.createElement('option'), { value: v, textContent: t })));
}

function fillHomes() {
  const seen = new Map();
  for (const c of combos) {
    const k = homeKey(c);
    if (!seen.has(k)) seen.set(k, (RACES[c.race] || 'Race ' + c.race) + (c.race === 0 && CITIES[c.city] ? ` (${CITIES[c.city]})` : ''));
  }
  fillSelect($('c-race'), [...seen.entries()].sort((a, b) => a[1].localeCompare(b[1])));
}

function current() {
  const [race, city] = $('c-race').value.split(':').map(Number);
  const cls = Number($('c-class').value);
  return { race, city, cls, combo: combos.find((c) => c.race === race && c.city === city && c.class === cls) };
}

function fillClasses() {
  const [race, city] = $('c-race').value.split(':').map(Number);
  const keep = $('c-class').value;
  const list = combos.filter((c) => c.race === race && c.city === city).map((c) => [c.class, CLASSES[c.class] || 'Class ' + c.class]);
  fillSelect($('c-class'), list.sort((a, b) => a[1].localeCompare(b[1])));
  if (list.some(([v]) => String(v) === keep)) $('c-class').value = keep;
  fillStats();
}

function fillStats() {
  const { combo } = current();
  const base = combo ? combo.stats : new Array(7).fill(60);
  const left = POINTS - spent.reduce((a, b) => a + b, 0);
  $('c-points').textContent = left;
  const t = $('c-stats');
  t.replaceChildren();
  STATS.forEach((name, i) => {
    const tr = document.createElement('tr');
    const td = (text, cls) => Object.assign(document.createElement('td'), { textContent: text, className: cls || '' });
    const minus = Object.assign(document.createElement('button'), { type: 'button', textContent: '−', disabled: spent[i] === 0 });
    const plus = Object.assign(document.createElement('button'), { type: 'button', textContent: '+', disabled: left === 0 });
    minus.setAttribute('aria-label', 'less ' + name);
    plus.setAttribute('aria-label', 'more ' + name);
    minus.onclick = () => { spent[i]--; fillStats(); };
    plus.onclick = () => { spent[i]++; fillStats(); };
    const b = document.createElement('td');
    b.append(minus, ' ', plus);
    tr.append(td(name), td(String(base[i] + spent[i]), 'n'), td(spent[i] ? '+' + spent[i] : '', 'n'), b);
    t.append(tr);
  });
}

function numbered(n) { return Array.from({ length: n }, (_, i) => [i, String(i + 1)]); }
fillSelect($('c-face'), numbered(4));
fillSelect($('c-hstyle'), numbered(4));
fillSelect($('c-hlength'), numbered(4));
fillSelect($('c-hcolor'), numbered(8));

$('new').onclick = async () => {
  error('chars', '');
  try {
    await loadCombos();
  } catch (e) {
    return error('chars', String(e.message || e));
  }
  fillHomes();
  fillClasses();
  spent = new Array(7).fill(0);
  // the look's defaults are the appearance page's (VIWndCharCreateMisc::Init)
  $('c-face').value = 3; $('c-hstyle').value = 3; $('c-hlength').value = 3; $('c-hcolor').value = 0;
  $('c-name').value = '';
  error('create', '');
  fillStats();
  show('create');
};
$('c-race').onchange = () => { spent = new Array(7).fill(0); fillClasses(); };
$('c-class').onchange = () => { spent = new Array(7).fill(0); fillStats(); };
$('c-back').onclick = () => show('chars');
$('c-random').onclick = () => {
  const { race } = current();
  random_name(race, Number($('c-sex').value));
};

$('create').addEventListener('submit', (ev) => {
  ev.preventDefault();
  const name = $('c-name').value.trim();
  if (!/^[A-Za-z]{3,15}$/.test(name)) return error('create', 'A name is 3 to 15 letters.');
  const { race, city, cls, combo } = current();
  if (!combo) return error('create', 'Pick a class.');
  error('create', '');
  creating = name;
  $('c-make').disabled = true;
  create_character({
    name, race, city, class: cls, sex: Number($('c-sex').value),
    face: Number($('c-face').value), hairStyle: Number($('c-hstyle').value),
    hairLength: Number($('c-hlength').value), hairColor: Number($('c-hcolor').value),
    stats: spent.slice(),
  });
});

$('eqoa').addEventListener('pointerdown', () => $('eqoa').focus());

// ---- a DNAS module off the disc (CLIENT, for the start menu): unpacked by
// the disc's own library in a worker (unpack.js), and kept in this origin's
// private storage under the SHA-256 of what went in, so a later visit reads
// it back instead of unpacking it again.

async function cacheName(name, util, aut) {
  if (!crypto.subtle) return null; // not a secure context: no cache
  const both = new Uint8Array(util.length + aut.length);
  both.set(util);
  both.set(aut, util.length);
  const h = new Uint8Array(await crypto.subtle.digest('SHA-256', both));
  return `${name}-${Array.from(h, (b) => b.toString(16).padStart(2, '0')).join('')}.snr1`;
}

async function cacheDir() {
  return (await navigator.storage.getDirectory()).getDirectoryHandle('dnas', { create: true });
}

async function cached(file) {
  const f = await (await (await cacheDir()).getFileHandle(file)).getFile();
  const b = new Uint8Array(await f.arrayBuffer());
  // an SNR1 module, not a write that never finished
  return b.length > 4 && b[0] === 0x53 && b[1] === 0x4e && b[2] === 0x52 && b[3] === 0x31 ? b : null;
}

async function keep(file, b) {
  const w = await (await (await cacheDir()).getFileHandle(file, { create: true })).createWritable();
  await w.write(b);
  await w.close();
}

function unpackInWorker(util, aut) {
  return new Promise((resolve, reject) => {
    const w = new Worker(new URL('unpack.js', import.meta.url), { type: 'module' });
    w.onmessage = ({ data }) => {
      w.terminate();
      if (data.error) reject(new Error(data.error));
      else resolve(data.out);
    };
    w.onerror = (e) => {
      w.terminate();
      reject(new Error(e.message || 'the unpacking worker failed'));
    };
    wasmModule.then((module) => w.postMessage({ module, util, aut }, [util.buffer, aut.buffer]), reject);
  });
}

// (name, UTIL.REL, the .AUT) -> the module; the client calls it (web::unpack_client).
window.eqoaUnpack = async (name, util, aut) => {
  const file = await cacheName(name, util, aut).catch(() => null);
  if (file) {
    const hit = await cached(file).catch(() => null);
    if (hit) return hit;
  }
  const out = await unpackInWorker(util, aut);
  if (file) keep(file, out).catch(() => { /* no storage here: unpacked again next time */ });
  return out;
};

// ---- graphics: which path the client will take (web::gfx asks for a WebGPU
// adapter the same way, else WebGL2), and how to get WebGPU where it is off.
// A page cannot change a browser's flags or open chrome:// itself, so the
// help copies each address for the player to paste.

function el(tag, props, ...kids) {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
}

function flag(url) {
  const copy = el('button', { type: 'button', className: 'small', textContent: 'Copy' });
  copy.onclick = async () => {
    try {
      await navigator.clipboard.writeText(url);
      copy.textContent = 'Copied: paste it in the address bar';
    } catch {
      const r = document.createRange();
      r.selectNodeContents(code);
      getSelection().removeAllRanges();
      getSelection().addRange(r);
      copy.textContent = 'Selected: copy it with Ctrl+C';
    }
  };
  const code = el('code', { textContent: url });
  return el('div', { className: 'flag' }, code, copy);
}

function browser() {
  const ua = navigator.userAgent;
  const brands = (navigator.userAgentData && navigator.userAgentData.brands || []).map((b) => b.brand);
  const chromium = brands.some((b) => /Chromium/.test(b)) || /Chrome\/|Chromium\/|Edg\//.test(ua);
  return {
    chromium,
    firefox: /Firefox\//.test(ua),
    safari: !chromium && /Safari\//.test(ua),
    linux: /Linux/.test(ua) && !/Android/.test(ua),
  };
}

function gfxHelp(b, why) {
  const help = $('gfx-help');
  help.replaceChildren();
  if (b.chromium && b.linux) {
    help.append(
      el('p', { textContent: why + ' Chromium-based browsers on Linux keep WebGPU behind two flags. To turn it on:' }),
      el('ol', {},
        el('li', {}, 'Paste this in the address bar and set it to Enabled:', flag('chrome://flags/#enable-unsafe-webgpu')),
        el('li', {}, 'The same for this one:', flag('chrome://flags/#enable-vulkan')),
        el('li', { textContent: 'Press Relaunch at the bottom of the flags page, then reload this page.' })));
  } else if (b.chromium) {
    help.append(
      el('p', { textContent: why + ' WebGPU is normally on in this browser, so graphics acceleration is probably off. To turn it on:' }),
      el('ol', {},
        el('li', {}, 'Open this and turn on "Use graphics acceleration when available":', flag('chrome://settings/system')),
        el('li', { textContent: 'Relaunch the browser, then reload this page.' }),
        el('li', {}, 'Still not on? This page says why:', flag('chrome://gpu'))));
  } else if (b.firefox) {
    help.append(el('p', { textContent: why + ' Firefox offers WebGPU only on some systems (Windows, from Firefox 141); elsewhere it cannot be switched on. For WebGPU, open this page in a Chromium-based browser.' }));
  } else if (b.safari) {
    help.append(el('p', { textContent: why + ' Safari has WebGPU from Safari 26: update the system, or open this page in a Chromium-based browser.' }));
  } else {
    help.append(el('p', { textContent: why + ' For WebGPU, open this page in a Chromium-based browser.' }));
  }
}

async function checkGraphics() {
  const state = $('gfx-state');
  const b = browser();
  const say = (cls, text) => { state.className = 'state ' + cls; state.textContent = text; };
  if (params.has('webgl')) {
    say('meh', 'Graphics: WebGL2, chosen by ?webgl in the address.');
    return;
  }
  let adapter = null;
  if (navigator.gpu) {
    try { adapter = await navigator.gpu.requestAdapter(); } catch { /* none */ }
  }
  const info = adapter && adapter.info || {};
  if (adapter && !(info.architecture === 'swiftshader' || adapter.isFallbackAdapter || info.isFallbackAdapter)) {
    say('ok', 'Graphics: WebGPU' + (info.vendor ? ', on ' + info.vendor.toUpperCase() : '') + '. The fastest path.');
    return;
  }
  if (adapter) {
    say('meh', 'Graphics: WebGPU, but drawn in software (no GPU). It will be slow.');
    gfxHelp(b, 'The browser is not using the graphics card.');
    return;
  }
  let renderer = '';
  try {
    const gl = document.createElement('canvas').getContext('webgl2');
    const ext = gl && gl.getExtension('WEBGL_debug_renderer_info');
    renderer = gl ? String(gl.getParameter(ext ? ext.UNMASKED_RENDERER_WEBGL : gl.RENDERER)) : '';
  } catch { /* no WebGL2 either: the client says so when it starts */ }
  if (/swiftshader|llvmpipe|softpipe|software/i.test(renderer)) {
    say('meh', 'Graphics: WebGL2, drawn in software (no GPU). It will be slow.');
    gfxHelp(b, 'The browser is not using the graphics card.');
    return;
  }
  say('meh', 'Graphics: WebGL2. It works, but costs about twice the CPU of WebGPU.');
  gfxHelp(b, 'WebGPU is not available here.');
}

checkGraphics();
setMode('login');
init({ module_or_path: wasmModule }).then(() => status('ready')).catch((e) => status('could not load the client: ' + e));
