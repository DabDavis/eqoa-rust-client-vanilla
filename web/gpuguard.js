// WebGPU as the spec has it on a lost device, in a browser that throws instead
// (web/main.js and fxview.js import this before the client's eqoa.js).
//
// By the spec a lost device's calls never throw: they give invalid objects
// that draw nothing, and the device's `lost` says why. Safari's WebGPU throws
// an InvalidStateError from them instead, once its graphics process has gone
// (iOS ends it under memory pressure): "GPUDevice.createBuffer: Unable to
// create buffer.", "GPUCommandEncoder.beginRenderPass: Unable to begin render
// pass.", "GPUDevice.createCommandEncoder: Unable to make command encoder.".
// wgpu's WebGPU backend unwraps those calls (wgpu-30.0.1
// src/backend/webgpu.rs:2461 createBuffer, :3334 beginRenderPass, and some 20
// more) or calls them through imports that cannot fail, so each was the
// client stopping (petitions 5074, 5076, 5105, 5106) where it would have
// recovered from the loss in place (crates/eqoa/src/gpulost.rs).
//
// Here every method of the objects a device makes is wrapped: an
// InvalidStateError from one takes that device as dead, the call gives a
// stand-in (an object whose every method does nothing and gives another
// stand-in; a promise that never settles where the call gives a promise; an
// ArrayBuffer of the bytes asked for from getMappedRange), every later call
// on that device's objects gives one too without reaching the browser, and
// the client is told once, after the call it was in has returned
// (window.eqoaGpuThrew: gpulost::mark, the device lost, a new one made). Any
// other error is thrown as before, and a device nothing has thrown on is
// called as before. Render and compute pass encoders' own methods (the draws,
// thousands a frame) are not wrapped: none of them throws on a lost device;
// a pass begun on a dead device is a stand-in anyway.
//
// For a test, window.eqoaGpuGuard.simulate() makes the newest device's calls
// throw as Safari's do (pagetest.py's gpuguard; drive.py --js).

const STUB = Symbol('eqoa.gpuguard.stub');
// a GPU object -> the device that made it
const owner = new WeakMap();
// devices taken as dead, and those a test makes throw
const dead = new WeakSet();
const simulated = new WeakSet();
// the newest device asked for: a dead older one (let go after a recovery)
// is no loss of the client's
let newest = null;
let lossesTold = 0;
let stubsMade = 0;

// calls that give a promise, so give one that never settles when stood in for
const PROMISES = new Set(['mapAsync', 'onSubmittedWorkDone', 'popErrorScope', 'createRenderPipelineAsync', 'createComputePipelineAsync', 'getCompilationInfo', 'requestAdapterInfo']);

function extent(size) {
  if (!size) return { width: 0, height: 1, depthOrArrayLayers: 1 };
  if (typeof size[Symbol.iterator] === 'function') {
    const [w = 0, h = 1, d = 1] = size;
    return { width: w, height: h, depthOrArrayLayers: d };
  }
  return { width: size.width || 0, height: size.height ?? 1, depthOrArrayLayers: size.depthOrArrayLayers ?? 1 };
}

// what a stand-in for `name(args)` says of itself (wgpu reads a buffer's size)
function metaFor(name, args) {
  const d = args[0] || {};
  if (name === 'createBuffer') return { size: Number(d.size) || 0, usage: d.usage || 0, mapState: d.mappedAtCreation ? 'mapped' : 'unmapped' };
  if (name === 'createTexture') {
    return { ...extent(d.size), mipLevelCount: d.mipLevelCount ?? 1, sampleCount: d.sampleCount ?? 1, dimension: d.dimension || '2d', format: d.format, usage: d.usage || 0 };
  }
  return {};
}

function mappedBytes(size, args) {
  const offset = Number(args[0]) || 0;
  const len = args[1] === undefined ? (Number(size) || 0) - offset : Number(args[1]);
  return Math.max(0, len || 0);
}

// what a call stood in for gives
function standIn(name, args, size) {
  if (PROMISES.has(name)) return new Promise(() => {});
  if (name === 'getMappedRange') return new ArrayBuffer(mappedBytes(size, args));
  return stub(metaFor(name, args));
}

function stub(meta) {
  stubsMade++;
  const t = { label: '', ...meta, [STUB]: true };
  return new Proxy(t, {
    get(target, k) {
      if (k in target) return target[k];
      // not a thenable, not a primitive
      if (typeof k === 'symbol' || k === 'then' || k === 'toJSON') return undefined;
      return (...args) => standIn(k, args, target.size);
    },
    set(target, k, v) {
      target[k] = v;
      return true;
    },
  });
}

function told(dev, where, e) {
  dead.add(dev);
  if (dev !== newest) return;
  lossesTold++;
  const said = e && e.message ? e.message : String(e);
  // Safari's words name the call already
  const text = said.startsWith(where) ? said : `${where}: ${said}`;
  console.warn(`graphics: the browser threw from a WebGPU call (${text}): the device is taken as lost`);
  // after the client's call that this is inside has returned: the client is
  // not called into from inside its own frame
  queueMicrotask(() => {
    try {
      if (typeof window.eqoaGpuThrew === 'function') window.eqoaGpuThrew(text);
    } catch (err) {
      console.warn(`graphics: the client was not told: ${err}`);
    }
  });
}

function deviceOf(o) {
  return typeof GPUDevice !== 'undefined' && o instanceof GPUDevice ? o : owner.get(o);
}

function wrap(cls, proto, name) {
  const desc = Object.getOwnPropertyDescriptor(proto, name);
  if (!desc || typeof desc.value !== 'function' || name === 'constructor') return;
  const orig = desc.value;
  const guarded = function (...args) {
    // a context configured (again) belongs to the device it is configured
    // with: a fresh device after a loss configures the canvas the dead one had
    if (name === 'configure' && args[0] && args[0].device) owner.set(this, args[0].device);
    const dev = deviceOf(this);
    if (dev && dead.has(dev)) return standIn(name, args, this.size);
    let r;
    try {
      if (dev && simulated.has(dev)) throw new DOMException(`${cls}.${name}: Unable to do it (a test's: the graphics process gone).`, 'InvalidStateError');
      r = orig.apply(this, args);
    } catch (e) {
      // only on a device known: an unconfigured canvas's getCurrentTexture
      // throws an InvalidStateError by the spec, which wgpu takes itself
      if (!dev || !e || e.name !== 'InvalidStateError') throw e;
      told(dev, `${cls}.${name}`, e);
      return standIn(name, args, this.size);
    }
    if (dev && r && typeof r === 'object' && !(r instanceof Promise) && !(r instanceof ArrayBuffer)) owner.set(r, dev);
    return r;
  };
  Object.defineProperty(proto, name, { ...desc, value: guarded });
}

function install() {
  if (typeof GPUDevice === 'undefined' || GPUDevice.prototype.createBuffer?.[STUB]) return false;
  const classes = ['GPUDevice', 'GPUQueue', 'GPUCommandEncoder', 'GPURenderBundleEncoder', 'GPUBuffer', 'GPUTexture', 'GPUCanvasContext', 'GPUQuerySet', 'GPUShaderModule', 'GPURenderPassEncoder', 'GPUComputePassEncoder'];
  for (const cls of classes) {
    const C = globalThis[cls];
    if (!C || !C.prototype) continue;
    for (const name of Object.getOwnPropertyNames(C.prototype)) {
      // the passes' draws and sets: only `end` (a stand-in pass is a stub)
      if ((cls === 'GPURenderPassEncoder' || cls === 'GPUComputePassEncoder') && name !== 'end') continue;
      wrap(cls, C.prototype, name);
    }
  }
  // a device's queue is its own
  const q = Object.getOwnPropertyDescriptor(GPUDevice.prototype, 'queue');
  if (q && q.get) {
    const get = q.get;
    Object.defineProperty(GPUDevice.prototype, 'queue', {
      ...q,
      get() {
        const queue = get.call(this);
        if (queue) owner.set(queue, this);
        return queue;
      },
    });
  }
  // the newest device
  if (typeof GPUAdapter !== 'undefined') {
    const rd = GPUAdapter.prototype.requestDevice;
    GPUAdapter.prototype.requestDevice = function (...args) {
      return rd.apply(this, args).then((d) => {
        newest = d;
        return d;
      });
    };
  }
  GPUDevice.prototype.createBuffer[STUB] = true;
  return true;
}

const installed = install();

window.eqoaGpuGuard = {
  installed,
  // a test's: the newest device's calls throw from now as Safari's do on a
  // lost one
  simulate() {
    if (newest) simulated.add(newest);
    return !!newest;
  },
  isStub: (o) => !!(o && o[STUB]),
  get lossesTold() { return lossesTold; },
  get stubsMade() { return stubsMade; },
};
