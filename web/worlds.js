// The page's worker for a server's own world (main.js's eqoaWorlds.build;
// crates/eqoa/src/web/worlds.rs): the client's own module, instantiated again
// here, checks the copy this origin's private storage keeps of it by its size
// and CRC (world_kept), or builds it there off the player's disc (world_build),
// a zone at a time: the disc read and the file written synchronously, which
// only a worker can do, and never the whole world in memory. One page at a time
// builds a world (a Web Lock on its name): another waits, then finds it kept.
// A page that holds a world for its game holds a shared lock on it too
// (eqoa-world-use-<name>, main.js's holdWorld) for as long as it lives: a world
// a page has in use is never removed or written over here (entry 1323: a
// world's file removed under a game that was reading it failed its world
// change with a NotFoundError), only one no page holds.
import init, { world_kept, world_build, heap_bytes } from './eqoa.js';

self.onmessage = async ({ data }) => {
  const { module, disc, list, name, size, mine } = data;
  const progress = (phase, done, total) => self.postMessage({ progress: [phase, done, total] });
  try {
    // a browser that cannot write a file synchronously here builds nothing
    // (said quietly: it plays as a page without the storage does)
    if (typeof FileSystemFileHandle === 'undefined' || typeof FileSystemFileHandle.prototype.createSyncAccessHandle !== 'function') {
      self.postMessage({ error: 'this browser cannot write a file from a worker', quiet: true });
      return;
    }
    await init({ module_or_path: module });
    // how many pages hold `file` for their game now (this one's counted when
    // `own`): none known where the browser has no Web Locks
    const holders = async (file) => {
      if (!navigator.locks || !navigator.locks.query) return 0;
      const { held = [] } = await navigator.locks.query().catch(() => ({}));
      return held.filter((l) => l.name === `eqoa-world-use-${file}`).length;
    };
    const job = async () => {
      const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle('worlds', { create: true });
      let kept = null;
      try {
        kept = await (await dir.getFileHandle(name)).getFile();
      } catch { /* none kept */ }
      if (kept && world_kept(kept, list, progress)) return 'found kept';
      // a copy that failed its check while another page's game reads it is
      // left to that page (built over, its game would lose its world)
      if (kept && (await holders(name)) > (mine ? 1 : 0)) {
        throw new Error('the copy kept in this browser failed its check while another window of the game has it in use: close the other windows and log in again');
      }
      // room for it in what the browser gives this page, a copy that failed
      // its check counted as free (it is replaced)
      if (navigator.storage.estimate) {
        const { usage, quota } = await navigator.storage.estimate().catch(() => ({}));
        const free = quota - usage + (kept ? kept.size : 0);
        if (quota && free < size) throw new Error(`the browser keeps ${Math.floor(free / 1048576)} MB for this page, not the ${Math.ceil(size / 1048576)} MB it needs`);
      }
      const h = await (await dir.getFileHandle(name, { create: true })).createSyncAccessHandle();
      try {
        world_build(disc, list, h, progress);
      } catch (e) {
        h.close();
        await dir.removeEntry(name).catch(() => {});
        throw e;
      }
      h.close();
      // the same world's builds of another CRC let go, but those a page
      // still has in use (another window on another server, or on this one
      // before its restart): the next build after they are let go removes them
      const stem = name.slice(0, name.lastIndexOf('-') + 1);
      const others = [];
      for await (const other of dir.keys()) {
        if (other !== name && other.startsWith(stem)) others.push(other);
      }
      for (const other of others) {
        if ((await holders(other)) === 0) await dir.removeEntry(other).catch(() => {});
      }
      return 'built';
    };
    const how = navigator.locks ? await navigator.locks.request(`eqoa-world-${name}`, job) : await job();
    self.postMessage({ done: how, heap: heap_bytes() });
  } catch (e) {
    self.postMessage({ error: String(e && e.message ? e.message : e) });
  }
};
