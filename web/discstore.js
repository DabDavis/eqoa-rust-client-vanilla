// The disc images a player chose to have remembered in this browser (the
// play page's "Remember my disc images", main.js; the community page,
// spellfx.html): each a File System Access handle kept in this origin's
// IndexedDB under its key ('disc' the Frontiers image, 'disc2002' the 2002
// one) as {handle, name}, never the image itself. Every call answers null
// when the browser has no IndexedDB, or refuses it (a private window).

export const canRemember = typeof window.showOpenFilePicker === 'function' && typeof indexedDB !== 'undefined';

function handleStore(mode, fn) {
  return new Promise((done) => {
    let open;
    try {
      open = indexedDB.open('eqoa-discs', 1);
    } catch {
      return done(null);
    }
    open.onupgradeneeded = () => open.result.createObjectStore('handles');
    open.onerror = () => done(null);
    open.onsuccess = () => {
      const db = open.result;
      try {
        const tx = db.transaction('handles', mode);
        const q = fn(tx.objectStore('handles'));
        tx.oncomplete = () => { db.close(); done(q.result === undefined ? null : q.result); };
        tx.onerror = tx.onabort = () => { db.close(); done(null); };
      } catch {
        db.close();
        done(null);
      }
    };
  });
}

export const keptHandle = (key) => handleStore('readonly', (s) => s.get(key));
export const keepHandle = (key, value) => handleStore('readwrite', (s) => s.put(value, key));
export const forgetHandle = (key) => handleStore('readwrite', (s) => s.delete(key));
