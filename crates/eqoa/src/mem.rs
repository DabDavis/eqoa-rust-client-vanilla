//! What memory the program holds, for a report and the log: the process's
//! (resident and its peak, and what it holds as its own: Windows' commit
//! charge, a Mac's footprint, Linux's anonymous pages and swap), the
//! machine's (memory free of all of it, and the commit limit), and the
//! graphics device's (wgpu's count of its buffers and bind groups, the bytes
//! under its buffers and textures, and, where the backend has one (Vulkan,
//! DX12), its allocator's blocks: on an integrated GPU they are the
//! machine's memory too). A page has the wasm heap and, where the browser
//! says it, the JavaScript heap.
//!
//! F3 crash 20261026-220031: a Windows client (Intel UHD, DX12) ran 11 hours
//! at Freeport's busiest spot and stopped with Rust's "memory allocation of
//! 229376 bytes failed"; nothing it sent could tell a leak from a machine
//! short of memory. Now the figures are taken every 10 s (the allocator's
//! every minute: its report lists every allocation) on the frame that draws
//! (`tick`), and every 10 minutes of the run they go into the log and the
//! run's own list, which every report's state carries (`describe`), a crash's
//! too, when the chat has long pushed those lines out of the log's last.

use std::sync::Mutex;
use web_time::{Duration, Instant};

/// The figures, taken this often.
const EVERY: Duration = Duration::from_secs(10);
/// The allocator's report, this often: it copies every allocation's name.
const GPU_EVERY: Duration = Duration::from_secs(60);
/// The log's line and the run's list, this often (natively
/// `EQOA_MEM_EVERY=<seconds>` for a test's run).
const LOG_EVERY: Duration = Duration::from_secs(600);
/// The run's list keeps its first and its last this many.
const KEPT: usize = 24;

/// A size as a reader takes it: "640 KB", "812 MB", "3.1 GB", "16 GB".
pub fn size(b: u64) -> String {
    const K: f64 = 1024.0;
    let f = b as f64;
    if f >= 10.0 * K * K * K {
        format!("{:.0} GB", f / (K * K * K))
    } else if f >= K * K * K {
        format!("{:.1} GB", f / (K * K * K))
    } else if f >= K * K {
        format!("{:.0} MB", f / (K * K))
    } else {
        format!("{:.0} KB", f / K)
    }
}

/// The process's memory.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Process {
    /// in physical memory: Windows' working set, Linux's VmRSS, a Mac's
    /// resident size
    pub resident: u64,
    /// its most: the peak working set, VmHWM (a Mac: none, its footprint's
    /// peak is `own_peak`)
    pub peak: Option<u64>,
    /// held as its own wherever it is: Windows' private bytes (the commit
    /// charge, what an allocation fails on when the machine has no more),
    /// a Mac's physical footprint (Activity Monitor's Memory), Linux's
    /// anonymous resident pages and swap
    pub own: u64,
    /// its most: Windows' peak commit, a Mac's lifetime footprint
    pub own_peak: Option<u64>,
}

/// The machine's memory.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Machine {
    pub total: u64,
    /// free for programs (Windows' available, Linux's MemAvailable, a Mac's
    /// memory pressure level times its total)
    pub free: u64,
    /// committed of the limit: Windows' (physical memory and the page
    /// file), Linux's Committed_AS of CommitLimit
    pub commit: Option<(u64, u64)>,
}

/// The graphics device's: wgpu's counts (its `counters`) and its
/// allocator's report, where the backend has one. Its count of textures is
/// left out: wgpu-hal 30's Vulkan counts their bytes and not their making
/// (DX12 counts both); the allocator's labels count them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graphics {
    pub buffers: i64,
    pub buffer_bytes: i64,
    pub texture_bytes: i64,
    pub bind_groups: i64,
    pub allocator: Option<Allocator>,
}

/// wgpu's allocator (gpu-allocator, under Vulkan and DX12): its blocks, what
/// they hold, and by whose label.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Allocator {
    /// when it was taken, since the run started
    pub at: Duration,
    pub reserved: u64,
    pub used: u64,
    pub blocks: usize,
    pub allocations: usize,
    /// the biggest blocks, biggest first
    pub largest: Vec<u64>,
    /// the labels holding the most (bytes, allocations), most first
    pub by_label: Vec<(String, u64, usize)>,
}

impl Allocator {
    /// The report's sums, its biggest blocks (`blocks` of them) and the
    /// labels holding most (`labels` of them).
    pub fn of(r: &render::wgpu::AllocatorReport, at: Duration, blocks: usize, labels: usize) -> Allocator {
        let mut largest: Vec<u64> = r.blocks.iter().map(|b| b.size).collect();
        largest.sort_unstable_by(|a, b| b.cmp(a));
        largest.truncate(blocks);
        let mut by: std::collections::HashMap<&str, (u64, usize)> = std::collections::HashMap::new();
        for a in &r.allocations {
            let e = by.entry(a.name.as_str()).or_default();
            e.0 += a.size;
            e.1 += 1;
        }
        let mut by_label: Vec<(String, u64, usize)> = by.into_iter().map(|(k, (b, n))| (if k.is_empty() { "-".to_string() } else { k.to_string() }, b, n)).collect();
        by_label.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        by_label.truncate(labels);
        Allocator { at, reserved: r.total_reserved_bytes, used: r.total_allocated_bytes, blocks: r.blocks.len(), allocations: r.allocations.len(), largest, by_label }
    }
}

impl Graphics {
    /// The device's counts, and its allocator's report when `allocator`
    /// (the run's time is `at`).
    pub fn of(device: &render::wgpu::Device, allocator: bool, at: Duration) -> Graphics {
        let c = device.get_internal_counters().hal;
        Graphics {
            buffers: c.buffers.read() as i64,
            buffer_bytes: c.buffer_memory.read() as i64,
            texture_bytes: c.texture_memory.read() as i64,
            bind_groups: c.bind_groups.read() as i64,
            allocator: if allocator { device.generate_allocator_report().map(|r| Allocator::of(&r, at, 3, 5)) } else { None },
        }
    }
}

/// One taking of the figures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    /// since the run started
    pub at: Duration,
    pub process: Option<Process>,
    pub machine: Option<Machine>,
    pub graphics: Option<Graphics>,
    /// a page's wasm heap, and the JavaScript heap (used, limit) where the
    /// browser says it
    pub page: Option<(u64, Option<(u64, u64)>)>,
}

/// "1:05:00" for a time since the start.
fn clock(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// The run's start as the log's `[memory]` lines count it (the first
/// `tick`), kept apart from `WATCH` so any thread can read it at once.
static RUN_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// The moment `t` on the clock the log's `[memory]` lines say ("2:13:05"
/// into the run), for the other lines a report is read beside them with
/// (`stall`).
pub fn clock_at(t: Instant) -> String {
    RUN_START.get().map_or_else(|| clock(Duration::ZERO), |s| clock(t.saturating_duration_since(*s)))
}

impl Sample {
    /// The figures in a line.
    pub fn line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(p) = &self.process {
            let peak = |v: Option<u64>| v.map_or(String::new(), |v| format!(" (peak {})", size(v)));
            parts.push(format!("resident {}{}, own {}{}", size(p.resident), peak(p.peak), size(p.own), peak(p.own_peak)));
        }
        if let Some((wasm, js)) = &self.page {
            parts.push(format!("wasm heap {}{}", size(*wasm), js.map_or(String::new(), |(u, l)| format!(", JavaScript heap {} of {}", size(u), size(l)))));
        }
        if let Some(m) = &self.machine {
            let commit = m.commit.map_or(String::new(), |(c, l)| format!(", committed {} of {}", size(c), size(l)));
            parts.push(format!("machine {} free of {}{commit}", size(m.free), size(m.total)));
        }
        if let Some(g) = &self.graphics {
            let mut s = format!("graphics {} buffers ({}), textures {}, {} bind groups", g.buffers, size(g.buffer_bytes.max(0) as u64), size(g.texture_bytes.max(0) as u64), g.bind_groups);
            if let Some(a) = &g.allocator {
                let largest: Vec<String> = a.largest.iter().map(|&b| size(b)).collect();
                let labels: Vec<String> = a.by_label.iter().map(|(l, b, n)| format!("{l} {} ({n})", size(*b))).collect();
                let when = if a.at == self.at { String::new() } else { format!(" (at {})", clock(a.at)) };
                s += &format!("; allocator{when} {} in {} blocks (largest {}), {} used by {}: {}", size(a.reserved), a.blocks, largest.join(", "), size(a.used), a.allocations, labels.join(", "));
            }
            parts.push(s);
        }
        if parts.is_empty() { "unknown here".to_string() } else { parts.join("; ") }
    }

    /// The run list's entry: the time, then what is held as the process's
    /// own (a page's wasm heap) and the allocator's blocks (else the
    /// device's buffers' and textures' bytes), in MB.
    fn short(&self) -> String {
        const MB: u64 = 1 << 20;
        let own = self.process.map(|p| p.own).or(self.page.map(|p| p.0)).map_or("-".to_string(), |b| (b / MB).to_string());
        let gpu = self.graphics.as_ref().map_or("-".to_string(), |g| match &g.allocator {
            Some(a) => (a.reserved / MB).to_string(),
            None => ((g.buffer_bytes.max(0) + g.texture_bytes.max(0)) as u64 / MB).to_string(),
        });
        format!("{} {own} {gpu}", clock(self.at))
    }
}

/// The run's figures: the last taken, and the list of those every
/// `LOG_EVERY`.
struct Watch {
    start: Option<Instant>,
    last: Option<Sample>,
    taken: Option<Instant>,
    gpu_taken: Option<Instant>,
    logged: Option<Instant>,
    /// the run's list: its first, and its last `KEPT` (whether any between
    /// were let go)
    first: Option<Sample>,
    list: std::collections::VecDeque<Sample>,
    dropped: bool,
}

static WATCH: Mutex<Watch> = Mutex::new(Watch { start: None, last: None, taken: None, gpu_taken: None, logged: None, first: None, list: std::collections::VecDeque::new(), dropped: false });

fn lock() -> std::sync::MutexGuard<'static, Watch> {
    WATCH.lock().unwrap_or_else(|e| e.into_inner())
}

/// How often the log's line comes (never more often than the figures are
/// taken).
fn log_every() -> Duration {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(s) = std::env::var("EQOA_MEM_EVERY").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
        return Duration::from_secs(s).max(EVERY);
    }
    LOG_EVERY
}

impl Watch {
    /// Whether the log's line is due at `now`.
    fn log_due(&self, now: Instant, every: Duration) -> bool {
        self.logged.is_none_or(|t| now.duration_since(t) >= every)
    }

    /// A sample `s` taken at `now`: kept as the last, and, when the log's
    /// is due, put in the run's list and handed back for the log.
    fn keep(&mut self, s: Sample, now: Instant, every: Duration) -> Option<Sample> {
        self.last = Some(s.clone());
        if !self.log_due(now, every) {
            return None;
        }
        self.logged = Some(now);
        if self.first.is_none() {
            self.first = Some(s.clone());
        } else {
            self.list.push_back(s.clone());
            while self.list.len() > KEPT {
                self.list.pop_front();
                self.dropped = true;
            }
        }
        Some(s)
    }

    fn describe(&self, now: Instant) -> String {
        let Some(last) = &self.last else { return "memory: not taken yet".to_string() };
        let ago = self.taken.map_or(0, |t| now.duration_since(t).as_secs());
        let mut o = format!("memory: {} (taken {ago} s ago, at {})", last.line(), clock(last.at));
        if let Some(f) = &self.first {
            let mut all = vec![f.short()];
            if self.dropped {
                all.push("…".to_string());
            }
            all.extend(self.list.iter().map(Sample::short));
            let every = log_every().as_secs();
            let every = if every % 60 == 0 { format!("{} min", every / 60) } else { format!("{every} s") };
            o += &format!("\nmemory over the run, every {every} (time, own MB, graphics MB): {}", all.join(" · "));
        }
        o
    }
}

/// The figures, on a frame that draws: taken when they are due (every 10 s,
/// the allocator's every minute), and every 10 minutes into the log. Cheap
/// when nothing is due: a look at the clock.
pub fn tick(device: Option<&render::wgpu::Device>) {
    let now = Instant::now();
    let mut w = lock();
    let start = *w.start.get_or_insert(now);
    RUN_START.get_or_init(|| start);
    if w.taken.is_some_and(|t| now.duration_since(t) < EVERY) {
        return;
    }
    w.taken = Some(now);
    let every = log_every();
    // the log's line has the allocator's report of its own moment
    let gpu = w.log_due(now, every) || w.gpu_taken.is_none_or(|t| now.duration_since(t) >= GPU_EVERY);
    if gpu && device.is_some() {
        w.gpu_taken = Some(now);
    }
    let at = now.duration_since(start);
    let mut s = Sample { at, process: process(), machine: machine(), graphics: device.map(|d| Graphics::of(d, gpu, at)), page: page() };
    // the allocator's last report kept between its takings
    if let (Some(g), Some(prev)) = (s.graphics.as_mut(), w.last.as_ref().and_then(|l| l.graphics.as_ref())) {
        if g.allocator.is_none() {
            g.allocator.clone_from(&prev.allocator);
        }
    }
    let logged = w.keep(s, now, every);
    drop(w);
    if let Some(s) = logged {
        crate::say(&format!("[memory] {} {}", clock(s.at), s.line()));
    }
}

/// For a report's state: the figures last taken, and the run's list.
pub fn describe() -> String {
    lock().describe(Instant::now())
}

// ---- the process and the machine, per system --------------------------------

/// A `Key:   123 kB` line's value of /proc's, in bytes.
#[cfg(any(target_os = "linux", test))]
fn proc_kb(text: &str, key: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.strip_prefix(key).is_some_and(|r| r.starts_with(':')))?;
    let v: u64 = line[key.len() + 1..].split_whitespace().next()?.parse().ok()?;
    Some(v * 1024)
}

#[cfg(any(target_os = "linux", test))]
fn linux_process(status: &str) -> Option<Process> {
    let resident = proc_kb(status, "VmRSS")?;
    let own = proc_kb(status, "RssAnon").unwrap_or(resident) + proc_kb(status, "VmSwap").unwrap_or(0);
    Some(Process { resident, peak: proc_kb(status, "VmHWM"), own, own_peak: None })
}

#[cfg(any(target_os = "linux", test))]
fn linux_machine(meminfo: &str) -> Option<Machine> {
    let total = proc_kb(meminfo, "MemTotal")?;
    let free = proc_kb(meminfo, "MemAvailable").or_else(|| proc_kb(meminfo, "MemFree"))?;
    let commit = proc_kb(meminfo, "Committed_AS").zip(proc_kb(meminfo, "CommitLimit"));
    Some(Machine { total, free, commit })
}

#[cfg(target_os = "linux")]
fn process() -> Option<Process> {
    linux_process(&std::fs::read_to_string("/proc/self/status").ok()?)
}

#[cfg(target_os = "linux")]
fn machine() -> Option<Machine> {
    linux_machine(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(windows)]
fn process() -> Option<Process> {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    let mut c = PROCESS_MEMORY_COUNTERS_EX { cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
    // SAFETY: the process's own pseudo handle (GetCurrentProcess's -1) and a
    // struct of the size `cb` says; the _EX form is taken where _ is asked
    let ok = unsafe { K32GetProcessMemoryInfo(-1isize as _, &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS, c.cb) };
    (ok != 0).then(|| Process { resident: c.WorkingSetSize as u64, peak: Some(c.PeakWorkingSetSize as u64), own: c.PrivateUsage as u64, own_peak: Some(c.PeakPagefileUsage as u64) })
}

#[cfg(windows)]
fn machine() -> Option<Machine> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: a struct of the size `dwLength` says
    let ok = unsafe { GlobalMemoryStatusEx(&mut m) };
    (ok != 0).then(|| Machine {
        total: m.ullTotalPhys,
        free: m.ullAvailPhys,
        // the commit limit is physical memory and the page file; what is
        // committed, the limit less what is left of it
        commit: Some((m.ullTotalPageFile.saturating_sub(m.ullAvailPageFile), m.ullTotalPageFile)),
    })
}

#[cfg(target_os = "macos")]
fn process() -> Option<Process> {
    let mut r: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    // SAFETY: this process, and a struct of the flavour asked for
    let ok = unsafe { libc::proc_pid_rusage(libc::getpid(), libc::RUSAGE_INFO_V4, &mut r as *mut libc::rusage_info_v4 as *mut libc::rusage_info_t) };
    (ok == 0).then(|| Process { resident: r.ri_resident_size, peak: None, own: r.ri_phys_footprint, own_peak: Some(r.ri_lifetime_max_phys_footprint) })
}

#[cfg(target_os = "macos")]
fn machine() -> Option<Machine> {
    fn sysctl<T: Copy + Default>(name: &std::ffi::CStr) -> Option<T> {
        let mut v = T::default();
        let mut n = size_of::<T>();
        // SAFETY: a value of the size the name's is
        let ok = unsafe { libc::sysctlbyname(name.as_ptr(), &mut v as *mut T as *mut libc::c_void, &mut n, std::ptr::null_mut(), 0) };
        (ok == 0 && n == size_of::<T>()).then_some(v)
    }
    let total: u64 = sysctl(c"hw.memsize")?;
    // the system's own measure of what is free for programs, 0..100
    let level: i32 = sysctl(c"kern.memorystatus_level").unwrap_or(0);
    Some(Machine { total, free: total / 100 * level.clamp(0, 100) as u64, commit: None })
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn process() -> Option<Process> {
    None
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn machine() -> Option<Machine> {
    None
}

/// A page's wasm heap, and the JavaScript heap where the browser says it
/// (Chromium's `performance.memory`).
#[cfg(target_arch = "wasm32")]
fn page() -> Option<(u64, Option<(u64, u64)>)> {
    use wasm_bindgen::JsCast;
    let wasm = wasm_bindgen::memory().dyn_into::<js_sys::WebAssembly::Memory>().ok()?.buffer().dyn_into::<js_sys::ArrayBuffer>().ok()?.byte_length() as u64;
    let js = (|| {
        let perf = js_sys::Reflect::get(&js_sys::global(), &"performance".into()).ok()?;
        let m = js_sys::Reflect::get(&perf, &"memory".into()).ok().filter(|m| m.is_object())?;
        let n = |k: &str| js_sys::Reflect::get(&m, &k.into()).ok()?.as_f64();
        Some((n("usedJSHeapSize")? as u64, n("jsHeapSizeLimit")? as u64))
    })();
    Some((wasm, js))
}

#[cfg(not(target_arch = "wasm32"))]
fn page() -> Option<(u64, Option<(u64, u64)>)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_as_a_reader_takes_them() {
        assert_eq!(size(640 << 10), "640 KB");
        assert_eq!(size(812 << 20), "812 MB");
        assert_eq!(size(3 << 30 | 100 << 20), "3.1 GB");
        assert_eq!(size(16 << 30), "16 GB");
        assert_eq!(size(0), "0 KB");
    }

    /// /proc's lines as Linux writes them; the control: a key that is a
    /// prefix of another (VmRSS of a made-up VmRSSx) is not read as it.
    #[test]
    fn proc_lines_are_read() {
        let status = "Name:\teqoa\nVmHWM:\t  900000 kB\nVmRSSx:\t 1 kB\nVmRSS:\t  812000 kB\nRssAnon:\t  700000 kB\nVmSwap:\t    1000 kB\n";
        let p = linux_process(status).unwrap();
        assert_eq!(p, Process { resident: 812000 * 1024, peak: Some(900000 * 1024), own: 701000 * 1024, own_peak: None });
        assert_eq!(proc_kb("VmRSSx:\t 1 kB\n", "VmRSS"), None, "the control: a longer key is another");
        let meminfo = "MemTotal:       16000000 kB\nMemFree:          100000 kB\nMemAvailable:    9000000 kB\nCommitLimit:    24000000 kB\nCommitted_AS:   12000000 kB\n";
        let m = linux_machine(meminfo).unwrap();
        assert_eq!(m, Machine { total: 16000000 * 1024, free: 9000000 * 1024, commit: Some((12000000 * 1024, 24000000 * 1024)) });
    }

    /// This process's own figures are there natively, resident under its
    /// peak; the control: a sample with none says so.
    #[cfg(any(target_os = "linux", windows, target_os = "macos"))]
    #[test]
    fn this_process_is_measured() {
        let p = process().expect("the process's memory");
        assert!(p.resident > 1 << 20, "{p:?}");
        assert!(p.peak.is_none_or(|k| k >= p.resident), "{p:?}");
        let m = machine().expect("the machine's memory");
        assert!(m.total > p.resident && m.free <= m.total, "{m:?}");
        let line = Sample { process: Some(p), machine: Some(m), ..Default::default() }.line();
        assert!(line.starts_with("resident ") && line.contains("machine "), "{line}");
        assert_eq!(Sample::default().line(), "unknown here");
    }

    /// The allocator's report in a line: its sums, its biggest blocks and the
    /// labels holding most, summed over their allocations.
    #[test]
    fn the_allocators_report_is_summed_by_label() {
        use render::wgpu::{AllocatorReport, wgt::{AllocationReport, MemoryBlockReport}};
        let a = |name: &str, size: u64| AllocationReport { name: name.to_string(), offset: 0, size };
        let r = AllocatorReport {
            allocations: vec![a("vertices", 3 << 20), a("ui", 1 << 20), a("vertices", 2 << 20), a("", 1 << 10)],
            blocks: vec![MemoryBlockReport { size: 32 << 20, allocations: 0..2 }, MemoryBlockReport { size: 64 << 20, allocations: 2..4 }],
            total_allocated_bytes: (6 << 20) + (1 << 10),
            total_reserved_bytes: 96 << 20,
        };
        let al = Allocator::of(&r, Duration::from_secs(60), 3, 2);
        assert_eq!(al.largest, vec![64 << 20, 32 << 20]);
        assert_eq!(al.by_label, vec![("vertices".to_string(), 5 << 20, 2), ("ui".to_string(), 1 << 20, 1)]);
        let g = Graphics { buffers: 10, buffer_bytes: 5 << 20, texture_bytes: 1 << 20, bind_groups: 7, allocator: Some(al) };
        let line = Sample { at: Duration::from_secs(60), graphics: Some(g.clone()), ..Default::default() }.line();
        assert_eq!(line, "graphics 10 buffers (5 MB), textures 1 MB, 7 bind groups; allocator 96 MB in 2 blocks (largest 64 MB, 32 MB), 6 MB used by 4: vertices 5 MB (2), ui 1 MB (1)");
        assert!(!line.contains("(at "), "the control: a report of the sample's own moment says no time");
        // a report from a minute before says when it was
        let later = Sample { at: Duration::from_secs(70), graphics: Some(g), ..Default::default() }.line();
        assert!(later.contains("; allocator (at 0:01:00) 96 MB"), "{later}");
    }

    /// The run's list keeps its first and its last `KEPT`, one a period
    /// however often the figures are taken; the control: the samples taken
    /// between are the last's, never the list's.
    #[test]
    fn the_runs_list_keeps_its_first_and_its_last() {
        let t0 = Instant::now();
        let mut w = Watch { start: Some(t0), last: None, taken: None, gpu_taken: None, logged: None, first: None, list: Default::default(), dropped: false };
        let every = Duration::from_secs(600);
        let mut logged = 0;
        let step = |w: &mut Watch, i: u64| {
            let at = Duration::from_secs(i * 10);
            let s = Sample { at, process: Some(Process { resident: i, peak: None, own: i << 20, own_peak: None }), ..Default::default() };
            w.keep(s, t0 + at, every).is_some() as usize
        };
        // the list full and nothing let go: no mark of a gap
        for i in 0..=KEPT as u64 * 60 {
            logged += step(&mut w, i);
        }
        assert_eq!((w.list.len(), w.dropped), (KEPT, false));
        assert!(!w.describe(t0).contains('…'), "{}", w.describe(t0));
        for i in KEPT as u64 * 60 + 1..(KEPT as u64 + 5) * 60 {
            logged += step(&mut w, i);
        }
        assert_eq!(logged, KEPT + 5, "one a period, the first at the start");
        assert_eq!(w.first.as_ref().unwrap().at, Duration::ZERO);
        assert_eq!(w.list.len(), KEPT);
        assert_eq!(w.list.back().unwrap().at, Duration::from_secs((KEPT as u64 + 4) * 600));
        assert_eq!(w.last.as_ref().unwrap().at, Duration::from_secs(((KEPT as u64 + 5) * 60 - 1) * 10), "the control: the last taken is not the list's");
        let d = w.describe(t0);
        assert!(d.contains("0:00:00 0 -") && d.contains(" · … · "), "{d}");
    }
}
