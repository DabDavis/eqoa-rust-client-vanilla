//! The patcher against a patch port of the test's own (`Port`) and a folder
//! in memory (`Mem`), each rule with its control.

use super::image::{Image, Images, crc32};
use super::*;
use std::collections::HashMap;

fn gz(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

/// A patch port: each path's reply, the requests it was sent.
#[derive(Default)]
struct Port {
    replies: HashMap<String, Vec<u8>>,
    requests: Vec<String>,
    down: bool,
}

impl Port {
    fn serve(&mut self, path: &str, body: &[u8]) {
        let mut r = format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\nContent-Type: application/x-gzip\r\n\r\n", body.len()).into_bytes();
        r.extend_from_slice(body);
        self.replies.insert(path.to_string(), r);
    }
}

impl Wire for Port {
    fn exchange(&mut self, host: &str, port: u16, request: &[u8], piece: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), http::Error> {
        if self.down {
            return Err(http::Error::new(5, format!("Could not connect to {host}")));
        }
        let req = String::from_utf8_lossy(request).to_string();
        let path = req.split(' ').nth(1).unwrap_or("").to_string();
        self.requests.push(format!("{host}:{port}{path}"));
        let reply = self.replies.get(&path).cloned().unwrap_or_else(|| b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec());
        // in pieces, as a socket hands them over
        for c in reply.chunks(700) {
            if piece(c) {
                break;
            }
        }
        Ok(())
    }
}

/// The server's folder, in memory.
#[derive(Default, Clone)]
struct Mem {
    files: HashMap<String, Vec<u8>>,
    record: Option<crate::servers::Record>,
    writes: usize,
}

impl Folder for Mem {
    fn read(&mut self, progress: &mut dyn FnMut(f32)) -> Result<Images, String> {
        let mut names: Vec<&String> = self.files.keys().collect();
        names.sort();
        let mut im = Images::default();
        for n in names {
            let d = self.files[n].clone();
            im.list.insert(0, Image { name: n.clone(), crc: crc32(&d), data: d, dirty: false, deleted: false });
        }
        progress(1.0);
        Ok(im)
    }
    fn write(&mut self, images: &Images, progress: &mut dyn FnMut(f32)) -> Result<bool, String> {
        progress(1.0);
        if !images.changed() {
            return Ok(false);
        }
        self.writes += 1;
        for im in &images.list {
            if im.dirty && !im.deleted {
                self.files.insert(im.name.clone(), im.data.clone());
            }
            if im.deleted {
                self.files.remove(&im.name);
            }
        }
        Ok(true)
    }
    fn record(&mut self, r: &crate::servers::Record) {
        self.record = Some(r.clone());
    }
}

/// A file the manifest names: its card name, its bytes, its served name.
struct F<'a> {
    local: &'a str,
    data: &'a [u8],
    served: &'a str,
    extra: &'a str,
}

/// The manifest as go-eqoa-server and Sandstorm write it: one
/// Distribution, files at the port's root, the release notes beside it.
fn manifest(use_crc: bool, files: &[F]) -> String {
    let mut s = format!("<VerantPatcher Version=\"1\"><Product Version=\"7\"><Distribution Name=\"Spire\"{} >", if use_crc { " UseCRC=\"True\"" } else { "" });
    for f in files {
        let z = gz(f.data);
        s += &format!(
            "<File LocalName=\"{}\" RemotePath=\"\" Server=\"192.0.2.1\" Name=\"{}\" CRC=\"{}\" TotalSize=\"{}\" DownloadSize=\"{}\"{}></File>",
            f.local,
            f.served,
            crc32(f.data),
            f.data.len(),
            z.len(),
            f.extra
        );
    }
    s += "</Distribution><Changes Server=\"192.0.2.1\" Port=\"7000\" Version=\"7\" RemotePath=\"patch/eqoa/Changes.txt\"></Changes></Product></VerantPatcher>";
    s
}

fn serve(port: &mut Port, man: &str, files: &[F]) {
    port.serve("/patch/eqoa/eqoa-frontierslive-update.xml.gz", &gz(man.as_bytes()));
    port.serve("/patch/eqoa/Changes.txt", b"New this week: robes.");
    for f in files {
        port.serve(&format!("/{}", f.served), &gz(f.data));
    }
}

fn cfg() -> Config {
    Config { host: "192.0.2.1".into(), port: PORT, manifest: MANIFEST.into(), frame_ms: 0 }
}

fn go(port: &mut Port, mem: &mut Mem) -> Status {
    let st: Shared = Default::default();
    run(&cfg(), port, mem, &st);
    let s = st.lock().unwrap().clone();
    assert!(s.done);
    s
}

const S2: &[u8] = b"S2 bytes: the server's own patch, any bytes at all";
const WORDS: &[u8] = b"line one\r\nline two\r\n";

fn two() -> [F<'static>; 2] {
    [F { local: "S2", data: S2, served: "s2-1.gz", extra: "" }, F { local: "WORDS.TXT", data: WORDS, served: "words-1.gz", extra: "" }]
}

/// A first patch fetches every file into an empty folder, with the PS2's
/// requests, and writes the folder and its record once; a second has
/// nothing to do and writes nothing.
#[test]
fn a_patch_fills_the_folder_then_has_nothing_to_do() {
    let files = two();
    let mut port = Port::default();
    serve(&mut port, &manifest(true, &files), &files);
    let mut mem = Mem::default();
    let s = go(&mut port, &mut mem);
    assert_eq!(s.state, state::DONE, "{:?}", s.err);
    assert!(s.changed);
    assert_eq!(s.notes, "New this week: robes.");
    assert_eq!(mem.files.get("S2").map(Vec::as_slice), Some(S2));
    assert_eq!(mem.files.get("WORDS.TXT").map(Vec::as_slice), Some(WORDS));
    assert_eq!(
        port.requests,
        [
            "192.0.2.1:7000/patch/eqoa/eqoa-frontierslive-update.xml.gz",
            // the product's notes, then the distribution's walk over the same <Changes>
            "192.0.2.1:7000/patch/eqoa/Changes.txt",
            "192.0.2.1:7000/patch/eqoa/Changes.txt",
            "192.0.2.1:7000/s2-1.gz",
            "192.0.2.1:7000/words-1.gz",
        ]
    );
    let r = mem.record.clone().unwrap();
    assert_eq!(r.files.iter().map(|f| (f.name.as_str(), f.crc)).collect::<Vec<_>>(), [("S2", crc32(S2)), ("WORDS.TXT", crc32(WORDS))]);
    port.requests.clear();
    let s = go(&mut port, &mut mem);
    assert_eq!((s.state, s.changed), (state::DONE, false));
    assert_eq!(mem.writes, 1, "the second patch writes nothing");
    assert!(!port.requests.iter().any(|r| r.ends_with(".gz") && !r.contains("update.xml")), "no file fetched: {:?}", port.requests);
}

/// A file is judged by SIZE unless the distribution says UseCRC: a
/// same-size change is fetched only under UseCRC (the control: without it,
/// it is not).
#[test]
fn a_same_size_change_needs_use_crc() {
    let old: &[u8] = b"WORDS v1 and some";
    let new: &[u8] = b"WORDS v2 and some";
    for (use_crc, fetched) in [(true, true), (false, false)] {
        let files = [F { local: "WORDS.TXT", data: new, served: "words-2.gz", extra: "" }];
        let mut port = Port::default();
        serve(&mut port, &manifest(use_crc, &files), &files);
        let mut mem = Mem::default();
        mem.files.insert("WORDS.TXT".into(), old.to_vec());
        let s = go(&mut port, &mut mem);
        assert_eq!(s.state, state::DONE);
        assert_eq!(mem.files["WORDS.TXT"] == new, fetched, "UseCRC {use_crc}");
    }
}

/// The CRC is checked after every download: a file that does not carry the
/// manifest's fails the patch, and a failed patch writes nothing.
#[test]
fn a_bad_crc_fails_and_writes_nothing() {
    let files = two();
    let mut port = Port::default();
    let man = manifest(true, &files).replace(&format!("CRC=\"{}\"", crc32(WORDS)), "CRC=\"12345\"");
    serve(&mut port, &man, &files);
    let mut mem = Mem::default();
    mem.files.insert("SS".into(), b"kept".to_vec());
    let before = mem.files.clone();
    let s = go(&mut port, &mut mem);
    assert_eq!(s.state, state::FAILED);
    assert!(!s.unreachable);
    assert_eq!(mem.files, before, "nothing written");
    assert!(mem.record.is_none());
    // the control: the right CRC passes
    let mut port = Port::default();
    serve(&mut port, &manifest(true, &files), &files);
    assert_eq!(go(&mut port, &mut mem).state, state::DONE);
}

/// Every file must be gzipped: one served plain inflates to nothing, whose
/// CRC fails.
#[test]
fn an_uncompressed_file_fails() {
    let files = two();
    let mut port = Port::default();
    serve(&mut port, &manifest(true, &files), &files);
    port.serve("/words-1.gz", WORDS);
    assert_eq!(go(&mut port, &mut Mem::default()).state, state::FAILED);
}

/// Downloads first, then Deletes: a LocalName that is both an entry and a
/// Delete ends up gone; a Delete alone removes the old file.
#[test]
fn deletes_come_after_downloads() {
    let files = [F { local: "S2", data: S2, served: "s2-1.gz", extra: "" }];
    let mut port = Port::default();
    let man = manifest(true, &files).replace("</Distribution>", "<File LocalName=\"old.txt\" Delete=\"True\"></File><File LocalName=\"S2\" Delete=\"True\"></File></Distribution>");
    serve(&mut port, &man, &files);
    let mut mem = Mem::default();
    mem.files.insert("old.txt".into(), b"stale".to_vec());
    let s = go(&mut port, &mut mem);
    assert_eq!(s.state, state::DONE, "{:?}", s.err);
    assert!(!mem.files.contains_key("old.txt"), "the Delete removed it");
    assert!(!mem.files.contains_key("S2"), "fetched, then deleted");
    // the control: without the Delete entries both stay
    let mut port = Port::default();
    serve(&mut port, &manifest(true, &files), &files);
    let mut mem = Mem::default();
    mem.files.insert("old.txt".into(), b"stale".to_vec());
    go(&mut port, &mut mem);
    assert!(mem.files.contains_key("old.txt") && mem.files.contains_key("S2"));
}

/// `Restart="True"` finishes the patch and cancels every file after it, so
/// the patch fails (a PS2 reboots); the control: no Restart, both fetched.
#[test]
fn a_restart_file_cancels_the_rest() {
    let files = [F { local: "S2", data: S2, served: "s2-1.gz", extra: " Restart=\"True\"" }, F { local: "WORDS.TXT", data: WORDS, served: "words-1.gz", extra: "" }];
    let mut port = Port::default();
    serve(&mut port, &manifest(true, &files), &files);
    let s = go(&mut port, &mut Mem::default());
    assert_eq!(s.state, state::FAILED);
    assert!(!port.requests.iter().any(|r| r.ends_with("words-1.gz")), "nothing after the restart: {:?}", port.requests);
    let files = two();
    let mut port = Port::default();
    serve(&mut port, &manifest(true, &files), &files);
    assert_eq!(go(&mut port, &mut Mem::default()).state, state::DONE);
}

/// A server whose patch port never answers is not a failed patch here (its
/// folder is played as it stands); one that answers without the manifest
/// is (the control).
#[test]
fn an_unreachable_server_is_not_a_failure() {
    let mut port = Port { down: true, ..Default::default() };
    let mut mem = Mem::default();
    mem.files.insert("S2".into(), S2.to_vec());
    let s = go(&mut port, &mut mem);
    assert!(s.unreachable);
    assert_ne!(s.state, state::FAILED);
    assert_eq!(mem.files.len(), 1, "the folder untouched");
    let mut port = Port::default();
    let s = go(&mut port, &mut Mem::default());
    assert!(!s.unreachable);
    assert_eq!(s.state, state::FAILED, "a 404 manifest fails");
}

/// The bar ends full, and the states run as RunPatcher sets them.
#[test]
fn the_bar_ends_full() {
    let files = two();
    let mut port = Port::default();
    serve(&mut port, &manifest(true, &files), &files);
    let s = go(&mut port, &mut Mem::default());
    assert_eq!(s.progress, 1.0);
}

/// What a page's patch port is (`patcher::web`): each path's body, fetched
/// over the server's web server.
fn site(files: &[F]) -> HashMap<String, Vec<u8>> {
    let mut site = HashMap::new();
    site.insert("patch/eqoa/eqoa-frontierslive-update.xml.gz".to_string(), gz(manifest(true, files).as_bytes()));
    site.insert("patch/eqoa/Changes.txt".to_string(), b"New this week: robes.".to_vec());
    for f in files {
        site.insert(f.served.to_string(), gz(f.data));
    }
    site
}

/// `replay::play` until a play asks for nothing new, each play into a copy
/// of the folder and only the last one kept, as `patcher::web` plays it:
/// the last status and every play's missing paths.
fn replayed(site: &HashMap<String, Vec<u8>>, mem: &mut Mem) -> (Status, Vec<Vec<String>>) {
    let mut got = replay::Fetched::new();
    let mut plays = Vec::new();
    loop {
        let mut trial = mem.clone();
        let (s, missing) = replay::play(&cfg(), &got, &mut trial);
        plays.push(missing.clone());
        if missing.is_empty() {
            if s.state == state::DONE {
                *mem = trial;
            }
            return (s, plays);
        }
        assert!(plays.len() < 8, "it keeps asking: {plays:?}");
        for p in missing {
            let r = site.get(&p).map(|b| (200, b.clone())).ok_or_else(|| format!("{p}: unreachable"));
            got.insert(p, r);
        }
    }
}

/// In a page `run` is played over what was fetched: the first play asks
/// for the manifest alone and fails (the control: a play that stood then
/// would have patched nothing), a later one for the files, and the last
/// patches the folder as a patch port would have, the release notes and
/// the record with it; then a second patch fetches no file.
#[test]
fn a_page_plays_the_patch_until_it_asks_for_nothing() {
    let files = two();
    let site = site(&files);
    let mut mem = Mem::default();
    let first = replay::play(&cfg(), &replay::Fetched::new(), &mut mem.clone());
    assert_ne!(first.0.state, state::DONE, "the control: the first play cannot patch");
    assert_eq!(first.1, ["patch/eqoa/eqoa-frontierslive-update.xml.gz"]);
    let (s, plays) = replayed(&site, &mut mem);
    assert_eq!(s.state, state::DONE, "{:?}", s.err);
    assert!(s.changed);
    assert_eq!(s.notes, "New this week: robes.");
    assert!(plays.iter().flatten().any(|p| p == "s2-1.gz") && plays.iter().flatten().any(|p| p == "words-1.gz"), "{plays:?}");
    assert_eq!(mem.files.get("S2").map(Vec::as_slice), Some(S2));
    assert_eq!(mem.files.get("WORDS.TXT").map(Vec::as_slice), Some(WORDS));
    assert_eq!(mem.writes, 1, "only the last play wrote the folder");
    assert!(mem.record.is_some());
    let (s, plays) = replayed(&site, &mut mem);
    assert_eq!((s.state, s.changed), (state::DONE, false));
    assert!(!plays.iter().flatten().any(|p| p.ends_with(".gz") && !p.contains("update.xml")), "no file fetched: {plays:?}");
}

/// A file the web server does not have fails the patch and leaves the
/// folder as it was (the control: with the file there, it patches).
#[test]
fn a_page_patch_missing_a_file_leaves_the_folder() {
    let files = two();
    let mut gone = site(&files);
    gone.remove("words-1.gz");
    let mut mem = Mem::default();
    let (s, _) = replayed(&gone, &mut mem);
    assert_ne!(s.state, state::DONE);
    assert!(mem.files.is_empty() && mem.writes == 0, "nothing written");
    let (s, _) = replayed(&site(&files), &mut mem);
    assert_eq!(s.state, state::DONE, "{:?}", s.err);
}
