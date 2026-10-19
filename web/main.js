// The page around the client (crates/eqoa/src/web): it takes the player's disc
// image and login and hands them to the wasm. Query parameters override the
// defaults: ?relay=ws://127.0.0.1:8790&server=203.0.113.10 for a local server.
import init, { play, view } from './eqoa.js';

const $ = (id) => document.getElementById(id);
const status = (t) => { $('status').textContent = t; };
window.eqoaStatus = status;

const params = new URLSearchParams(location.search);
const store = {
  get(k) { try { return localStorage.getItem('eqoa.' + k) || ''; } catch { return ''; } },
  set(k, v) { try { localStorage.setItem('eqoa.' + k, v); } catch { /* private window */ } },
};

const wsBase = (location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host;
for (const k of ['user', 'character', 'relay', 'server', 'viewargs']) {
  $(k).value = params.get(k) || store.get(k);
}
if (!$('relay').value) $('relay').value = wsBase + '/eqoa/relay';

// The game server's IPv4 address: given, or this page's host as the relay
// resolves it (the relay reaches only the server's own ports anyway).
async function serverAddress(relay) {
  const given = $('server').value.trim();
  if (given) return given;
  const host = location.hostname;
  if (/^\d+\.\d+\.\d+\.\d+$/.test(host)) return host;
  const http = relay.replace(/^ws/, 'http');
  const r = await fetch(http + '/resolve?name=' + encodeURIComponent(host));
  if (!r.ok) throw new Error('the relay could not resolve ' + host);
  return (await r.text()).trim();
}

function busy(on) {
  for (const b of ['play', 'view']) $(b).disabled = on;
}

function start() {
  $('panel').style.display = 'none';
  $('eqoa').focus();
}

function failed(e) {
  $('panel').style.display = '';
  busy(false);
  status(String(e && e.message ? e.message : e));
}

function remember() {
  for (const k of ['user', 'character', 'relay', 'server', 'viewargs']) store.set(k, $(k).value.trim());
}

$('form').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const file = $('iso').files[0];
  if (!file) return status('pick your disc image first');
  remember();
  busy(true);
  try {
    const relay = $('relay').value.trim().replace(/\/+$/, '');
    const server = await serverAddress(relay);
    start();
    await play(file, $('user').value.trim(), $('pass').value, { relay, server, character: $('character').value.trim() });
  } catch (e) {
    failed(e);
  }
});

$('view').addEventListener('click', async () => {
  const file = $('iso').files[0];
  if (!file) return status('pick your disc image first');
  remember();
  busy(true);
  try {
    start();
    await view(file, $('viewargs').value.trim() || 'CHAR.ESF 0x70D898C6');
  } catch (e) {
    failed(e);
  }
});

$('eqoa').addEventListener('pointerdown', () => $('eqoa').focus());

init().then(() => status('ready')).catch((e) => status('could not load the client: ' + e));
