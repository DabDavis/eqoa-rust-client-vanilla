//! A start that dies inside the graphics driver, natively (F3 crash
//! 20261025-180337: Windows, an Intel HD Graphics 630 on Vulkan, "attempting
//! to launch client and it closes itself"; the log's last line the window's
//! place, its next would have been the present mode): no panic is caught
//! there, the driver takes the program down. So a start writes a marker
//! (`gpu-start.txt` in the config directory, `gpu-start-<n>.txt` for window
//! n beside it) naming the graphics APIs it tries before it makes the
//! instance, and takes it away once the surface is configured; a marker
//! found at a start is a start that never got that far, and this one tries
//! the platform's other API (Windows: DirectX 12 after Vulkan; Linux:
//! OpenGL). An API that started where the first died is kept (`gpu.opt`), so
//! the player is not left with every other start dying. `WGPU_BACKEND` set by
//! hand wins over all of it. Each step goes in the log, so a report says
//! where a start stopped.

use render::wgpu::Backends;
use std::path::PathBuf;

/// What a start tries, and its marker.
pub struct Start {
    marker: Option<PathBuf>,
    /// the APIs this start tries; None: wgpu's own choice (`WGPU_BACKEND`,
    /// else every primary one)
    pub backends: Option<Backends>,
    /// kept as the one that works once this start gets through
    keep: bool,
}

/// The APIs named as the marker and `gpu.opt` keep them.
fn names(b: Backends) -> String {
    let mut v = Vec::new();
    for (f, n) in [(Backends::VULKAN, "vulkan"), (Backends::DX12, "dx12"), (Backends::METAL, "metal"), (Backends::GL, "gl")] {
        if b.contains(f) {
            v.push(n);
        }
    }
    v.join(" ")
}

fn parse(s: &str) -> Backends {
    let mut b = Backends::empty();
    for w in s.split_whitespace() {
        b |= match w {
            "vulkan" => Backends::VULKAN,
            "dx12" => Backends::DX12,
            "metal" => Backends::METAL,
            "gl" => Backends::GL,
            _ => Backends::empty(),
        };
    }
    b
}

/// The platform's other API after `died`: the one to try next, if any.
fn fallback(died: Backends) -> Option<Backends> {
    if cfg!(windows) && died.contains(Backends::VULKAN) {
        Some(Backends::DX12)
    } else if cfg!(target_os = "linux") && died.contains(Backends::VULKAN) {
        Some(Backends::GL)
    } else {
        None
    }
}

/// What to try, given the marker a start before left (`died`) and the API a
/// start like this kept (`kept`): a death falls back once, a fallback that
/// died too goes back to wgpu's own choice.
fn choose(died: Option<Backends>, kept: Option<Backends>) -> (Option<Backends>, bool) {
    match died {
        Some(d) => match fallback(d) {
            Some(f) => (Some(f), true),
            None => (None, false),
        },
        None => (kept.filter(|k| !k.is_empty()), false),
    }
}

impl Start {
    /// Before the instance is made: the marker read, the choice made and
    /// logged, the marker written.
    pub fn begin() -> Start {
        if std::env::var_os("WGPU_BACKEND").is_some() {
            eprintln!("graphics: WGPU_BACKEND chooses");
            return Start { marker: None, backends: None, keep: false };
        }
        let dir = crate::settings::config_dir();
        // a window's own, named as its log is (`report`: window 1's plain)
        let n = crate::multibox::place() + 1;
        let name = if n == 1 { "gpu-start.txt".to_string() } else { format!("gpu-start-{n}.txt") };
        let marker = dir.as_ref().map(|d| d.join(name));
        let kept_path = dir.as_ref().map(|d| d.join("gpu.opt"));
        let died = marker.as_ref().and_then(|m| std::fs::read_to_string(m).ok()).map(|s| parse(&s));
        let kept = kept_path.as_ref().and_then(|k| std::fs::read_to_string(k).ok()).map(|s| parse(&s));
        let (backends, keep) = choose(died, kept);
        match (died, backends) {
            (Some(d), Some(b)) => eprintln!("graphics: the last start stopped while starting {}: trying {}", names(d), names(b)),
            (Some(d), None) => eprintln!("graphics: the last start stopped while starting {}: trying the usual way again", names(d)),
            (None, Some(b)) => eprintln!("graphics: {} (kept in gpu.opt: another API died here before)", names(b)),
            (None, None) => {}
        }
        if died.is_some() && backends.is_none() {
            // what was kept died too: forget it
            if let Some(k) = &kept_path {
                let _ = std::fs::remove_file(k);
            }
        }
        let trying = backends.unwrap_or(Backends::PRIMARY);
        if let Some(m) = &marker {
            let _ = std::fs::write(m, names(trying));
        }
        Start { marker, backends, keep }
    }

    /// The surface is configured: the marker gone, and an API that started
    /// where another died kept.
    pub fn done(&self) {
        if let Some(m) = &self.marker {
            let _ = std::fs::remove_file(m);
        }
        if self.keep
            && let (Some(b), Some(d)) = (self.backends, crate::settings::config_dir())
        {
            let _ = std::fs::write(d.join("gpu.opt"), names(b));
            eprintln!("graphics: {} works here: kept for the next starts (gpu.opt)", names(b));
        }
    }
}

/// A step of a start, in the log.
pub fn step(what: &str) {
    eprintln!("graphics: {what}");
}

/// Whether the adapter named `name` draws without the graphics card: a CPU
/// device by the driver's word (`cpu`), or one named as a software renderer
/// (Mesa's llvmpipe, lavapipe and softpipe, SwiftShader, a "Software
/// Renderer", Windows' WARP: "Microsoft Basic Render Driver"). The page's
/// list is the same (`web/main.js`'s SOFTWARE, F3 20261027-003944).
pub fn software(name: &str, cpu: bool) -> bool {
    let n = name.to_ascii_lowercase();
    cpu || ["llvmpipe", "lavapipe", "softpipe", "swiftshader", "software", "basic render driver"].iter().any(|s| n.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_that_died_on_vulkan_tries_the_other_api() {
        let both = Backends::VULKAN | Backends::DX12;
        if cfg!(windows) {
            assert_eq!(choose(Some(both), None), (Some(Backends::DX12), true));
            // DirectX 12 died too: wgpu's own choice again
            assert_eq!(choose(Some(Backends::DX12), Some(Backends::DX12)), (None, false));
        } else if cfg!(target_os = "linux") {
            assert_eq!(choose(Some(Backends::VULKAN), None), (Some(Backends::GL), true));
            assert_eq!(choose(Some(Backends::GL), Some(Backends::GL)), (None, false));
        }
        // kept from a start before: used, not kept again
        assert_eq!(choose(None, Some(Backends::DX12)), (Some(Backends::DX12), false));
        // the control: nothing died and nothing kept, wgpu chooses
        assert_eq!(choose(None, None), (None, false));
        // a Mac has no other API
        assert_eq!(fallback(Backends::METAL), None);
    }

    /// The software renderers are told by name or by the driver's device
    /// type; the control: graphics cards are not.
    #[test]
    fn a_software_adapter_is_told_apart() {
        for n in ["llvmpipe (LLVM 15.0.7, 256 bits)", "Microsoft Basic Render Driver", "SwiftShader Device (Subzero)", "Apple Software Renderer"] {
            assert!(software(n, false), "{n}");
        }
        assert!(software("Some Adapter", true), "a CPU device by its type");
        for n in ["AMD Radeon RX 6600 (RADV NAVI23)", "NVIDIA GeForce RTX 3060", "Intel(R) UHD Graphics 630", "Apple M1"] {
            assert!(!software(n, false), "the control: {n}");
        }
    }

    #[test]
    fn the_names_read_back() {
        for b in [Backends::VULKAN, Backends::DX12, Backends::GL, Backends::VULKAN | Backends::DX12, Backends::PRIMARY] {
            assert_eq!(parse(&names(b)) & (Backends::VULKAN | Backends::DX12 | Backends::METAL | Backends::GL), b & (Backends::VULKAN | Backends::DX12 | Backends::METAL | Backends::GL));
        }
        assert_eq!(parse("nonsense"), Backends::empty());
    }
}
