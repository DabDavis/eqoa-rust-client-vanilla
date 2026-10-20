// The page's worker for one DNAS module (main.js's eqoaUnpack): the client's
// own wasm module, instantiated again here, runs the disc's DNAS library on
// it (unpack_module, crate dnas), away from the page's thread.
import init, { unpack_module } from './eqoa.js';

self.onmessage = async ({ data }) => {
  try {
    await init({ module_or_path: data.module });
    const out = unpack_module(data.util, data.aut);
    self.postMessage({ out }, [out.buffer]);
  } catch (e) {
    self.postMessage({ error: String(e && e.message ? e.message : e) });
  }
};
