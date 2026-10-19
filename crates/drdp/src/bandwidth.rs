//! `bandwidth.cpp`: a leaky bucket over the send queue plus a one-second
//! sample window for the two bps counters. Its arithmetic is the EE's
//! `float`, so it is generic over the float model.

use vi::fpu::{F, Model, f};

#[derive(Clone, Copy, Debug, Default)]
pub struct Bandwidth<M: Model> {
    pub bps_limit: F<M>,
    pub queue_size: F<M>,
    pub queue_size_max: F<M>,
    pub last_process: u32,
    pub bps_sent: i32,
    pub bps_recv: i32,
    pub last_sample: u32,
    pub sent_since: i64,
    pub recv_since: i64,
}

impl<M: Model> Bandwidth<M> {
    pub fn new(bps_limit: i32, queue_size_max: i32) -> Self {
        Bandwidth { bps_limit: F::new(bps_limit as f32), queue_size_max: F::new(queue_size_max as f32), ..Default::default() }
    }

    pub fn process(&mut self, time: u32) {
        let secs = F::<M>::new(time.wrapping_sub(self.last_process) as f32) * f(0.001);
        self.queue_size -= self.bps_limit * secs;
        if self.queue_size < F::ZERO {
            self.queue_size = F::ZERO;
        }
        self.last_process = time;
        let since = time.wrapping_sub(self.last_sample);
        if since > 1000 {
            self.bps_sent = ((1000 * self.sent_since) / since as i64) as i32;
            self.bps_recv = ((1000 * self.recv_since) / since as i64) as i32;
            self.last_sample = time;
            self.recv_since = 0;
            self.sent_since = 0;
        }
    }

    pub fn send(&mut self, time: u32, size: i32) {
        self.process(time);
        self.queue_size += F::new(size as f32);
        self.sent_since += size as i64;
    }

    pub fn recv(&mut self, time: u32, size: i32) {
        self.process(time);
        self.recv_since += size as i64;
    }

    /// `bandwidth_get_flush_time`: now plus the time the bucket needs to
    /// drain below its maximum.
    pub fn flush_time(&mut self, time: u32) -> u32 {
        self.process(time);
        let mut over = self.queue_size - self.queue_size_max;
        if over < F::ZERO {
            over = F::ZERO;
        }
        time.wrapping_add((over * f(1000.0) / self.bps_limit).0 as u32)
    }

    pub fn process_interval_max(&self) -> F<M> {
        self.queue_size_max / self.bps_limit
    }

    pub fn set_bps_limit(&mut self, time: u32, limit: i32) {
        self.process(time);
        self.bps_limit = F::new(limit as f32);
    }
}
