// The game's clock while its page is hidden (main.js): a hidden page gets no
// animation frames, and the browser holds its own timers back (to once a
// second, then once a minute after five minutes), but not a worker's. Told
// true, this posts a tick at the game's 30 frames a second until told false;
// each tick runs a frame of the game with nothing drawn (web::hidden_tick),
// so its network keeps the session (F3 20261025-175001).
let timer = null;
onmessage = ({ data }) => {
  if (data && timer === null) timer = setInterval(() => postMessage(0), 33);
  else if (!data && timer !== null) {
    clearInterval(timer);
    timer = null;
  }
};
