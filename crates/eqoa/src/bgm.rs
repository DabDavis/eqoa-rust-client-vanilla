//! The music CDSTREAM streams, off a disc that can only be read
//! asynchronously (a page's, `web::bgm`): 107 BGM files of 7 to 9 MB, which
//! the driver reads a sector run at a time as it plays, filling what it cannot
//! read with silence. So a file is read whole before the device may start it
//! (`snd::Disc::ready`, which the device asks through `Link::bgm_ready`; what
//! is wanted is read by the page, `take_wanted`, and handed back, `insert`),
//! and the driver's reads are served from that copy. The three files used
//! last are kept.
//!
//! Natively the same, the files read on a thread of their own
//! (`read_on_a_thread`): the driver runs under its lock (`audio::Shared`),
//! which the output's callback takes to mix and the game's thread takes every
//! frame, so a read of the disc there held both for as long as the disc took
//! (F3 20261029-025532: two windows freezing at once, a callback once mixing
//! for 8,363 ms). Now the driver's reads are copies, and a slow disc holds
//! back only the music's start.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Natively, how often the thread looks for files asked for: a BGM starts
/// this much later at most, past its read.
#[cfg(not(target_arch = "wasm32"))]
const LOOK_MS: u64 = 10;

const KEEP: usize = 3;

#[derive(Default)]
struct Cache {
    /// every file on the disc by CDSTREAM's name (`\BGM\THEME.BGM;1`): its
    /// first sector and size
    names: HashMap<String, (u32, u32)>,
    /// files read whole, by first sector, the one used last at the end
    files: Vec<(u32, Vec<u8>)>,
    /// asked for and not handed out yet, being read, and not readable
    wanted: Vec<(u32, u32)>,
    reading: Vec<u32>,
    failed: Vec<u32>,
    /// a read of sectors no file held (said once)
    missed: bool,
}

#[derive(Clone)]
pub struct Bgm(Arc<Mutex<Cache>>);

impl Bgm {
    pub fn new(entries: &[iso::Entry]) -> Bgm {
        let names = entries.iter().map(|e| (snd::driver::cd_name(&e.path), (e.lba, e.size))).collect();
        Bgm(Arc::new(Mutex::new(Cache { names, ..Default::default() })))
    }

    /// The files asked for since the last call (first sector, size), now
    /// being read.
    pub fn take_wanted(&self) -> Vec<(u32, u32)> {
        let mut c = self.0.lock().unwrap();
        let w = std::mem::take(&mut c.wanted);
        c.reading.extend(w.iter().map(|f| f.0));
        w
    }

    /// A file read whole.
    pub fn insert(&self, lsn: u32, bytes: Vec<u8>) {
        let mut c = self.0.lock().unwrap();
        c.reading.retain(|&l| l != lsn);
        c.files.push((lsn, bytes));
        while c.files.len() > KEEP {
            c.files.remove(0);
        }
    }

    /// A file that could not be read: it plays silent, as a failed read does.
    pub fn fail(&self, lsn: u32) {
        let mut c = self.0.lock().unwrap();
        c.reading.retain(|&l| l != lsn);
        c.failed.push(lsn);
    }

    /// The file's name, for what is said about it.
    pub fn name(&self, lsn: u32) -> String {
        self.0.lock().unwrap().names.iter().find(|(_, v)| v.0 == lsn).map(|(n, _)| n.clone()).unwrap_or_default()
    }
}

/// Natively: the files the device asks for (`ready`), each read whole by
/// `read` (the image's byte offset and the length) on a thread of its own,
/// timed as the disc's reads are (`stall::disc`), said in the log with how
/// long it took, as a page says it, and handed to the cache; one that cannot
/// be read plays silent, as a failed read does. The thread ends once the
/// cache has no other holder (the driver's disc gone). False when no thread
/// could be made (the caller reads the disc as before).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_on_a_thread(bgm: &Bgm, mut read: impl FnMut(u64, usize) -> std::io::Result<Vec<u8>> + Send + 'static) -> bool {
    let b = bgm.clone();
    let spawned = std::thread::Builder::new().name("music".into()).spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(LOOK_MS));
            if Arc::strong_count(&b.0) == 1 {
                return;
            }
            for (lsn, size) in b.take_wanted() {
                let t = web_time::Instant::now();
                match crate::stall::disc::timed(crate::stall::disc::Read::Music, || read(lsn as u64 * iso::SECTOR, size as usize)) {
                    Ok(bytes) => {
                        crate::say(&format!("music: {} read ({:.1} MB, {} ms)", b.name(lsn), bytes.len() as f64 / 1048576.0, t.elapsed().as_millis()));
                        b.insert(lsn, bytes);
                    }
                    Err(e) => {
                        crate::say(&format!("music: {} could not be read ({e})", b.name(lsn)));
                        b.fail(lsn);
                    }
                }
            }
        }
    });
    match spawned {
        Ok(_) => true,
        Err(e) => {
            crate::say(&format!("music: no thread to read it on ({e}): read by the driver as it plays"));
            false
        }
    }
}

/// `read_on_a_thread`'s reads off the disc image at `image`, through a
/// handle of its own.
#[cfg(not(target_arch = "wasm32"))]
pub fn image_reader(image: std::path::PathBuf) -> impl FnMut(u64, usize) -> std::io::Result<Vec<u8>> + Send + 'static {
    let mut file: Option<std::fs::File> = None;
    move |off, len| {
        use std::io::{Read, Seek, SeekFrom};
        let f = match file.as_mut() {
            Some(f) => f,
            None => file.insert(std::fs::File::open(&image)?),
        };
        f.seek(SeekFrom::Start(off))?;
        let mut b = vec![0u8; len];
        f.read_exact(&mut b)?;
        Ok(b)
    }
}

impl snd::Disc for Bgm {
    fn search(&mut self, name: &str) -> Option<(u32, u32)> {
        self.0.lock().unwrap().names.get(name).copied()
    }

    fn read(&mut self, lsn: u32, sectors: u32, buf: &mut [u8]) -> bool {
        let mut c = self.0.lock().unwrap();
        let held = |f: &(u32, Vec<u8>)| lsn >= f.0 && ((lsn - f.0) as usize) * 0x800 < f.1.len();
        let Some(k) = c.files.iter().position(held) else {
            if !c.missed {
                c.missed = true;
                crate::say(&format!("music: sector {lsn} read before its file was"));
            }
            return false;
        };
        let f = c.files.remove(k);
        let at = (lsn - f.0) as usize * 0x800;
        let n = (sectors as usize * 0x800).min(buf.len());
        let have = (f.1.len() - at).min(n);
        buf[..have].copy_from_slice(&f.1[at..at + have]);
        buf[have..n].fill(0);
        c.files.push(f);
        true
    }

    /// A file not on the disc is ready: its open fails, as on a console. One
    /// that could not be read is too.
    fn ready(&mut self, name: &str) -> bool {
        let mut c = self.0.lock().unwrap();
        let Some(&(lsn, size)) = c.names.get(name) else { return true };
        if c.files.iter().any(|f| f.0 == lsn) || c.failed.contains(&lsn) {
            return true;
        }
        if !c.reading.contains(&lsn) && !c.wanted.iter().any(|w| w.0 == lsn) {
            c.wanted.push((lsn, size));
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snd::Disc;
    use snd::device::SoundDevice;
    use vi::fpu::Ee;

    /// A BGM streamed by the driver through `Bgm`, its file handed over
    /// frames after it was asked for (as a page's read finishes), against
    /// the same through the disc (`IsoDisc`): silent until the file is
    /// there, never a read before it, then the same frames for 20 seconds.
    /// SKIPs without the disc. A copy of the file one byte off must differ.
    #[test]
    fn the_page_streams_what_the_disc_does() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let file = disc.read("BGM/BATTLE01.BGM").expect("BATTLE01.BGM");
        let entries = disc.entries().to_vec();
        // (ms of output after the BGM starts) frames from the driver, with the
        // file handed over `late` frames after the device asked for it, and
        // `poke` changing one of its bytes
        let stream = |bgm: Option<(usize, Option<usize>)>| -> (Vec<i16>, bool) {
            let (audio, cache) = match bgm {
                None => (crate::audio::Audio::offline(Box::new(crate::audio::IsoDisc(iso::Iso::open(&iso_path).unwrap()))), None),
                Some(_) => {
                    let b = Bgm::new(&entries);
                    (crate::audio::Audio::offline(Box::new(b.clone())), Some(b))
                }
            };
            let mut link = audio.link();
            let mut dev = SoundDevice::<Ee>::new(1);
            assert!(dev.init(&mut link) >= 0);
            dev.play_bgm(0, "bgm\\battle01.bgm");
            let mut pcm = Vec::new();
            let mut asked = None;
            for frame in 0..60 * 30 {
                dev.process(&mut link);
                if let (Some(b), Some((late, poke))) = (&cache, bgm) {
                    for (lsn, _) in b.take_wanted() {
                        asked = Some((frame, lsn));
                    }
                    if let Some((at, lsn)) = asked
                        && frame == at + late
                    {
                        let mut bytes = file.clone();
                        if let Some(k) = poke {
                            bytes[k] ^= 1;
                        }
                        b.insert(lsn, bytes);
                    }
                }
                pcm.extend(audio.render(800));
            }
            let missed = cache.is_some_and(|b| b.0.lock().unwrap().missed);
            let start = pcm.iter().position(|&s| s != 0).expect("the BGM plays");
            (pcm[start..start + 2 * 48000 * 20].to_vec(), missed)
        };
        let (disc_pcm, _) = stream(None);
        let (page, missed) = stream(Some((7, None)));
        assert!(!missed, "a read came before the file");
        assert!(page == disc_pcm, "the page's stream differs from the disc's");
        let (poked, _) = stream(Some((7, Some(0x40000))));
        assert!(poked != disc_pcm, "a file one bit off must sound different");

        // what is not on the disc is ready (its open fails, as on a console)
        let mut b = Bgm::new(&entries);
        assert!(b.ready("\\BGM\\NOSUCH.BGM;1"));
        assert!(!b.ready("\\BGM\\BATTLE01.BGM;1"));
        assert_eq!(b.take_wanted().len(), 1);
        assert!(!b.ready("\\BGM\\BATTLE01.BGM;1"), "still being read");
        assert!(b.take_wanted().is_empty(), "asked for once");
        // a file that could not be read plays silent, as a failed read does
        let lsn = disc.find("BGM/BATTLE01.BGM").unwrap().lba;
        assert_eq!(b.name(lsn), "\\BGM\\BATTLE01.BGM;1");
        b.fail(lsn);
        assert!(b.ready("\\BGM\\BATTLE01.BGM;1"));
    }

    /// Natively the music's files are read on a thread of their own
    /// (`read_on_a_thread`): what the driver plays is the disc's, frame for
    /// frame from the BGM's start for 20 seconds, and a read of the disc
    /// that takes 400 ms holds none of the driver's calls (the device's
    /// frame and a mix, each run under the driver's lock as the game's
    /// thread and the output's callback run them) for even half of it. The
    /// control: the same slow read made by the driver itself, as natively
    /// before (`IsoDisc`), holds the call it comes in for all of it. SKIPs
    /// without the disc.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_native_reader_keeps_the_disc_off_the_drivers_lock() {
        use std::time::{Duration, Instant};
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let entries = disc.entries().to_vec();
        const SLOW: Duration = Duration::from_millis(400);
        /// The disc, its first read slow.
        struct Slow(crate::audio::IsoDisc, bool);
        impl snd::Disc for Slow {
            fn search(&mut self, name: &str) -> Option<(u32, u32)> {
                self.0.search(name)
            }
            fn read(&mut self, lsn: u32, sectors: u32, buf: &mut [u8]) -> bool {
                if !std::mem::replace(&mut self.1, true) {
                    std::thread::sleep(SLOW);
                }
                self.0.read(lsn, sectors, buf)
            }
        }
        // the 20 s from the BGM's start, and the longest of the driver's
        // calls (the device's frame and a mix of a frame's 800 samples)
        let stream = |disc: Box<dyn snd::Disc>| -> (Vec<i16>, Duration) {
            let audio = crate::audio::Audio::offline(disc);
            let mut link = audio.link();
            let mut dev = SoundDevice::<Ee>::new(1);
            assert!(dev.init(&mut link) >= 0);
            dev.play_bgm(0, "bgm\\battle01.bgm");
            let mut pcm: Vec<i16> = Vec::new();
            let mut longest = Duration::ZERO;
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut frames = 0;
            while frames < 30 * 45 && Instant::now() < deadline {
                let t = Instant::now();
                dev.process(&mut link);
                pcm.extend(audio.render(800));
                longest = longest.max(t.elapsed());
                // while silent, the frames at about the game's pace: the
                // thread's read is under way
                if pcm.iter().all(|&s| s == 0) {
                    std::thread::sleep(Duration::from_millis(2));
                } else {
                    frames += 1;
                }
            }
            let start = pcm.iter().position(|&s| s != 0).expect("the BGM plays");
            assert!(pcm.len() >= start + 2 * 48000 * 20, "20 s of it");
            (pcm[start..start + 2 * 48000 * 20].to_vec(), longest)
        };
        let (on_disc, _) = stream(Box::new(crate::audio::IsoDisc(disc)));
        let b = Bgm::new(&entries);
        let mut read = image_reader(iso_path.clone());
        let slow_read = move |off: u64, len: usize| {
            std::thread::sleep(SLOW);
            read(off, len)
        };
        assert!(read_on_a_thread(&b, slow_read));
        let (threaded, longest) = stream(Box::new(b.clone()));
        assert!(threaded == on_disc, "the thread's copy plays as the disc does");
        assert!(longest < SLOW / 2, "no call held by the read: the longest {longest:?}");
        assert!(!b.0.lock().unwrap().missed, "no read came before its file");
        let (_, held) = stream(Box::new(Slow(crate::audio::IsoDisc(iso::Iso::open(&iso_path).unwrap()), false)));
        assert!(held >= SLOW, "the control: the driver's own read holds its call ({held:?})");
    }

    /// Three files kept, the one used last kept longest.
    #[test]
    fn keeps_the_three_used_last() {
        let e = |k: u32| iso::Entry { path: format!("BGM/T{k}.BGM"), lba: 1000 * k, size: 0x800 };
        let mut b = Bgm::new(&[e(1), e(2), e(3), e(4)]);
        let mut buf = [0u8; 0x800];
        for k in 1..=3 {
            b.insert(1000 * k, vec![k as u8; 0x800]);
        }
        assert!(b.read(1000, 1, &mut buf) && buf[0] == 1, "file 1 used: now the last one used");
        b.insert(4000, vec![4; 0x800]);
        assert!(b.read(1000, 1, &mut buf), "file 1 kept");
        assert!(!b.read(2000, 1, &mut buf), "file 2, used least lately, gone");
        assert!(b.read(3000, 1, &mut buf) && b.read(4000, 1, &mut buf));
    }
}
