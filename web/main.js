// The page around the client (crates/eqoa/src/web): a gate that asks for the
// player's disc image, then the game on the canvas from its boot (web::boot:
// the loading screens, the licence and the title, Station's login screen, the
// server list, character select and creation, the world). Query parameters
// override the defaults: ?relay=ws://127.0.0.1:8790&server=203.0.113.10 for a
// local server. Reports, notices and what goes wrong are report.js's
// (window.eqoaReport).
import init, { boot, view, client_state, want_shot, crash_test, disc_id } from './eqoa.js';

const $ = (id) => document.getElementById(id);
// the client's own graphics line ("graphics: <adapter> (<backend>)"), for reports
let clientGraphics = '';
const status = (t) => {
  $('status').textContent = t;
  if (t.startsWith('graphics: ')) clientGraphics = t.slice(10);
};
window.eqoaStatus = status;
// report.js's, when it loaded (it is a module of its own, before this one)
const report = window.eqoaReport || null;
const problem = (key, text) => (report ? report.problem(key, text) : console.warn(text));

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
  }).catch((e) => problem('sound-worklet', `No sound: the page's sound worklet (sound.js) did not load (${e}).`));
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

// The gamepads' motors: the Gamepad API's dual-rumble, where the browser has
// it. `strong` is the DualShock 2's large motor's level, `weak` its small
// one, each 0..1, for `ms`; both 0 stops them. `pads`: the indices of the
// pads this window reads (its Controller Focus), every one when absent.
const rumblers = () => [...(navigator.getGamepads ? navigator.getGamepads() : [])].filter((g) => g && g.connected && g.vibrationActuator);
window.eqoaCanRumble = () => rumblers().length > 0;

// The client's windows open in this browser (multibox.rs): each page takes
// the first free place, a Web Lock `eqoa-window-<n>` held while it is open
// (the browser lets go as it closes), and looks twice a second at the places
// held. Which controller each window reads, the client keeps in
// localStorage. Without Web Locks a page is alone.
const windows = { ready: false, place: -1, live: [] };
window.eqoaWindows = windows;
(async () => {
  if (!navigator.locks) return;
  for (let n = 1; n <= 16 && windows.place < 0; n++) {
    const held = await new Promise((done) => {
      navigator.locks.request(`eqoa-window-${n}`, { ifAvailable: true }, (lock) => {
        done(lock !== null);
        return lock ? new Promise(() => {}) : undefined;
      }).catch(() => done(false));
    });
    if (held) windows.place = n - 1;
  }
  const look = () => navigator.locks.query().then((q) => {
    windows.live = (q.held || []).map((l) => /^eqoa-window-(\d+)$/.exec(l.name)).filter(Boolean).map((m) => +m[1] - 1);
  }).catch(() => {});
  await look();
  setInterval(look, 500);
})().catch(() => {}).finally(() => { windows.ready = true; });

// A touch-first device (its main pointer is coarse: a phone or a tablet): the
// client shows its on-screen pad from the start.
window.eqoaTouchFirst = () => matchMedia('(pointer: coarse)').matches;
window.eqoaRumble = (strong, weak, ms, pads) => {
  for (const g of rumblers()) {
    if (Array.isArray(pads) && !pads.includes(g.index)) continue;
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
  drop(k) { try { localStorage.removeItem('eqoa.' + k); } catch { /* as above */ } },
};
const session = {
  get(k) { try { return sessionStorage.getItem('eqoa.' + k) || ''; } catch { return ''; } },
  set(k, v) { try { sessionStorage.setItem('eqoa.' + k, v); } catch { /* as above */ } },
};
const wsBase = (location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host;
// What the page offers when nothing is given: kept only when the player
// changed it, so that a new default reaches everyone who did not.
const defaults = { relay: wsBase + '/eqoa/relay', server: '', viewargs: '' };
for (const k of Object.keys(defaults)) $(k).value = params.get(k) || store.get(k) || defaults[k];
function keepFields() {
  for (const k of Object.keys(defaults)) {
    const v = $(k).value.trim();
    if (v && v !== defaults[k]) store.set(k, v);
    else store.drop(k);
  }
}

// ---- views

function show(id) {
  $('panel').hidden = id === null;
  for (const v of ['gate']) $(v).hidden = v !== id;
}

function error(where, text) {
  $(where + '-error').textContent = text || '';
}

// ---- the gate: the disc image, then the game from its boot

let started = false; // the game or the viewer began: a reload would end it

// The game server's IPv4 address when the game's Network Setup names none:
// given, or this page's host as the relay resolves it (the relay reaches
// only the server's own ports anyway).
async function serverAddress(relay) {
  const given = $('server').value.trim();
  if (given) return given;
  const host = location.hostname;
  if (/^\d+\.\d+\.\d+\.\d+$/.test(host)) return host;
  const r = await fetch(relay.replace(/^ws/, 'http') + '/resolve?name=' + encodeURIComponent(host));
  if (!r.ok) throw new Error('the relay could not resolve ' + host);
  return (await r.text()).trim();
}

function busy(on) {
  for (const b of ['go', 'view']) $(b).disabled = on;
}

$('gate').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  startAudio(); // in the gesture itself, before anything is awaited
  const file = $('iso').files[0];
  if (!file) return error('gate', 'Pick your disc image first.');
  keepFields();
  started = true;
  busy(true);
  error('gate', '');
  try {
    const relay = $('relay').value.trim().replace(/\/+$/, '');
    let server = '';
    try {
      server = await serverAddress(relay);
    } catch { /* the game's Network Setup can name one */ }
    // resolves once the game is running on the canvas (window.eqoaBooted)
    await boot(file, { relay, server });
  } catch (e) {
    show('gate');
    busy(false);
    error('gate', String(e && e.message ? e.message : e));
  }
});

window.eqoaBooted = () => {
  show(null);
  $('eqoa').focus();
  // the status line goes right: the HUD's report button is at the bottom left
  document.body.classList.add('playing');
  if (report) report.running(true);
};

$('view').addEventListener('click', async () => {
  const file = $('iso').files[0];
  if (!file) return error('gate', 'Pick your disc image first.');
  keepFields();
  started = true;
  busy(true);
  try {
    show(null);
    $('eqoa').focus();
    await view(file, $('viewargs').value.trim() || 'CHAR.ESF 0x70D898C6');
    if (report) report.running(true);
  } catch (e) {
    show('gate');
    busy(false);
    error('gate', String(e && e.message ? e.message : e));
  }
});

// The server ended the game's connection: the page starts over, so the
// sockets, the sound and the GPU go with it, and the gate says why.
window.eqoaDisconnected = (text) => {
  session.set('notice', text);
  location.reload();
};
{
  const notice = session.get('notice');
  if (notice) {
    session.set('notice', '');
    error('gate', notice);
  }
}

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

// (name, UTIL.REL, the .AUT, fresh) -> the module; the client calls it
// (web::unpack_modules), with `fresh` when the copy kept from before failed
// its check: unpacked again, and that copy replaced.
window.eqoaUnpack = async (name, util, aut, fresh) => {
  const file = await cacheName(name, util, aut).catch(() => null);
  if (file && !fresh) {
    const hit = await cached(file).catch(() => null);
    if (hit) return hit;
  }
  const out = await unpackInWorker(util, aut);
  if (file) keep(file, out).catch(() => { /* no storage here: unpacked again next time */ });
  return out;
};

// ---- the map's pictures (the client's map, M: map/store.rs): each zone's
// tile and overview cell, kept in this origin's private storage under
// map/<the world's key>, so a world's map is drawn once. A write commits
// whole on close; a page with no storage (plain http to another host) keeps
// nothing and draws the map again each visit.

async function mapDir(key, create) {
  const root = await navigator.storage.getDirectory();
  return (await root.getDirectoryHandle('map', { create })).getDirectoryHandle(key, { create });
}

window.eqoaMapGet = async (key, name) => {
  try {
    const f = await (await (await mapDir(key, false)).getFileHandle(name)).getFile();
    return new Uint8Array(await f.arrayBuffer());
  } catch {
    return null;
  }
};

window.eqoaMapPut = async (key, name, bytes) => {
  try {
    const w = await (await (await mapDir(key, true)).getFileHandle(name, { create: true })).createWritable();
    await w.write(bytes);
    await w.close();
  } catch { /* no storage here: drawn again next time */ }
};

window.eqoaMapList = async (key) => {
  const out = [];
  try {
    for await (const name of (await mapDir(key, false)).keys()) out.push(name);
  } catch { /* none kept */ }
  return out;
};

// the same world's older pictures (another build of the map or of the
// world's file): directories named for its file but not `key`
window.eqoaMapPrune = async (key) => {
  try {
    const stem = key.slice(0, key.lastIndexOf('-'));
    const map = await (await navigator.storage.getDirectory()).getDirectoryHandle('map', { create: true });
    for await (const name of map.keys()) {
      if (name !== key && name.slice(0, name.lastIndexOf('-')) === stem) await map.removeEntry(name, { recursive: true });
    }
  } catch { /* nothing to let go */ }
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

// ---- the disc image: which disc it is, by the program it starts
// (web::disc_id), as soon as it is picked, so a disc other than Frontiers
// is named before a session finds out without its HUD.

let discLine = '';
$('iso').addEventListener('change', async () => {
  const file = $('iso').files[0];
  const line = $('iso-what');
  line.textContent = '';
  line.className = 'note';
  discLine = '';
  if (!file) return;
  try {
    await ready;
    const [ok, text] = await disc_id(file);
    line.className = 'note ' + (ok ? 'ok' : 'warn');
    line.textContent = text;
    discLine = `${text} (${file.size} bytes)`;
  } catch (e) {
    line.className = 'note warn';
    line.textContent = String(e && e.message ? e.message : e);
    discLine = `${line.textContent} (${file.size} bytes)`;
  }
});

// ---- updates: this page is one build (web/build.sh puts each in
// v/<build>/, kept as it is for good), and version.json beside the page names
// the newest. A tab left open asks now and then: on an untouched gate
// it reloads at once, nothing being lost; anywhere else it says so, and the
// player reloads when it suits them.

const BUILD = (import.meta.url.match(/\/v\/([0-9a-f]+)\/main\.js(?:$|[?#])/) || [])[1] || null;
window.eqoaBuild = BUILD;
let latest = null;     // the newest build version.json named, when last asked
let asked = 0;
let laterFor = null;   // the build the player said "later" to

async function checkUpdate(force) {
  if (!BUILD) return null; // not a built page (no v/<build>/): nothing to compare
  if (!force && Date.now() - asked < 10000) return latest;
  asked = Date.now();
  try {
    const r = await fetch(new URL('../../version.json', import.meta.url), { cache: 'no-store' });
    if (!r.ok) return latest;
    latest = (await r.json()).build || null;
  } catch {
    return latest; // offline, or the host is down: nothing to say
  }
  if (!report) return latest;
  if (!latest || latest === BUILD) {
    report.notice('update');
    return latest;
  }
  const untouched = !started && !$('iso').files.length;
  if (session.get('reloadedFor') === latest) {
    // reloaded for it once already and still this build: something between
    // here and the server keeps the old page
    problem('stale-page', `This page is build ${BUILD}, and the server's is ${latest}; a reload did not bring it. Something between the browser and the server may be keeping the old page: try a hard reload (Ctrl+F5, or Ctrl+Shift+R).`);
    return latest;
  }
  if (untouched) {
    session.set('reloadedFor', latest);
    location.reload();
    return latest;
  }
  if (laterFor !== latest) {
    report.notice('update', '', started
      ? 'A new version of the client is out. Reloading leaves the game, and you sign in again.'
      : 'A new version of the client is out.', [
      ['Reload', () => { session.set('reloadedFor', latest); location.reload(); }, true],
      ['Later', () => { laterFor = latest; report.notice('update'); }],
    ]);
  }
  return latest;
}
window.eqoaCheckUpdate = () => checkUpdate(true);
setInterval(checkUpdate, 60000);
document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') checkUpdate(); });
addEventListener('pageshow', (e) => { if (e.persisted) checkUpdate(); });

// ---- reports (report.js): what this page adds to each, and the gate's button

if (report) {
  report.graphics = () => {
    if (/\(Gl\)$/.test(clientGraphics)) return 'WebGL2';
    if (/WebGpu/.test(clientGraphics)) return 'WebGPU';
    const g = $('gfx-state').textContent;
    return /WebGPU/.test(g) ? 'WebGPU' : /WebGL2/.test(g) ? 'WebGL2' : '';
  };
  report.facts = () => [
    `update: ${!BUILD ? 'not a built page' : !latest ? 'not asked yet' : latest === BUILD ? 'this is the newest' : `the server has ${latest}`}`,
    `graphics: ${$('gfx-state').textContent}${clientGraphics ? `; the client's: ${clientGraphics}` : ''}`,
    `disc: ${discLine || ($('iso').files.length ? 'picked, not identified' : 'none picked')}`,
    `panel: ${$('gate').hidden ? 'none (the game)' : 'gate'}, status: ${$('status').textContent}`,
    `relay ${$('relay').value || '-'}, server ${$('server').value || '(this host)'}`,
    `sound: ${JSON.stringify(window.eqoaAudio.stats())}`,
  ];
}
$('report-gate').onclick = () => {
  const e = $('gate-error').textContent.trim();
  if (report) report.open(e ? `The page said: ${e}` : '');
};

checkGraphics();
const ready = init({ module_or_path: wasmModule });
ready.then(() => {
  window.eqoaClient = { state: client_state, wantShot: want_shot, crashTest: crash_test };
  status('ready');
}).catch((e) => {
  status('could not load the client: ' + e);
  problem('load', `The client did not load (${e}). Reload the page; if it keeps happening, send a report.`);
});
