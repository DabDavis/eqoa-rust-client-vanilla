// Notices from the page: a line at the top of the page, over the game too,
// for what the player should know. A new version of the client (main.js);
// something gone wrong short of stopping the client (a file of the build
// that would not load, a module that would not unpack, no sound: `problem`);
// the client stopped (a panic, a trap, the graphics device lost: `crash`),
// which only a reload gets past. Nothing is sent anywhere.
//
// This file is a module of its own, loaded before main.js and importing
// nothing, so that it runs when the client's files do not: the page's errors
// are its to catch. The client reaches it through window.eqoaProblem and
// window.eqoaCrash (crates/eqoa/src/web).

const $ = (id) => document.getElementById(id);

function el(tag, props, ...kids) {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
}

let dead = false;    // the client stopped: nothing brings it back but a reload
let running = false; // in the world, or the viewer: the canvas has the keys

// ---- notices: one per key, replaced when the same key comes again, gone
// when its text is empty

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

// Something went wrong short of a crash: said on the page until dismissed.
function problem(key, text) {
  console.warn(`problem (${key}): ${text}`);
  notice('problem:' + key, 'bad', text, [['Dismiss', () => notice('problem:' + key)]]);
}

// The client stopped: `title`, and `detail` (the panic, web::crash; or the
// error that trapped it). The canvas keeps its last frame, so the page says
// so, or a player would see only a game that no longer moves.
function crash(title, detail) {
  if (dead) return; // the panic's own notice is up; its trap follows it
  dead = true;
  running = false;
  console.error(`${title}: ${detail}`);
  const what = detail.split('\n').map((l) => l.trim()).filter(Boolean).slice(0, 3).join(' ').slice(0, 300);
  notice('crash', 'bad', `${title}${what ? ` (${what})` : ''}. Reload the page to play again.`, [
    ['Reload', () => location.reload(), true],
  ]);
}

// ---- what the page catches

function errorText(e) {
  return e && (e.stack || e.message) ? String(e.stack || e.message) : String(e);
}

function caught(err, where) {
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
  caught(e.error || e.message, 'an event');
}, true);

addEventListener('unhandledrejection', (e) => caught(e.reason, 'a promise'));

// ---- for main.js and the client

window.eqoaNotices = {
  notice,
  problem,
  crash,
  // the game runs (in the world or the viewer): a notice's button gives the
  // keys back to the canvas
  running(on) { running = on && !dead; },
};
window.eqoaProblem = ([key, text]) => problem(key, text);
window.eqoaCrash = ([title, detail]) => crash(title, detail);
