// The page around the client (crates/eqoa/src/web): a gate that asks for the
// player's disc image, then the game on the canvas from its boot (web::boot:
// the loading screens, the licence and the title, Station's login screen, the
// server list, character select and creation, the world). Query parameters
// override the defaults: ?relay=ws://127.0.0.1:8790&server=203.0.113.10 for a
// local server. Reports, notices and what goes wrong are report.js's
// (window.eqoaReport).
import init, { boot, view, client_state, want_shot, crash_test, disc_id, disc_2002, update_notices, hidden_tick } from './eqoa.js';
import { canRemember, keptHandle, keepHandle, forgetHandle } from './discstore.js';

const $ = (id) => document.getElementById(id);
// the client's own graphics line ("graphics: <adapter> (<backend>)"), for reports
let clientGraphics = '';
// a software renderer's name, as WebGL's unmasked renderer, WebGPU's adapter
// info or the client's adapter names it: SwiftShader, Mesa's llvmpipe,
// softpipe and lavapipe, "Software Rasterizer"/"Software Renderer", and
// Windows' WARP, "Microsoft Basic Render Driver" (F3 20261027-003944: a
// steady 27 fps, never told why). gpustart.rs keeps the same list natively.
const SOFTWARE = /swiftshader|llvmpipe|softpipe|lavapipe|software|basic render driver/i;
let software = false; // the browser draws without the graphics card (noteSoftware)
let softwareTold = false; // its notice was raised (once a page)
const status = (t) => {
  if (t.startsWith('graphics: ')) {
    clientGraphics = t.slice(10);
    if (SOFTWARE.test(clientGraphics)) noteSoftware(/\(Gl\)$/.test(clientGraphics) ? 'WebGL2' : 'WebGPU');
  }
  // in the world the status line keeps saying it
  $('status').textContent = software && t && document.body.classList.contains('playing') ? `${t} · drawn without the graphics card` : t;
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

// The mouse look asks for the pointer through winit, which drops the promise
// a browser answers requestPointerLock with. One refused (asked again too soon
// after the player let go of it with Escape, or a page not in front) is the
// client's to see, and it does (mouse.rs, `lock_seen`: no lock came, the drag
// goes on with the cursor), not the page failing: a line in the log, no
// problem notice (F3 20261023-193535).
{
  const lock = Element.prototype.requestPointerLock;
  if (lock) {
    Element.prototype.requestPointerLock = function (...args) {
      const p = lock.apply(this, args);
      if (p && typeof p.catch === 'function') p.catch((e) => console.log(`the pointer lock was refused: ${e && e.message ? e.message : e}`));
      return p;
    };
  }
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

// The game server when the game's Network Setup names none: given, or this
// page's host. Kept a name, not resolved here: the client resolves it through
// the relay for each connection itself, and fetches the server's patch over
// https by it, which an address has no certificate for.
async function serverAddress(relay) {
  const given = $('server').value.trim();
  if (given) return given;
  return location.hostname;
}

function busy(on) {
  for (const b of ['go', 'view']) $(b).disabled = on;
}

$('gate').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  startAudio(); // in the gesture itself, before anything is awaited
  const file = $('iso').files[0];
  if (!file) return error('gate', 'Pick your disc image first.');
  fullScreen(); // in the gesture too
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
  fullScreen();
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

// A phone's or a tablet's own keyboard for the game's edit boxes (Station's
// name and password, Network Setup's address, creation's name, play's editor):
// while the client says one has the keys (window.eqoaWantText), a touch on the
// canvas puts the focus in a hidden input (in the touch, so the keyboard comes
// up: a browser shows it only for a user's gesture), and what is typed there
// goes to the canvas as the key events a keyboard's would be. Nothing typed is
// kept or logged: the input holds a few spaces between keystrokes, and what
// changed is all that is read.
const typed = document.createElement('input');
typed.id = 'eqoatyped';
typed.type = 'text';
typed.setAttribute('aria-hidden', 'true');
typed.tabIndex = -1;
for (const [k, v] of [['autocomplete', 'off'], ['autocorrect', 'off'], ['autocapitalize', 'none'], ['spellcheck', 'false'], ['inputmode', 'text'], ['enterkeyhint', 'done']]) typed.setAttribute(k, v);
// out of sight, 16 px so no browser zooms to it, at the top so none scrolls to it
typed.style.cssText = 'position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;border:0;padding:0;font-size:16px;pointer-events:none';
document.body.append(typed);
const TYPED_REST = '    ';
let typedLast = TYPED_REST;
let wantText = false;
const keyTo = (key) => {
  for (const type of ['keydown', 'keyup']) $('eqoa').dispatchEvent(new KeyboardEvent(type, { key, code: '', cancelable: true }));
};
const typedRest = () => {
  typed.value = TYPED_REST;
  typedLast = TYPED_REST;
  try { typed.setSelectionRange(TYPED_REST.length, TYPED_REST.length); } catch { /* not focused */ }
};
typed.addEventListener('input', (e) => {
  // what changed since the last: characters taken away (Backspace each), then the ones put in
  const v = typed.value;
  let same = 0;
  while (same < v.length && same < typedLast.length && v[same] === typedLast[same]) same++;
  for (let k = same; k < typedLast.length; k++) keyTo('Backspace');
  for (const ch of v.slice(same)) keyTo(ch === '\n' ? 'Enter' : ch);
  typedLast = v;
  // a word still being put together by the keyboard is left alone until it is done
  if (!e.isComposing) typedRest();
});
typed.addEventListener('compositionend', typedRest);
typed.addEventListener('keydown', (e) => {
  // a keyboard's keys that put nothing in the input: as they are
  if (['Enter', 'Escape', 'Tab', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
    e.preventDefault();
    keyTo(e.key);
  } else if (e.key === 'Backspace' && typed.value === '') {
    keyTo('Backspace');
    typedRest();
  }
});
const typeHere = () => {
  typedRest();
  typed.focus({ preventScroll: true });
};
let lastTouch = false;
window.eqoaWantText = (on) => {
  wantText = !!on;
  if (!wantText && document.activeElement === typed) $('eqoa').focus();
  // the touch that opened the box still the user's gesture: the keyboard up at once
  // (where a browser allows it; else at the next touch)
  if (wantText && lastTouch && navigator.userActivation && navigator.userActivation.isActive) typeHere();
};
$('eqoa').addEventListener('pointerdown', (e) => {
  lastTouch = e.pointerType === 'touch';
  $('eqoa').focus();
});
// winit puts the focus on the canvas at every press: a touch while the game
// wants text puts it back in the input, the device's keyboard with it
$('eqoa').addEventListener('pointerup', (e) => { if (wantText && e.pointerType === 'touch') typeHere(); });

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
  const fallback = info.architecture === 'swiftshader' || adapter && adapter.isFallbackAdapter || info.isFallbackAdapter
    || SOFTWARE.test([info.vendor, info.architecture, info.device, info.description].join(' '));
  if (adapter && !fallback) {
    // a line the client's adapter may yet correct (noteSoftware)
    if (!software) say('ok', 'Graphics: WebGPU' + (info.vendor ? ', on ' + info.vendor.toUpperCase() : '') + '. The fastest path.');
    return;
  }
  if (adapter) {
    noteSoftware('WebGPU');
    return;
  }
  let renderer = '';
  try {
    const gl = document.createElement('canvas').getContext('webgl2');
    const ext = gl && gl.getExtension('WEBGL_debug_renderer_info');
    renderer = gl ? String(gl.getParameter(ext ? ext.UNMASKED_RENDERER_WEBGL : gl.RENDERER)) : '';
  } catch { /* no WebGL2 either: the client says so when it starts */ }
  if (SOFTWARE.test(renderer)) {
    noteSoftware('WebGL2');
    return;
  }
  if (software) return;
  say('meh', 'Graphics: WebGL2. It works, but costs about twice the CPU of WebGPU.');
  gfxHelp(b, 'WebGPU is not available here.');
}

// What the player can do about a browser drawing without the graphics card,
// in a few words for the notice: its graphics acceleration on, as its
// browser names it, and the graphics card's driver brought up to date.
function softwareFix(b) {
  if (b.chromium) return 'Turn on graphics acceleration (Chrome or Edge: Settings > System > "Use graphics acceleration when available", then relaunch; chrome://gpu shows whether it is on) and update the graphics card\'s driver.';
  if (b.firefox) return 'Turn on hardware acceleration (Settings > General > Performance: untick "Use recommended performance settings", tick "Use hardware acceleration when available", then restart Firefox) and update the graphics card\'s driver.';
  return 'Turn on the browser\'s graphics acceleration in its settings, restart it, and update the graphics card\'s driver.';
}

// The browser draws without the graphics card, through `api` ('WebGPU' or
// 'WebGL2'), as the page's probe or the client's adapter found: the card's
// line says so with the steps, the status line in the world from its next
// update (status), and a notice once a page.
function noteSoftware(api) {
  software = true;
  const b = browser();
  const state = $('gfx-state');
  state.className = 'state meh';
  state.textContent = `Graphics: ${api}, drawn without the graphics card (software). The game will be slow.`;
  const help = $('gfx-help');
  if (b.chromium) {
    help.replaceChildren(
      el('p', { textContent: 'The browser is not using the graphics card. To turn graphics acceleration on:' }),
      el('ol', {},
        el('li', {}, 'Open this and turn on "Use graphics acceleration when available":', flag('chrome://settings/system')),
        el('li', { textContent: 'Relaunch the browser, then reload this page.' }),
        el('li', {}, 'Still not on? This page says why:', flag('chrome://gpu')),
        el('li', { textContent: 'Update the graphics card\'s driver.' })));
  } else {
    help.replaceChildren(el('p', { textContent: 'The browser is not using the graphics card. ' + softwareFix(b) }));
  }
  if (softwareTold) return;
  softwareTold = true;
  const text = `The browser is drawing without the graphics card, so the game will be slow. ${softwareFix(b)}`;
  if (report) report.notice('software-graphics', '', text, [['Dismiss', () => report.notice('software-graphics')]]);
  else console.warn(text);
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
    discLine = `${text} (${file.size} bytes${$('iso').dataset.again ? ', remembered from the last visit' : ''})`;
  } catch (e) {
    line.className = 'note warn';
    line.textContent = String(e && e.message ? e.message : e);
    discLine = `${line.textContent} (${file.size} bytes)`;
  }
});

// iOS turns an input's `accept` into file types and knows none for a disc
// image's .iso, .bin or .img, so its picker greys the image out in every
// browser there (all of them WebKit; Chrome on an iPhone, 2026-10-27: "it
// doesn't let you select the ISO"). The disc is judged here anyway (SYSTEM.CNF,
// `disc_id`), so on iOS the inputs take any file.
const onIOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
if (onIOS) for (const id of ['iso', 'iso2002']) $(id).removeAttribute('accept');

// ---- the 2002 disc: EverQuest Online Adventures (USA), SLUS-20470, whose
// art F9 swaps in (crates/eqoa/src/era.rs). Optional, judged by the client's
// rules as it is picked (web::disc_2002) and held by the client; picked on
// the gate, or when F9 asks for it in the game. A disc refused is let go of,
// so the line says what is read.

let disc2002Line = '';
$('iso2002').addEventListener('change', async () => {
  const file = $('iso2002').files[0] || null;
  const line = $('iso2002-what');
  line.textContent = '';
  line.className = 'note';
  disc2002Line = '';
  try {
    await ready;
    const [ok, text] = await disc_2002(file);
    if (file) {
      line.className = 'note ' + (ok ? 'ok' : 'warn');
      line.textContent = text;
      disc2002Line = `${text} (${file.size} bytes)`;
    }
    // in the game the gate is hidden: a refusal is a notice; a disc taken
    // clears F9's ask, and the swap it was pressed for goes on
    if (report && started) {
      if (ok || !file) report.notice('disc2002');
      else report.notice('disc2002', 'bad', text, [['Choose another', () => $('iso2002').click(), true], ['Close', () => report.notice('disc2002')]]);
    }
  } catch (e) {
    line.className = 'note warn';
    line.textContent = String(e && e.message ? e.message : e);
    disc2002Line = `${line.textContent} (${file ? file.size : 0} bytes)`;
  }
  if (started) $('eqoa').focus();
});

// F9 with no 2002 disc in hand (web::hooks_2002, era::ask): the picker opened
// at once, in the key's own user activation, and a notice whose button opens
// it again (a browser that would not open it from a key, a player who
// closed it).
window.eqoaAsk2002 = () => {
  // a 2002 disc remembered from an earlier visit (`remembering`): asked for
  // in this key's activation, the picker only when it cannot be had
  if (rememberOn() && remembered.disc2002) {
    takeAgain('iso2002').then((ok) => { if (ok && report) report.notice('disc2002'); });
  } else {
    try { $('iso2002').click(); } catch { /* the notice's button does */ }
  }
  if (report) {
    report.notice('disc2002', '', 'The 2002 art comes off your own EverQuest Online Adventures (USA) disc image, SLUS-20470: pick it, and F9 swaps the art.', [
      ['Choose the disc', () => $('iso2002').click(), true],
      ['Not now', () => report.notice('disc2002')],
    ]);
  }
};

// ---- remembering the disc images (jib's F3 20261025-214009: "Remember
// location"): with the box ticked, a disc is picked through the File System
// Access picker (Chromium-based browsers; `showOpenFilePicker`, its `id`
// opening it where the last image was), and its handle is kept in this
// origin's IndexedDB, never the image: the next visit puts it back in its
// input at once when the browser still allows reading it, else offers it on
// a button ("Use <file> again", whose click asks the browser). A file moved
// or deleted is said so and forgotten. Unticked (the default), or in a
// browser without the picker, the inputs are the browser's own, as before.

// per input: where its handle is kept, and its picker's own memory of a folder
const discInputs = { iso: { key: 'disc', id: 'eqoa-disc' }, iso2002: { key: 'disc2002', id: 'eqoa-disc-2002' } };
// what is kept, read once at the start: { handle, name } by key
const remembered = {};
const rememberOn = () => canRemember && $('remember').checked;
// what a report says of it (report.facts' `remember:` line, F3
// 20261026-014934: "did not actually remember"): the browser's last answers
// by disc, and the picker's last error; names only, never where a file is
const rememberLog = { query: {}, request: {}, error: '' };

// keptHandle, keepHandle, forgetHandle: discstore.js (the community page reads them too)

// the file put in its input as if picked there (its change handler names it)
function putFile(name, file, again) {
  const dt = new DataTransfer();
  dt.items.add(file);
  $(name).files = dt.files;
  $(name).dataset.again = again ? '1' : '';
  $(name + '-again-row').hidden = true;
  $(name).dispatchEvent(new Event('change'));
}

function saySoft(name, text) {
  const line = $(name + '-what');
  line.className = 'note warn';
  line.textContent = text;
}

// The kept handle's file, read now: false when it cannot be (moved, deleted,
// not allowed). `ask`: asking the browser first (in a click's activation).
async function takeAgain(name, ask = true) {
  const d = discInputs[name];
  const r = remembered[d.key];
  if (!r) return false;
  let state = 'prompt';
  try {
    state = await (ask ? r.handle.requestPermission({ mode: 'read' }) : r.handle.queryPermission({ mode: 'read' }));
  } catch (e) {
    // an old handle, or asked with no activation: as not allowed
    state = `refused (${e && e.name ? e.name : e})`;
  }
  rememberLog[ask ? 'request' : 'query'][d.key] = state;
  if (state !== 'granted') {
    if (ask) saySoft(name, `The browser did not allow reading ${r.name}: choose it again.`);
    return false;
  }
  try {
    putFile(name, await r.handle.getFile(), true);
    return true;
  } catch {
    // moved, renamed or deleted since
    delete remembered[d.key];
    forgetHandle(d.key);
    $(name + '-again-row').hidden = true;
    saySoft(name, `${r.name} is no longer where it was last time: choose it again.`);
    return false;
  }
}

// A kept disc the browser must ask for first: on a button, and said on the
// disc's line, where the eye goes; the Start button asks for it too.
function offerAgain(name) {
  const r = remembered[discInputs[name].key];
  const b = $(name + '-again');
  b.dataset.mode = 'again';
  b.textContent = `Use ${r.name} again`;
  $(name + '-again-row').hidden = false;
  const line = $(name + '-what');
  line.className = 'note';
  line.textContent = name === 'iso'
    ? 'Remembered from your last visit: press Start or the button above to use it (the browser may ask you to allow it).'
    : 'Remembered from your last visit: the button above uses it (the browser may ask you to allow it).';
}

// A disc picked the browser's own way before Remember was ticked has no
// handle to keep: a button that picks it once more in the picker that can.
function offerKeep(name) {
  const b = $(name + '-again');
  b.dataset.mode = 'keep';
  b.textContent = `Remember ${$(name).files[0].name}: choose it once more`;
  $(name + '-again-row').hidden = false;
}

// The picker that can be remembered, in a click's (or a tick's) activation:
// it opens where the kept image is, else where this picker last opened (its
// `id`); a start it cannot open in is tried once more without.
function pickRemembered(name) {
  const d = discInputs[name];
  const opts = { id: d.id, types: [{ description: 'Disc image', accept: { 'application/octet-stream': ['.iso', '.bin', '.img'] } }] };
  const start = remembered[d.key] ? remembered[d.key].handle : null;
  const open = (o) => window.showOpenFilePicker(o).then(async ([handle]) => {
    const file = await handle.getFile();
    remembered[d.key] = { handle, name: file.name };
    keepHandle(d.key, remembered[d.key]);
    rememberLog.error = '';
    putFile(name, file, false);
  });
  const failed = (e) => {
    if (e && e.name === 'AbortError') {
      // closed without a pick: a disc picked the other way is still unkept
      if ($(name).files.length && !remembered[d.key]) offerKeep(name);
      return;
    }
    rememberLog.error = `${e && e.name ? e.name : 'Error'}: ${e && e.message ? e.message : e}`;
    saySoft(name, `The picker did not open (${e && e.message ? e.message : e}): click to choose it again, or untick Remember to use the browser's own.`);
    if ($(name).files.length && !remembered[d.key]) offerKeep(name);
  };
  open(start ? { ...opts, startIn: start } : opts).catch((e) => {
    if (start && !(e && e.name === 'AbortError') && navigator.userActivation && navigator.userActivation.isActive) {
      const note = `${e && e.name ? e.name : 'Error'} starting where it was`;
      return open(opts).then(() => { rememberLog.error = `${note} (opened without it)`; }, failed);
    }
    failed(e);
  });
}

// For a report: what is kept, what the browser said, what is offered.
function rememberFacts() {
  if (!canRemember) return 'not in this browser (it has no File System Access picker)';
  const parts = [$('remember').checked ? 'on' : 'off'];
  for (const name of Object.keys(discInputs)) {
    const d = discInputs[name];
    const r = remembered[d.key];
    const said = [rememberLog.query[d.key] && `checked: ${rememberLog.query[d.key]}`,
      rememberLog.request[d.key] && `asked: ${rememberLog.request[d.key]}`].filter(Boolean).join(', ');
    const row = $(name + '-again-row');
    parts.push(`${d.key} ${r ? `kept "${r.name}"` : 'not kept'}${said ? ` (${said})` : ''}`
      + `${row.hidden ? '' : `, offered: "${$(name + '-again').textContent}"`}`
      + `${$(name).dataset.again ? ', in use from the last visit' : ''}`);
  }
  parts.push(`last picker error: ${rememberLog.error || 'none'}`);
  return parts.join('; ');
}

if (canRemember) {
  // a pick in the browser's own picker is not the last visit's
  for (const name of Object.keys(discInputs)) $(name).addEventListener('input', () => { $(name).dataset.again = ''; });
  $('remember-row').hidden = false;
  $('remember').checked = store.get('rememberDiscs') === '1';
  $('remember-what').hidden = !$('remember').checked;
  for (const name of Object.keys(discInputs)) {
    const d = discInputs[name];
    // its button: a kept disc's "Use ... again" asks the browser for it;
    // "Remember ...: choose it once more" opens the picker that can keep it
    $(name + '-again').addEventListener('click', () => {
      if ($(name + '-again').dataset.mode === 'keep') pickRemembered(name);
      else takeAgain(name);
    });
    // the picker that can be remembered, in the click's own activation
    $(name).addEventListener('click', (ev) => {
      if (!rememberOn()) return; // the browser's own picker
      ev.preventDefault();
      pickRemembered(name);
    });
    // a disc dropped on its input arrives the browser's own way, but a
    // dropped file can give its handle (Chromium's getAsFileSystemHandle,
    // asked for inside the drop): kept as a picked one is
    $(name).addEventListener('drop', (ev) => {
      if (!rememberOn()) return;
      const item = [...((ev.dataTransfer && ev.dataTransfer.items) || [])].find((i) => i.kind === 'file');
      if (!item || typeof item.getAsFileSystemHandle !== 'function') return;
      item.getAsFileSystemHandle().then(async (handle) => {
        if (!handle || handle.kind !== 'file') return;
        const file = await handle.getFile();
        remembered[d.key] = { handle, name: file.name };
        keepHandle(d.key, remembered[d.key]);
        $(name + '-again-row').hidden = true;
      }).catch(() => { /* taken the browser's own way only */ });
    });
  }
  $('remember').addEventListener('change', () => {
    const on = $('remember').checked;
    $('remember-what').hidden = !on;
    if (on) {
      store.set('rememberDiscs', '1');
      // A disc already picked the browser's own way has no handle to keep,
      // and the box sits under the disc, so it is often ticked after the
      // pick (F3 20261026-014934: nothing was remembered): the picker that
      // can keep one opens at once, in this tick's activation, for the first
      // such disc; another, or one the picker cannot open for here, gets a
      // button that opens it.
      let opened = false;
      for (const name of Object.keys(discInputs)) {
        if (!$(name).files.length || remembered[discInputs[name].key]) continue;
        if (!opened && !(navigator.userActivation && !navigator.userActivation.isActive)) {
          opened = true;
          pickRemembered(name);
        } else {
          offerKeep(name);
        }
      }
    } else {
      store.drop('rememberDiscs');
      for (const name of Object.keys(discInputs)) {
        delete remembered[discInputs[name].key];
        forgetHandle(discInputs[name].key);
        $(name + '-again-row').hidden = true;
      }
    }
  });
  // what the last visit kept: back in its input when still allowed, else
  // offered (a disc picked meanwhile, by a quick player or a test, stays)
  (async () => {
    if (!$('remember').checked) return;
    for (const name of Object.keys(discInputs)) {
      const r = await keptHandle(discInputs[name].key);
      if (!r || !r.handle) continue;
      remembered[discInputs[name].key] = r;
      if ($(name).files.length) continue;
      if (!(await takeAgain(name, false)) && remembered[discInputs[name].key]) offerAgain(name);
    }
  })();
  // Start with no disc in its input but one kept: the browser asked for it
  // in Start's own activation (the input's `required` would stop the form
  // before its submit), then started as if it had been picked
  $('go').addEventListener('click', (ev) => {
    if ($('iso').files.length || !rememberOn() || !remembered.disc) return; // the form's own way
    ev.preventDefault();
    startAudio(); // in the gesture itself, as the form's submit does
    // not full screen yet: a full screen asked for uses the click's activation
    // up, and the browser's ask for the disc needs it; the submit asks for
    // full screen once the disc is in, if the activation lasted
    takeAgain('iso').then((ok) => { if (ok) $('gate').requestSubmit(); });
  });
}

// ---- full screen (the user, 2026-10-27: "the browser build should force
// full screen, mobile players cant figure it out", and "still allow f11 to
// non fullscreen"). Start and View put the whole page full screen in their
// own click (the page, not the canvas: the notices, the report dialog and the
// phone keyboard's input stay on it), where it can stay so:
// - on a phone or a tablet (`eqoaTouchFirst`), in any browser that lets a
//   page. One left (the back gesture, another app) comes back at the next
//   touch on the game, but not while an edit box has the keyboard (that touch
//   puts it up);
// - on a computer only where Escape stays the game's (its windows, its
//   menus): a browser ends a page's full screen on Escape, but Chromium-based
//   ones let the page keep it (Keyboard Lock; held, it still ends it, as the
//   browser says on entering). Firefox's and Safari's full screen stays F11's
//   alone, as before. Not with another window of the game open in this
//   browser either: multiboxing, the windows side by side.
// F11 stays the browser's: in a window its own full screen, in any full
// screen (the page's too) back to a window (Chromium takes it before the
// page there). The canvas cancels every key it is given (winit's listener),
// so F11 is kept from it, caught on its way down, and left to the browser.
// iOS lets no page on an iPhone go full screen, and an iPad's Safari ends
// one at the first key typed ("Typing is not allowed in full screen
// websites"): there the app off the Home Screen is full screen (`installWhat`).

const full = { said: 'not asked' }; // what the page last asked or heard, for a report
// Escape can stay the game's in a page's full screen
const keepsEscape = !!(navigator.keyboard && typeof navigator.keyboard.lock === 'function');
// full screen already: the page's, the browser's own (F11), or an installed app's
const shownFull = () => !!document.fullscreenElement || matchMedia('(display-mode: fullscreen)').matches;
// a browser and a device where the page puts itself full screen
const canFull = () => !!document.fullscreenEnabled && !onIOS && (window.eqoaTouchFirst() || keepsEscape);
// another window of the game is open in this browser (the places multibox.rs reads)
const multiboxing = () => windows.live.some((n) => n !== windows.place);

function fullScreen() {
  if (!canFull() || shownFull()) return;
  if (!window.eqoaTouchFirst() && multiboxing()) {
    full.said = 'not asked: another window of the game is open';
    return;
  }
  full.said = 'asked';
  let asked;
  try {
    asked = document.documentElement.requestFullscreen({ navigationUI: 'hide' });
  } catch (e) {
    asked = Promise.reject(e);
  }
  Promise.resolve(asked).catch((e) => {
    full.said = `refused (${e && e.message ? e.message : e})`;
    console.log(`full screen ${full.said}`);
  });
}

document.addEventListener('fullscreenchange', () => {
  const on = !!document.fullscreenElement;
  full.said = on ? 'on' : 'left';
  if (!keepsEscape) return;
  if (!on) {
    navigator.keyboard.unlock();
    return;
  }
  navigator.keyboard.lock(['Escape']).then(
    () => { if (document.fullscreenElement) full.said = 'on, Escape the game’s'; },
    (e) => { full.said = `on, Escape not kept (${e && e.message ? e.message : e})`; });
});

for (const ev of ['keydown', 'keyup']) {
  addEventListener(ev, (e) => { if (e.key === 'F11') e.stopImmediatePropagation(); }, true);
}

$('eqoa').addEventListener('pointerup', (e) => {
  if (e.pointerType !== 'mouse' && started && !wantText && window.eqoaTouchFirst()) fullScreen();
});

// For a report.
function fullFacts() {
  const where = !document.fullscreenEnabled ? 'the browser lets no page'
    : onIOS ? 'none on iOS (the Home Screen app is)'
    : window.eqoaTouchFirst() ? 'from Start, and again at a touch'
    : keepsEscape ? 'from Start, Escape the game’s'
    : 'F11’s alone (Escape would end the page’s)';
  const now = document.fullscreenElement ? 'the page’s' : shownFull() ? 'the browser’s or the app’s' : 'a window';
  return `${where}; now ${now}; last: ${full.said}`;
}

// ---- installing the page as an app (the user, 2026-10-27: "we can add the
// install app feature for each"). The build's manifest (manifest.json: this
// page, full screen, the drawn disc's icons) lets a browser install the
// page: started from the Home Screen or the apps it is full screen, with
// nothing of the browser round it, and its storage is the app's (the
// modules unpacked off the disc stay: Safari clears a site's after a week
// unused, not a Home Screen app's). It draws no faster: the engine is the
// browser's. Chromium-based browsers offer the install themselves once the
// page has been used a little (`beforeinstallprompt`): the card's button asks
// for it. The others keep it in a menu, and the card says which: iOS's Share
// menu (an iPhone's only full screen), Firefox's on Android, any phone
// browser's, Safari's File menu on a Mac. Started as the app, the card says
// nothing of it.

const install = { prompt: null, said: '' };
// started as an installed app (judged as the page opens: a browser's F11 full
// screen, entered later, is the same display mode)
const asApp = navigator.standalone === true || matchMedia('(display-mode: fullscreen), (display-mode: standalone), (display-mode: minimal-ui)').matches;

function installWhat() {
  const b = browser();
  let text = '';
  if (install.said === 'accepted' || install.said === 'installed') {
    text = 'Installed: start EQOA from your Home Screen or your apps from now on.';
  } else if (install.prompt) {
    text = 'Then it starts from your Home Screen or your apps, full screen, with nothing of the browser round it.';
  } else if (onIOS) {
    text = 'For full screen on an iPhone or iPad, install it: in the browser’s Share menu choose Add to Home Screen, then start EQOA from your Home Screen.';
  } else if (window.eqoaTouchFirst()) {
    text = b.firefox
      ? 'To install it as an app (full screen, from your Home Screen), choose Install in Firefox’s menu.'
      : 'To install it as an app (full screen, from your Home Screen), choose Install app or Add to Home screen in the browser’s menu.';
  } else if (b.safari) {
    text = 'To install it as an app, choose File > Add to Dock in Safari.';
  }
  $('install-go').hidden = !install.prompt;
  $('install-what').textContent = text;
  $('install').hidden = asApp || !text;
}

addEventListener('beforeinstallprompt', (e) => {
  install.prompt = e;
  if (!install.said) install.said = 'offered';
  installWhat();
});
addEventListener('appinstalled', () => {
  install.prompt = null;
  install.said = 'installed';
  installWhat();
});
$('install-go').addEventListener('click', async () => {
  const p = install.prompt;
  if (!p) return;
  install.prompt = null;
  try {
    await p.prompt();
    install.said = (await p.userChoice).outcome; // 'accepted' or 'dismissed'
  } catch (e) {
    install.said = `failed (${e && e.message ? e.message : e})`;
  }
  installWhat();
});
installWhat();

// ---- updates: this page is one build (web/build.sh puts each in
// v/<build>/, kept as it is for good), and version.json beside the page names
// the newest. A tab left open asks now and then: on an untouched gate
// it reloads at once, nothing being lost; anywhere else it says so, and the
// player reloads when it suits them: Later holds for half an hour (as the
// native game's question, update.rs's LATER), or until a newer build. The
// notice says what is new in it: the sections of the new build's update
// notes (build.sh's update-notes.md) this page's own lack, as the game's
// question does (update.rs's whats_new). With Update Notices off (the
// Extended page, settings::update_notices) it says nothing: the newest build
// is the page the next time it loads, as the native game's starts unasked.

const BUILD = (import.meta.url.match(/\/v\/([0-9a-f]+)\/main\.js(?:$|[?#])/) || [])[1] || null;
window.eqoaBuild = BUILD;
let latest = null;     // the newest build version.json named, when last asked
let clientReady = false; // the client's exports can be called (init done)
let asked = 0;
let later = null;      // the build the player said "Later" to, and until when
// how long a Later holds (`?later=<ms>` for tests)
const LATER_MS = Number(new URLSearchParams(location.search).get('later')) || 30 * 60 * 1000;

// An UPDATE-NOTES.md's sections, newest first: `## ` a title, `- ` a bullet
// and an indented line carrying it on, any other line a paragraph; markdown
// emphasis and code marks out.
function noteSections(md) {
  const out = [];
  let open = false;
  const plain = (t) => t.replace(/\*\*/g, '').replace(/`/g, '');
  for (const raw of md.split('\n')) {
    const t = raw.trimEnd();
    if (t.startsWith('## ')) {
      out.push({ title: plain(t.slice(3).trim()), bullets: [] });
      open = false;
      continue;
    }
    const s = out[out.length - 1];
    if (!s) continue;
    if (!t.trim()) open = false;
    else if (t.startsWith('- ')) { s.bullets.push(plain(t.slice(2).trim())); open = true; }
    else if (open && t.startsWith(' ') && s.bullets.length) s.bullets[s.bullets.length - 1] += ' ' + plain(t.trim());
    else { s.bullets.push(plain(t.trim())); open = true; }
  }
  return out;
}

// What is new in build `b` for this page: its notes' sections this build's
// notes lack (at most five, newest first), as text; '' when there are none
// or they could not be read.
async function whatsNew(b) {
  try {
    const get = async (u) => { const r = await fetch(u); if (!r.ok) throw new Error(String(r.status)); return r.text(); };
    const [theirs, mine] = await Promise.all([get(new URL(`../${b}/update-notes.md`, import.meta.url)), get(new URL('./update-notes.md', import.meta.url))]);
    const had = new Set(noteSections(mine).map((s) => s.title));
    const news = noteSections(theirs).filter((s) => !had.has(s.title)).slice(0, 5);
    return news.map((s) => `${s.title}\n${s.bullets.map((x) => `• ${x}`).join('\n')}`).join('\n\n');
  } catch {
    return '';
  }
}

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
  const untouched = !started && !$('iso').files.length && !$('iso2002').files.length;
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
  if (!noticesOn()) {
    report.notice('update');
    return latest;
  }
  if (!later || later.build !== latest || Date.now() >= later.until) {
    const news = await whatsNew(latest);
    report.notice('update', '', (started
      ? 'A new version of the client is out. Reloading leaves the game, and you sign in again.'
      : 'A new version of the client is out.') + (news ? `\n\nWhat's new:\n${news}` : ''), [
      ['Reload', () => { session.set('reloadedFor', latest); location.reload(); }, true],
      ['Later', () => { later = { build: latest, until: Date.now() + LATER_MS }; report.notice('update'); }],
    ]);
  }
  return latest;
}
// Update Notices, as the client keeps them (on until the client is loaded,
// or if it cannot say).
function noticesOn() {
  if (!clientReady) return true;
  try { return update_notices(); } catch { return true; }
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
    `2002 disc: ${disc2002Line || ($('iso2002').files.length ? 'picked, not identified' : 'none picked')}`,
    `remember: ${rememberFacts()}`,
    `full screen: ${fullFacts()}`,
    `app: ${asApp ? 'started as the installed app' : install.said || (install.prompt ? 'offered' : 'not offered')}`,
    `panel: ${$('gate').hidden ? 'none (the game)' : 'gate'}, status: ${$('status').textContent}`,
    `relay ${$('relay').value || '-'}, server ${$('server').value || '(this host)'}`,
    `sound: ${JSON.stringify(window.eqoaAudio.stats())}`,
  ];
}
$('report-gate').onclick = () => {
  const e = $('gate-error').textContent.trim();
  if (report) report.open(e ? `The page said: ${e}` : '');
};

// ---- a hidden page: no animation frames come, and the browser holds the
// page's timers back, so the game would stop and the server let its session
// go (F3 20261025-175001). A worker's timer (tick.js) is not held back: while
// the page is hidden it ticks at the game's 30 frames a second, and each tick
// runs a frame of the game with nothing drawn (web::hidden_tick), its sound
// and its network going on. A frozen page (the browser's own choice, on low
// memory or a laptop asleep) runs nothing at all.

window.eqoaHiddenTicks = 0;
function hiddenTicker() {
  let ticker;
  try {
    ticker = new Worker(new URL('tick.js', import.meta.url));
  } catch (e) {
    console.log(`no ticker for a hidden page (${e}): the game stops while the page is hidden`);
    return;
  }
  ticker.onmessage = () => {
    window.eqoaHiddenTicks++; // for web/pagetest.py
    try { hidden_tick(); } catch { /* the client stopped: its crash report says so */ }
  };
  const tell = () => ticker.postMessage(document.hidden);
  document.addEventListener('visibilitychange', tell);
  tell();
}

checkGraphics();
const ready = init({ module_or_path: wasmModule });
ready.then(() => {
  clientReady = true;
  window.eqoaClient = { state: client_state, wantShot: want_shot, crashTest: crash_test };
  hiddenTicker();
  status('ready');
}).catch((e) => {
  status('could not load the client: ' + e);
  problem('load', `The client did not load (${e}). Reload the page; if it keeps happening, send a report.`);
});
