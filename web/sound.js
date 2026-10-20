// The AudioWorklet the client's sound plays through (main.js, eqoaAudio):
// interleaved 48 kHz stereo frames as Int16Arrays, queued as they come and
// played in order, linearly resampled when the context does not run at
// 48 kHz; silence while the queue is dry. Every half second it posts what it
// has played, how many output frames found the queue dry since the first
// frames came, the level and how much is queued, which the page keeps for
// its tests (eqoaAudio.stats).
class EqoaOut extends AudioWorkletProcessor {
  constructor() {
    super();
    this.queue = [];
    this.at = 0; // the frame of queue[0] being played
    this.frac = 0; // and how far on towards the next one
    this.step = 48000 / sampleRate;
    this.played = 0;
    this.dry = 0;
    this.sum = 0;
    this.peak = 0;
    this.n = 0;
    this.fed = false;
    this.port.onmessage = ({ data }) => {
      this.queue.push(data);
      this.fed = true;
    };
  }

  // channel c of the frame k past the one being played, or null if not here yet
  sample(k, c) {
    let i = this.at + k;
    for (const q of this.queue) {
      if (2 * i < q.length) return q[2 * i + c] / 32768;
      i -= q.length / 2;
    }
    return null;
  }

  process(_inputs, outputs) {
    const l = outputs[0][0];
    const r = outputs[0][1] || l;
    for (let i = 0; i < l.length; i++) {
      while (this.queue.length && 2 * this.at >= this.queue[0].length) {
        this.at -= this.queue[0].length / 2;
        this.queue.shift();
      }
      let a = this.sample(0, 0);
      if (a === null) {
        l[i] = r[i] = 0;
        if (this.fed) this.dry++;
        continue;
      }
      let b = this.sample(0, 1);
      if (this.frac > 0) {
        const na = this.sample(1, 0);
        if (na !== null) {
          a += (na - a) * this.frac;
          b += (this.sample(1, 1) - b) * this.frac;
        }
      }
      l[i] = a;
      r[i] = b;
      this.sum += a * a + b * b;
      this.peak = Math.max(this.peak, Math.abs(a), Math.abs(b));
      this.frac += this.step;
      while (this.frac >= 1) {
        this.frac -= 1;
        this.at++;
      }
      this.played++;
    }
    this.n += l.length;
    if (this.n >= sampleRate / 2) {
      const queued = this.queue.reduce((s, q) => s + q.length / 2, 0) - this.at;
      this.port.postMessage({ played: this.played, dry: this.dry, rms: Math.sqrt(this.sum / (2 * this.n)), peak: this.peak, queued });
      this.sum = this.peak = this.n = 0;
    }
    return true;
  }
}

registerProcessor('eqoa-out', EqoaOut);
