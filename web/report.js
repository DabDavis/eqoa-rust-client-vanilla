// Problem reports and crash reports from the page, as the Go client has them
// (Dab's Mod's F3 and its crash reports). F3, the HUD's "Report a bug" or a
// card's "Report a problem" opens a dialog that sends a note, a picture of
// the frame, the client's state and the last lines of the console to the
// report server's /report (go-eqoa-client-v1's cmd/reportd; its public board,
// /eqoa/reports/board, shows the note and the picture and never the rest).
// A crash (a panic, a trap, the graphics device lost) opens one that sends to
// /crash, which nothing serves back; so does a tab that died while it was in
// the world, on the next visit. What goes wrong without stopping the client
// (a file of the build that would not load, a module that would not unpack,
// a page older than the server's) is a notice with a Send report button.
//
// Nothing is sent unless the player presses Send, and the account name and
// the password are taken out of everything that is.
//
// This file is a module of its own, loaded before main.js and importing
// nothing, so that it runs when the client's files do not: the page's errors
// are its to catch. The client's exports reach it through window.eqoaClient
// (main.js, once the wasm is up).

const params = new URLSearchParams(location.search);
const REPORTS = (params.get('reports') || '/eqoa/reports').replace(/\/+$/, '');
const LOG_LINES = 200;
const LOG_LINE_MAX = 400;
const NOTE_MAX = 1000;
const NAME_MAX = 32;
const SHOT_WIDTH = 1280;
// the server keeps 512 KiB of a report and 256 KiB of a crash
const REPORT_MAX = 500 * 1024;
const CRASH_MAX = 240 * 1024;
// a tab that died in the world, offered on the next visit for this long
const MARK = 'eqoa.session';
const MARK_AGE = 3 * 24 * 3600 * 1000;

const $ = (id) => document.getElementById(id);
const opened = Date.now();

function el(tag, props, ...kids) {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
}

const store = {
  get(k) { try { return localStorage.getItem(k) || ''; } catch { return ''; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* no storage: nothing kept */ } },
  drop(k) { try { localStorage.removeItem(k); } catch { /* as above */ } },
};

// ---- this build (web/build.sh writes build.json beside this file)

let build = { build: 'dev', commit: '' };
const buildKnown = fetch(new URL('build.json', import.meta.url))
  .then((r) => (r.ok ? r.json() : null))
  .then((b) => { if (b && b.build) build = b; })
  .catch(() => {});

function version() {
  return 'web-' + (build.commit || build.build);
}

// The board shows this as the report's OS: the system, the browser and how
// it draws, in 64 characters.
function platform() {
  const ua = navigator.userAgent;
  const os = /Windows/.test(ua) ? 'Windows' : /Android/.test(ua) ? 'Android' : /iPhone|iPad|iPod/.test(ua) ? 'iOS'
    : /CrOS/.test(ua) ? 'ChromeOS' : /Mac OS X/.test(ua) ? 'macOS' : /Linux/.test(ua) ? 'Linux' : 'other';
  const b = ua.match(/(Edg|OPR|Firefox|Chrome|Version)\/(\d+)/);
  const name = !b ? 'browser' : { Edg: 'Edge', OPR: 'Opera', Version: 'Safari' }[b[1]] || b[1];
  return `${os} ${name}${b ? ' ' + b[2] : ''}${api.graphics ? ' ' + api.graphics() : ''}`.slice(0, 64);
}

// ---- the console's last lines: every report carries them, the only copy a
// player has

const ring = [];

function line(level, args) {
  const text = args.map((a) => {
    if (typeof a === 'string') return a;
    if (a instanceof Error) return a.stack || String(a);
    try { return JSON.stringify(a); } catch { return String(a); }
  }).join(' ');
  const t = ((Date.now() - opened) / 1000).toFixed(1);
  ring.push(`[${t}] ${level === 'log' || level === 'info' ? '' : level + ': '}${text.slice(0, LOG_LINE_MAX)}`);
  if (ring.length > LOG_LINES) ring.shift();
}

for (const level of ['log', 'info', 'warn', 'error']) {
  const to = console[level].bind(console);
  console[level] = (...args) => {
    try { line(level, args); } catch { /* never in the way of the console */ }
    to(...args);
  };
}

// ---- what a report says of the page and of the client

const problems = [];
let dead = false;    // the client stopped: its exports are not called again
let running = false; // in the world, or the viewer: there is a frame to picture

function scrub(text) {
  const secrets = [];
  const user = ($('user') && $('user').value.trim()) || '';
  for (const id of ['pass', 'pass2']) {
    const v = $(id) && $(id).value;
    // under four characters a password is as likely to be part of a word
    if (v && v.length >= 4) secrets.push([v, '<password>']);
  }
  for (const [s, to] of secrets) text = text.split(s).join(to);
  // as a word, case kept: a character named after the account keeps its name
  if (user) text = text.replace(new RegExp(`\\b${user.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`, 'g'), '<account>');
  return text;
}

function clientState() {
  if (dead || !window.eqoaClient) return '';
  try { return window.eqoaClient.state(); } catch (e) { return `(the client's state could not be read: ${e})`; }
}

function dump() {
  const out = [];
  const f = (s) => out.push(s);
  f(`eqoa web client trace ${new Date().toISOString()}`);
  f(`build: ${build.build}, commit ${build.commit || '-'} (${version()})`);
  f(`page open ${((Date.now() - opened) / 1000).toFixed(0)} s: ${location.origin}${location.pathname}${location.search}`);
  f(`browser: ${navigator.userAgent}`);
  const c = $('eqoa');
  f(`window ${innerWidth}x${innerHeight}, device pixel ratio ${devicePixelRatio}, canvas ${c ? c.width + 'x' + c.height : '-'}`);
  try {
    for (const s of (api.facts ? api.facts() : [])) f(s);
  } catch (e) {
    f(`(the page's facts failed: ${e})`);
  }
  if (problems.length) {
    f('');
    f('[problems]');
    for (const p of problems) f(`${p.at} s ${p.key}: ${p.text}`);
  }
  const state = clientState();
  if (state) {
    f('');
    f('[client]');
    f(state.trimEnd());
  }
  f('');
  f(`[log] last ${ring.length} lines`);
  for (const l of ring) f(l);
  return out.join('\n') + '\n';
}

function fit(text, max) {
  return text.length <= max ? text : text.slice(0, max - 40) + '\n(cut to fit the report server)\n';
}

async function post(route, body) {
  let r;
  try {
    r = await fetch(`${REPORTS}/${route}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  } catch {
    throw new Error('the report server could not be reached');
  }
  let j = {};
  try { j = await r.json(); } catch { /* not JSON: the status says it */ }
  if (!r.ok || !j.ok) throw new Error(j.error || `the report server answered ${r.status}`);
  return j.id;
}

// ---- notices: a line at the top of the page, over the game too; one per
// key, replaced when the same key comes again, gone when its text is empty

function notice(key, kind, text, actions = []) {
  const box = $('notices');
  if (!box) return;
  let n = [...box.children].find((c) => c.dataset.key === key);
  if (!text) {
    if (n) n.remove();
    return;
  }
  if (!n) {
    n = el('div', { className: 'notice', role: 'status' });
    n.dataset.key = key;
    box.append(n);
  }
  n.className = 'notice ' + (kind || '');
  n.replaceChildren(el('div', { className: 'text', textContent: text }), ...actions.map(([label, act, primary]) => {
    const b = el('button', { type: 'button', textContent: label, className: primary ? 'primary' : '' });
    b.onclick = () => {
      act();
      // back to the game, which takes keys only while its canvas has focus
      const d = $('report');
      if (running && (!d || d.hidden)) $('eqoa').focus();
    };
    return b;
  }));
}

// Something went wrong short of a crash: on the page with a way to report
// it, and in every report from now on.
function problem(key, text) {
  if (!problems.some((p) => p.key === key && p.text === text)) {
    problems.push({ key, text, at: ((Date.now() - opened) / 1000).toFixed(1) });
  }
  console.warn(`problem (${key}): ${text}`);
  notice('problem:' + key, 'bad', text, [
    ['Send report', () => open(text), true],
    ['Dismiss', () => notice('problem:' + key)],
  ]);
}

// ---- the dialog: a problem report (F3) or a crash report

const DIALOG = `
<form class="card wide" id="rp-form" novalidate>
  <h2 id="rp-title"></h2>
  <p class="lede" id="rp-intro"></p>
  <pre class="crash" id="rp-crash" hidden></pre>
  <label for="rp-note">What happened</label>
  <textarea id="rp-note" rows="4" maxlength="${NOTE_MAX}"></textarea>
  <div id="rp-name-row">
    <label for="rp-name">Your name on the board, to credit you (optional)</label>
    <input id="rp-name" maxlength="${NAME_MAX}" autocomplete="off">
  </div>
  <label class="check" id="rp-shot-row"><input type="checkbox" id="rp-shot" checked> A picture of the screen</label>
  <img id="rp-pic" alt="the picture that goes with the report" hidden>
  <div class="error" id="rp-error"></div>
  <div class="ok" id="rp-done"></div>
  <div class="row">
    <button type="button" id="rp-close">Close</button>
    <button type="submit" id="rp-send" class="primary"></button>
  </div>
  <p class="note" id="rp-fine"></p>
</form>`;

let kind = null;      // 'problem' or 'crash'
let crashText = '';   // a crash's report, made when it happened
let shot = null;      // a problem's picture (a data: URL), or null

function dialog() {
  let d = $('report');
  if (d) return d;
  d = el('div', { id: 'report', className: 'modal', hidden: true });
  d.innerHTML = DIALOG;
  document.body.append(d);
  $('rp-close').onclick = close;
  $('rp-form').addEventListener('submit', (ev) => { ev.preventDefault(); send(); });
  $('rp-shot').onchange = () => { $('rp-pic').hidden = !$('rp-shot').checked || !shot; };
  d.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') { e.preventDefault(); close(); }
    // the game under the dialog sees none of what is typed in it
    e.stopPropagation();
  });
  return d;
}

function close() {
  const d = $('report');
  if (!d) return;
  d.hidden = true;
  kind = null;
  if (running && !dead) $('eqoa').focus();
}

// A picture of the next frame the client draws: the client says when it is
// drawn (`eqoaFrameDrawn`, web::frame_drawn), while the canvas still holds it.
function picture() {
  return new Promise((resolve) => {
    const client = window.eqoaClient;
    if (!running || dead || !client) return resolve(null);
    const late = setTimeout(() => { window.eqoaFrameDrawn = null; resolve(null); }, 1000);
    window.eqoaFrameDrawn = () => {
      clearTimeout(late);
      window.eqoaFrameDrawn = null;
      try {
        const src = $('eqoa');
        const k = Math.min(1, SHOT_WIDTH / src.width);
        const out = el('canvas', { width: Math.round(src.width * k), height: Math.round(src.height * k) });
        out.getContext('2d').drawImage(src, 0, 0, out.width, out.height);
        out.toBlob((b) => {
          if (!b) return resolve(null);
          const r = new FileReader();
          r.onload = () => resolve(r.result);
          r.onerror = () => resolve(null);
          r.readAsDataURL(b);
        }, 'image/png');
      } catch (e) {
        console.warn('no picture: ' + e);
        resolve(null);
      }
    };
    try { client.wantShot(); } catch { clearTimeout(late); resolve(null); }
  });
}

function show(k, title, intro, sendLabel) {
  const d = dialog();
  kind = k;
  $('rp-title').textContent = title;
  $('rp-intro').textContent = intro;
  $('rp-send').textContent = sendLabel;
  $('rp-send').disabled = false;
  $('rp-send').onclick = null;
  $('rp-crash').hidden = k !== 'crash';
  $('rp-error').textContent = '';
  $('rp-done').textContent = '';
  $('rp-name-row').hidden = k !== 'problem';
  $('rp-shot-row').hidden = k !== 'problem' || !shot;
  $('rp-pic').hidden = k !== 'problem' || !shot || !$('rp-shot').checked;
  $('rp-fine').textContent = k === 'problem'
    ? 'It goes to the developer. The note and the picture appear on the public board (eqoaclassic.com/eqoa/reports/board); the client’s state and its log do not. Your account name and password are never sent.'
    : 'It goes to the developer and is not shown anywhere. Your account name and password are never sent.';
  d.hidden = false;
  $('rp-note').focus();
}

// F3, the HUD's button, a card's, or a notice's: `note` starts the note.
// After a crash it is the crash's dialog again.
async function open(note = '') {
  if (dead) {
    if ($('report')) $('report').hidden = false;
    return;
  }
  if (kind) return; // up already
  kind = 'problem';
  shot = await picture();
  $('rp-note') || dialog();
  $('rp-note').value = note ? note.slice(0, NOTE_MAX) : '';
  $('rp-name').value = store.get('eqoa.report_name');
  if (shot) $('rp-pic').src = shot;
  show('problem', 'Report a problem', 'What went wrong, and what were you doing? A picture of the game and what the client knows go with it.', 'Send');
}

// The client stopped: `title`, and `detail` (the panic and the state the
// client noted, web::crash; or the error that trapped it).
function crash(title, detail) {
  if (dead) return; // the panic's own report is up; its trap follows it
  dead = true;
  running = false;
  console.error(`${title}: ${detail.split('\n')[0]}`);
  // a trap without a panic carries no state; the last the page kept is close
  let kept = '';
  if (!/\n\n\S/.test(detail)) {
    try { kept = JSON.parse(store.get(MARK) || 'null').state || ''; } catch { /* none kept */ }
  }
  crashText = `${title}\n${detail.trimEnd()}${kept ? `\n\n[client, as last kept]\n${kept.trimEnd()}` : ''}\n\n${dump()}`;
  store.drop(MARK);
  dialog();
  $('rp-crash').hidden = false;
  $('rp-crash').textContent = detail.split('\n').slice(0, 12).join('\n');
  $('rp-note').value = '';
  show('crash', title, 'The client cannot go on: reload the page to play again. Sending the crash report helps it get fixed.', 'Send crash report');
}

async function send() {
  const note = $('rp-note').value.trim().slice(0, NOTE_MAX);
  $('rp-send').disabled = true;
  $('rp-error').textContent = '';
  $('rp-done').textContent = 'Sending…';
  await buildKnown;
  try {
    let id;
    if (kind === 'crash') {
      const report = fit(scrub(`EQOA web client crash report\nversion: ${version()}\nchannel: web\nnote: ${note.replace(/\n/g, ' ') || '-'}\n\n--- crash ---\n${crashText}`), CRASH_MAX);
      id = await post('crash', { report, note, version: version(), platform: platform(), channel: 'web' });
      $('rp-done').textContent = `Sent (${id}). Thank you. Reload the page to play again.`;
      $('rp-send').textContent = 'Reload';
      $('rp-send').disabled = false;
      $('rp-send').onclick = (e) => { e.preventDefault(); location.reload(); };
      return;
    }
    const name = $('rp-name').value.replace(/[\r\n\t]/g, ' ').trim().slice(0, NAME_MAX);
    store.set('eqoa.report_name', name);
    const withShot = shot && $('rp-shot').checked;
    const report = fit(scrub(`EQOA web client problem report\nversion: ${version()}\nchannel: web\nname: ${name || '-'}\nnote: ${note.replace(/\n/g, ' ') || '-'}\n\n--- trace ---\n${dump()}`), REPORT_MAX);
    id = await post('report', {
      report, note, name, version: version(), platform: platform(), channel: 'web',
      screenshot: withShot ? shot.slice(shot.indexOf(',') + 1) : '',
    });
    $('rp-done').replaceChildren(`Sent as ${id}. Thank you. `,
      el('a', { href: `${REPORTS}/board?id=${encodeURIComponent(id)}`, target: '_blank', rel: 'noopener', textContent: 'See it on the board' }));
    $('rp-send').disabled = false;
    $('rp-send').textContent = 'Send another';
  } catch (e) {
    $('rp-done').textContent = '';
    $('rp-error').textContent = `Not sent: ${e.message || e}. Try again in a moment.`;
    $('rp-send').disabled = false;
  }
}

// ---- what the page catches

// F3 is the report, as in the Go client and Dab's Mod; caught on its way
// down, before the canvas (which takes every key) or the browser's own find
// sees it.
for (const ev of ['keydown', 'keyup']) {
  addEventListener(ev, (e) => {
    if (e.key !== 'F3') return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (ev === 'keydown' && !e.repeat && !kind) open();
  }, true);
}

function errorText(e) {
  return e && (e.stack || e.message) ? String(e.stack || e.message) : String(e);
}

function caught(err, where) {
  // a trap is the client stopping where no panic was raised (out of memory,
  // a stack overflow); after a panic it is that panic's, already reported
  if (typeof WebAssembly !== 'undefined' && err instanceof WebAssembly.RuntimeError) {
    crash('The client stopped', errorText(err));
    return;
  }
  const text = err && err.message ? err.message : String(err);
  // the browser's own, harmless
  if (/ResizeObserver loop/.test(text)) return;
  problem('page', `Something in the page failed (${where}): ${text}`);
}

addEventListener('error', (e) => {
  const t = e.target;
  if (t && t !== window) {
    // one of the page's own files did not load: nothing after it runs
    if (t.tagName === 'SCRIPT' || t.tagName === 'LINK') {
      problem('load', `A file of the page did not load (${t.src || t.href}). Reload the page; if it happens again, the site may be in the middle of an update: wait a minute.`);
    }
    return;
  }
  // an extension's errors are not the page's
  if (e.filename && !e.filename.startsWith(location.origin)) return;
  line('error', [e.error || e.message]);
  caught(e.error || e.message, 'an event');
}, true);

addEventListener('unhandledrejection', (e) => {
  line('error', ['unhandled: ', e.reason]);
  caught(e.reason, 'a promise');
});

// ---- a tab that died in the world: while the game runs and is shown, the
// page keeps a mark with its state and its last lines; a page that closes
// takes it away (pagehide), one hidden marks it so (a background tab the
// browser discards is not a crash). A mark left by a tab that was shown is
// a tab or a browser that died: the next visit offers it.

function mark() {
  if (!running || dead) return;
  store.set(MARK, JSON.stringify({
    when: Date.now(), build: build.build, version: version(), hidden: document.visibilityState !== 'visible',
    state: clientState(), log: ring.slice(-60),
  }));
}

setInterval(mark, 10000);
document.addEventListener('visibilitychange', mark);
addEventListener('pagehide', () => store.drop(MARK));

function lastSession() {
  let m;
  try { m = JSON.parse(store.get(MARK) || 'null'); } catch { m = null; }
  if (!m || m.hidden || !(Date.now() - m.when < MARK_AGE)) {
    store.drop(MARK);
    return;
  }
  const when = new Date(m.when).toLocaleString();
  notice('last-session', 'bad', `The last session (${when}) ended without the page closing: the tab or the browser may have crashed.`, [
    ['Send crash report', async () => {
      notice('last-session', 'bad', 'Sending…');
      try {
        const text = `The page closed without closing (the tab or the browser died) at ${new Date(m.when).toISOString()}, in build ${m.build} (${m.version}).\n\n[client]\n${m.state || '-'}\n\n[log] last ${m.log.length} lines\n${m.log.join('\n')}\n\n[this page]\n${dump()}`;
        const report = fit(scrub(`EQOA web client crash report\nversion: ${m.version}\nchannel: web\n\n--- crash ---\n${text}`), CRASH_MAX);
        const id = await post('crash', { report, note: 'a tab that died', version: m.version, platform: platform(), channel: 'web' });
        store.drop(MARK);
        notice('last-session', '', `Sent (${id}). Thank you.`, [['Close', () => notice('last-session')]]);
      } catch (e) {
        notice('last-session', 'bad', `Not sent: ${e.message || e}.`, [['Close', () => notice('last-session')]]);
      }
    }, true],
    ['Not now', () => notice('last-session')],
    ['Delete', () => { store.drop(MARK); notice('last-session'); }],
  ]);
}

// ---- for main.js and the client

const api = {
  notice,
  problem,
  open,
  crash,
  // the game runs (in the world or the viewer): there is a frame to picture
  running(on) { running = on && !dead; mark(); },
  // lines main.js adds to every report (set by main.js)
  facts: null,
  // how the page draws, for the board's OS column (set by main.js)
  graphics: null,
  get dead() { return dead; },
};
window.eqoaReport = api;
window.eqoaProblem = ([key, text]) => problem(key, text);
window.eqoaCrash = ([title, detail]) => crash(title, detail);
window.eqoaReportOpen = () => open();

if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', lastSession);
else lastSession();
