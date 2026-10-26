// The spell-visual preview's page (fxview.html; crates/eqoa/src/fxview.rs):
// a human casting a spell visual as a cast's 0x00B1 plays it in the game.
// The GM panel (go-eqoa-server's internal/gmweb, on its own host) frames it
// and talks to it by postMessage only, both ways checked by origin; so does the
// community page beside it (spellfx.html, this site's own: it hands over the
// player's disc image itself); opened on its own, it asks for the disc image.
//
// From the panel: {type: 'fxview-files', files: [[name, Blob], ...], play:
// {effect, castMs, aim}, dummy: {model, size}} starts it; then 'fxview-play'
// {effect, castMs, aim}, 'fxview-again', 'fxview-aim' {aim}, 'fxview-distance'
// {distance}, 'fxview-camera' {yaw, pitch, zoom}; or, from a page of this site,
// 'fxview-disc' {file: the disc image (a File), play, dummy} in place of the
// files. To the panel, each {source: 'eqoa-fxview', type, ...}: 'ready'
// {files} (send them now), 'status' {text}, 'started' {visuals: every visual
// on the disc, '0x...' in its order}, 'error' {text}.

import init, { fxview, fxview_disc, fxview_play, fxview_again, fxview_aim, fxview_distance, fxview_camera, fxview_visuals } from './eqoa.js';

const $ = (id) => document.getElementById(id);
const framed = window.parent !== window;

// The panels allowed to drive it; a test's panel on this machine may name
// itself with ?panel=<origin> (loopback only).
// A page of this site's own (the community page, spellfx.html) may too.
const ALLOWED = new Set(['https://gm.eqoaclassic.com', location.origin]);
let sameSite = false;
try { sameSite = framed && window.parent.location.origin === location.origin; } catch { /* another site's frame */ }
const asked = new URLSearchParams(location.search).get('panel');
if (asked) {
  try {
    const u = new URL(asked);
    if (/^(127\.\d+\.\d+\.\d+|localhost|\[::1\])$/.test(u.hostname)) ALLOWED.add(u.origin);
  } catch { /* not an origin */ }
}
let panel = null; // the origin of the panel that spoke first

function tell(type, extra) {
  if (!framed) return;
  const msg = Object.assign({ source: 'eqoa-fxview', type }, extra || {});
  // before the panel has spoken, each allowed origin is tried: a message
  // for an origin the parent is not is dropped by the browser
  for (const o of panel ? [panel] : ALLOWED) window.parent.postMessage(msg, o);
}

window.eqoaStatus = (text) => {
  $('status').textContent = text;
  tell('status', { text });
};
window.eqoaCrash = (a) => {
  const text = Array.isArray(a) ? a.join(': ') : String(a);
  $('status').textContent = text;
  tell('error', { text });
};
window.eqoaProblem = (a) => window.eqoaStatus(Array.isArray(a) ? a[1] : String(a));

const ready = init({ module_or_path: new URL('eqoa_bg.wasm', import.meta.url) });
let started = false;

async function begin(run) {
  if (started) return;
  started = true;
  $('pick').hidden = true;
  try {
    await ready;
    await run();
    let visuals = [];
    try { visuals = Array.from(fxview_visuals()); } catch { /* an older build */ }
    tell('started', { visuals });
    $('eqoa').focus();
  } catch (e) {
    started = false;
    const text = String(e && e.message || e);
    $('status').textContent = text;
    tell('error', { text });
    if (!framed) { $('pick').hidden = false; $('error').textContent = text; }
  }
}

addEventListener('message', (e) => {
  if (e.source !== window.parent || !ALLOWED.has(e.origin)) return;
  const m = e.data || {};
  if (typeof m.type !== 'string' || !m.type.startsWith('fxview-')) return;
  panel = e.origin;
  const play = m.play || m;
  switch (m.type) {
    case 'fxview-files': {
      const opts = { effect: play.effect, castMs: play.castMs, aim: play.aim, dummy: m.dummy && m.dummy.model, dummySize: m.dummy && m.dummy.size };
      begin(() => fxview(m.files, opts));
      break;
    }
    case 'fxview-disc': {
      const opts = { effect: play.effect, castMs: play.castMs, aim: play.aim, dummy: m.dummy && m.dummy.model, dummySize: m.dummy && m.dummy.size };
      if (m.file instanceof Blob) begin(() => fxview_disc(m.file, opts));
      break;
    }
    case 'fxview-play':
      if (started) fxview_play(String(play.effect), Number(play.castMs ?? 2000), String(play.aim || ''));
      break;
    case 'fxview-again': if (started) fxview_again(); break;
    case 'fxview-aim': if (started) fxview_aim(String(m.aim)); break;
    case 'fxview-distance': if (started) fxview_distance(Number(m.distance)); break;
    case 'fxview-camera': if (started) fxview_camera(Number(m.yaw), Number(m.pitch), Number(m.zoom)); break;
  }
});

if (framed) {
  $('status').textContent = sameSite ? 'waiting for your disc image…' : 'waiting for the GM panel…';
  ready.then(() => tell('ready', { files: ['CHARCUST.CSF', 'CHARFACE.CSF', 'CHAR.ESF', 'SPELLFX.CSF', 'FX.CSF', 'ITEM.ESF'] }), (e) => tell('error', { text: String(e) }));
} else {
  // on its own: the disc picked here, then a bar to change what plays
  $('pick').hidden = false;
  $('status').textContent = 'pick your disc image';
  const p = new URLSearchParams(location.search);
  if (p.get('effect')) $('fx').value = p.get('effect');
  if (p.get('aim')) $('aim').value = p.get('aim');
  $('pick-form').addEventListener('submit', (ev) => {
    ev.preventDefault();
    const file = $('iso').files[0];
    if (!file) return;
    const opts = { effect: $('fx').value, castMs: Math.round(Number($('secs').value || 2) * 1000), aim: $('aim').value };
    $('bar-fx').value = $('fx').value;
    $('bar-secs').value = $('secs').value;
    $('bar-aim').value = $('aim').value;
    $('bar').hidden = false;
    begin(() => fxview_disc(file, opts));
  });
  const replay = () => fxview_play($('bar-fx').value, Math.round(Number($('bar-secs').value || 2) * 1000), $('bar-aim').value);
  $('bar-play').addEventListener('click', replay);
  $('bar-fx').addEventListener('keydown', (ev) => { if (ev.key === 'Enter') replay(); });
  $('bar-aim').addEventListener('change', () => fxview_aim($('bar-aim').value));
  $('bar-again').addEventListener('click', () => fxview_again());
}
