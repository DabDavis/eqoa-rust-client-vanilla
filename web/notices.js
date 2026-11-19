// The page's notices: a line at the top of the page, over the game too, one
// per key, replaced when the same key comes again, gone when its text is
// empty (main.js's update, graphics and disc notices). What goes wrong is one
// too: short of a crash (a file of the build that would not load, a module
// that would not unpack, an error the page caught) a notice to dismiss; the
// client stopping (a panic, a trap, the graphics device lost: web::crash) one
// to reload the page by.
//
// This file is a module of its own, loaded before main.js and importing
// nothing, so that it runs when the client's files do not: the page's errors
// are its to catch.
//
// Crash reports (the server owner, 2026-11-19: "add crash reports back so we
// can find what the issue is", a player whose page stopped before it ever
// logged in): a crash, and a problem that leaves the page unable to start, is
// sent at once to eqoaclassic's report server (`REPORTS`/crash, the board's
// crashes/), with the console's last lines (`ring`), the browser and its
// graphics and the build. Never an account name or a password: the client
// tells the page each one it sends Station (window.eqoaSecret, web::secret)
// and every report has them taken out (`scrub`). The kept report of the
// client's own panic (localStorage eqoa.crashReport, crashfile.rs) rides
// along; it is still sent to the game server at the next login too.

const $ = (id) => document.getElementById(id);

function el(tag, props, ...kids) {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
}

let dead = false;    // the client stopped: its exports are not called again
let leavingAt = -Infinity; // the page's last beforeunload (performance.now())
addEventListener('beforeunload', () => { leavingAt = performance.now(); });
let running = false; // in the world, or the viewer: the canvas has the keys

// ---- crash reports

// ?reports=<url> sends them to another report server (a test's, never the
// live board's): only crash reports go there
const REPORTS = (() => {
  try { return new URLSearchParams(location.search).get('reports') || 'https://eqoaclassic.com/eqoa/reports'; } catch { return 'https://eqoaclassic.com/eqoa/reports'; }
})();
const CRASH_MAX = 250 * 1024; // the report server takes 256 KiB of text
const LOG_LINES = 200;
const LOG_LINE_MAX = 500;
const opened = Date.now();

// the console's last lines: a report carries them, the only copy a player has
const ring = [];
for (const level of ['log', 'info', 'warn', 'error']) {
  const to = console[level].bind(console);
  console[level] = (...args) => {
    try {
      const text = args.map((a) => {
        if (typeof a === 'string') return a;
        if (a instanceof Error) return a.stack || String(a);
        try { return JSON.stringify(a); } catch { return String(a); }
      }).join(' ');
      const t = ((Date.now() - opened) / 1000).toFixed(1);
      ring.push(`[${t}] ${level === 'log' || level === 'info' ? '' : level + ': '}${text.slice(0, LOG_LINE_MAX)}`);
      if (ring.length > LOG_LINES) ring.shift();
    } catch { /* never in the way of the console */ }
    to(...args);
  };
}

// the account names and passwords the client sent Station (web::secret)
const accounts = new Set();
const passwords = new Set();
window.eqoaSecret = (account, password) => {
  if (account && account.trim()) accounts.add(account.trim());
  // under four characters a password is as likely to be part of a word
  if (password && password.length >= 4) passwords.add(password);
};

function scrub(text) {
  for (const p of passwords) text = text.split(p).join('<password>');
  // as a word, case kept: a character named after the account keeps its name
  for (const user of accounts) text = text.replace(new RegExp(`\\b${user.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`, 'g'), '<account>');
  return text;
}

let build = { build: window.eqoaBuild || 'dev', commit: '' };
const buildKnown = fetch(new URL('build.json', import.meta.url))
  .then((r) => (r.ok ? r.json() : null))
  .then((b) => { if (b && b.build) build = b; })
  .catch(() => {});

function version() {
  return 'web-' + (build.commit || build.build || window.eqoaBuild || 'dev');
}

function graphics() {
  const g = String(window.eqoaClientGraphics || '');
  if (/\(Gl\)$/.test(g)) return 'WebGL2';
  if (g) return 'WebGPU';
  return navigator.gpu ? 'WebGPU?' : 'WebGL2?';
}

// the board's OS column: the system, the browser and how it draws, in 64
function platform() {
  const ua = navigator.userAgent;
  const os = /Windows/.test(ua) ? 'Windows' : /Android/.test(ua) ? 'Android' : /iPhone|iPad|iPod/.test(ua) ? 'iOS'
    : /CrOS/.test(ua) ? 'ChromeOS' : /Mac OS X/.test(ua) ? 'macOS' : /Linux/.test(ua) ? 'Linux' : 'other';
  const b = ua.match(/(Edg|OPR|Firefox|Chrome|Version)\/(\d+)/);
  const name = !b ? 'browser' : { Edg: 'Edge', OPR: 'Opera', Version: 'Safari' }[b[1]] || b[1];
  return `${os} ${name}${b ? ' ' + b[2] : ''} ${graphics()}`.slice(0, 64);
}

function pageFacts() {
  const c = $('eqoa');
  let kept = '';
  try { kept = localStorage.getItem('eqoa.crashReport') || ''; } catch { /* a private window */ }
  return [
    `version: ${version()}`,
    `build: ${build.build}, commit ${build.commit || '-'}`,
    `page open ${((Date.now() - opened) / 1000).toFixed(0)} s: ${location.origin}${location.pathname}`,
    `browser: ${navigator.userAgent}`,
    `graphics: ${window.eqoaClientGraphics || '-'} (navigator.gpu ${navigator.gpu ? 'yes' : 'no'})`,
    `window ${innerWidth}x${innerHeight}, device pixel ratio ${devicePixelRatio}, canvas ${c ? c.width + 'x' + c.height : '-'}`,
    `memory: ${navigator.deviceMemory ? navigator.deviceMemory + ' GB (device)' : '-'}${performance.memory ? `, JS heap ${(performance.memory.usedJSHeapSize / 1048576).toFixed(0)} MB` : ''}`,
    `in the world: ${running ? 'yes' : 'no'}`,
    ...(kept ? ['', '[the client\'s own report of its panic]', kept.trimEnd()] : []),
  ];
}

// at most one report of each kind a page: a crash, and each problem's key
const sent = new Set();

// a crash (or a problem that stops the page) sent to the report server;
// `done` hears its id, or why it was not sent
async function sendCrash(kind, title, detail, done) {
  if (sent.has(kind)) return;
  sent.add(kind);
  await Promise.race([buildKnown, new Promise((r) => setTimeout(r, 2000))]);
  let text = `EQOA web client crash report\n${title}\n${String(detail).trimEnd()}\n\n${pageFacts().join('\n')}\n\n[log] last ${ring.length} lines\n${ring.join('\n')}\n`;
  text = scrub(text);
  if (text.length > CRASH_MAX) text = text.slice(0, CRASH_MAX - 40) + '\n(cut to fit the report server)\n';
  try {
    const r = await fetch(`${REPORTS}/crash`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, keepalive: text.length < 60000,
      body: JSON.stringify({ report: text, note: kind === 'crash' ? '' : kind, version: version(), platform: platform(), channel: 'web' }),
    });
    let j = {};
    try { j = await r.json(); } catch { /* not JSON */ }
    if (r.ok && j.ok) {
      console.log(`crash report sent: ${j.id}`);
      if (done) done(j.id, '');
    } else if (done) done('', j.error || `the report server answered ${r.status}`);
  } catch (e) {
    if (done) done('', 'the report server could not be reached');
  }
}

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
      if (running) $('eqoa').focus();
    };
    return b;
  }));
}

// Something went wrong short of a crash: said on the page.
function problem(key, text) {
  console.warn(`problem (${key}): ${text}`);
  notice('problem:' + key, 'bad', text, [['Dismiss', () => notice('problem:' + key)]]);
  // a problem that stops the page before the game runs (a file that did not
  // load, the client that would not start) is a crash report too: nothing
  // else of it would ever reach anyone
  if (!running && !leaving()) sendCrash('problem:' + key, `A problem before the game ran (${key})`, text);
}

// The client stopped: `title`, and `detail` (the panic, web::crash; or the
// error that trapped it). Its trap after a panic says nothing more.
function crash(title, detail) {
  if (dead) return;
  dead = true;
  running = false;
  console.error(`${title}: ${detail}`);
  // kept for the next session's bug report (the client's bugnote.rs reads
  // `eqoa.lastCrash`, says it once in the note and lets it go): the page sends
  // no crash reports, so a crash a player only describes (petition 1355: "I hold
  // down keys", nothing else) comes with its message the next time they press F3
  try {
    // a panic's first line is its place ("panicked at crates/...:12:34:"), its
    // second what went wrong: both
    const lines = String(detail).split('\n').map((l) => l.trim()).filter(Boolean).slice(0, 2).join(' ');
    localStorage.setItem('eqoa.lastCrash', `${new Date().toISOString().slice(0, 19)}Z, page ${window.eqoaBuild || 'unbuilt'}: ${title}: ${lines}`.slice(0, 600));
  } catch { /* a private window: nothing kept */ }
  const head = `${title}: ${String(detail).split('\n')[0]}\n\nThe client cannot go on: reload the page to play again.`;
  const say = (more) => notice('crash', 'bad', head + more, [['Reload', () => location.reload(), true]]);
  say('\nSending a crash report...');
  sendCrash('crash', title, detail, (id, why) => say(id ? `\nA crash report was sent (${id}): thank you.` : `\nThe crash report could not be sent (${why}).`));
}

// ---- what the page catches

// what went wrong and where: Safari's and Firefox's stacks leave out the
// error's own words (petition 2097 kept only "Uint8Array@[native code]
// __wbg_new_with_byte_offset_and_length…", not its RangeError or TypeError),
// so they go first wherever the stack does not start with them
function errorText(e) {
  if (!e || !(e.stack || e.message)) return String(e);
  const head = e.name && e.message ? `${e.name}: ${e.message}` : String(e.message || '');
  const stack = String(e.stack || '');
  return !stack ? head : !head || stack.startsWith(head) || stack.includes(e.message) ? stack : `${head}\n${stack}`;
}

// an InvalidStateError from a WebGPU object's call ("GPUDevice.createCommandEncoder:
// Unable to make command encoder.")
function gpuRefused(err) {
  return !!err && err.name === 'InvalidStateError' && /^GPU[A-Za-z]+\./.test(String(err.message || ''));
}

function caught(err, where) {
  // the page going: winit's unload on a loop left dead traps after its
  // panic, which the client lets go (main.js's pagehide); no crash
  if (window.eqoaUnloading) {
    console.log(`as the page leaves (${where}): ${errorText(err)}`);
    return;
  }
  // a trap is the client stopping where no panic was raised (out of memory,
  // a stack overflow); after a panic it is that panic's, already said
  if (typeof WebAssembly !== 'undefined' && err instanceof WebAssembly.RuntimeError) {
    crash('The client stopped', errorText(err));
    return;
  }
  const text = err && err.message ? err.message : String(err);
  // the browser's own, harmless
  if (/ResizeObserver loop/.test(text)) return;
  problem('page', `Something in the page failed (${where}): ${text}`);
  // an exception thrown through the client's event loop (the engine's stack
  // overflowing is a RangeError, not a trap) leaves it dead: winit's runner
  // stays borrowed, the game stands still and the page's unload panics in
  // winit ("runner.rs:683:69: RefCell already borrowed", petitions 1618 and
  // 1664), this error's words lost. Asked in a task of its own, when no call
  // of the loop's is under way (web::event_loop_stuck): the client stopped,
  // with this error as its cause.
  setTimeout(() => {
    if (dead || leaving() || !loopStuck()) return;
    notice('problem:page');
    // a winit closure called again while its call the browser stopped never
    // returned is that stop's consequence, not its cause: the watch below
    // says it, after the page has had its time to go (a navigation's stop)
    if (/closure invoked recursively or after being dropped/.test(text)) return;
    // a WebGPU call the browser threw from on a lost device (Safari's,
    // petition 5074) that gpuguard.js did not stand in for: the graphics
    // stopped, which a reload brings back
    if (gpuRefused(err)) crash('The graphics stopped', `the browser's graphics stopped answering (${text}): its graphics process ended, as a browser's does when it runs short of memory`);
    else crash('The client stopped', `${errorText(err)} (in ${stuckIn()})`);
  }, 0);
}

// the page going: from its beforeunload (a navigation's fetch between) to
// its pagehide, a loop stopped is the browser's doing for the navigation,
// no crash
function leaving() {
  return !!window.eqoaUnloading || performance.now() - leavingAt < 10000;
}

function stoppedText() {
  return `the browser stopped the game's script in ${stuckIn()}, with no error to say why (a "page is slowing down" Stop, or the browser out of memory)`;
}

function loopStuck() {
  try { return typeof window.eqoaLoopStuck === 'function' && !!window.eqoaLoopStuck(); } catch { return false; /* the client is gone */ }
}

function stuckIn() {
  try { return (typeof window.eqoaLoopStuckIn === 'function' && window.eqoaLoopStuckIn()) || 'the game'; } catch { return 'the game'; }
}

// A call of the loop's can end with no exception, nothing above to catch:
// the browser stops the page's script where it is (Firefox's "this page is
// slowing down" Stop, an out-of-memory it does not throw, its cancelling a
// running script for a navigation away), and the game stands still with no
// word. So the loop is looked at every second too, in a task of its own (no
// call of the loop's under way: one counted is one that never returned),
// and seen dead twice running, the client stopped is said with where it
// stopped. Not while the page is going (beforeunload to pagehide, a
// navigation's fetch between): that is no crash, the page is leaving.
let stuckSeen = 0;
setInterval(() => {
  if (dead || leaving()) {
    stuckSeen = 0;
    return;
  }
  stuckSeen = loopStuck() ? stuckSeen + 1 : 0;
  if (stuckSeen >= 2) crash('The client stopped', stoppedText());
}, 1000);

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
  caught(e.error || e.message, 'an event');
}, true);

addEventListener('unhandledrejection', (e) => caught(e.reason, 'a promise'));

// ---- for main.js and the client

window.eqoaNotices = {
  notice,
  problem,
  crash,
  // the game runs (in the world or the viewer): a notice's button gives the
  // canvas the keys back
  running(on) { running = on && !dead; },
  get dead() { return dead; },
};
window.eqoaProblem = ([key, text]) => problem(key, text);
// The graphics device was lost and made again (the client's web::recovered):
// said, gone by itself after a minute.
window.eqoaGraphicsBack = (text) => {
  console.warn(text);
  notice('gpu', '', text, [['Dismiss', () => notice('gpu')]]);
  setTimeout(() => notice('gpu'), 60000);
};
window.eqoaCrash = ([title, detail]) => crash(title, detail);
// A web page opened from the game (the client's web::open_link: Station's
// Forgot Password on eqoaclassic) in a new tab. A key's or a touch's press is
// the browser's gesture for it; a pad's is none, and a browser holds such a
// tab back: then a notice with the link, a press of which is one.
window.eqoaOpenLink = (url) => {
  let w = null;
  try {
    w = window.open(url, '_blank');
  } catch { /* held back */ }
  if (w) {
    try { w.opener = null; } catch { /* another origin's */ }
    notice('link');
    return;
  }
  notice('link', '', `Open ${url} in a new tab?`, [
    ['Open', () => { window.open(url, '_blank', 'noopener'); notice('link'); }, true],
    ['Dismiss', () => notice('link')],
  ]);
};
