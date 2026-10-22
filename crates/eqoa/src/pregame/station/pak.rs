//! STATION.PAK, as `VIFlatFileSystem` holds it (viflatfile.cpp): a run of
//! big-endian records, a name's length, the name, the data's length, the
//! data; each record's path ("data/ui/login.xml") names its directories,
//! made as they are first met. A record running past the end stops the load.
//! The renderer reads the disc's copy (`/station.pak`): the card's, which a
//! server's patcher writes, is not a thing here.

/// A file, or a directory of them.
#[derive(Clone, Debug)]
enum Entry {
    File { name: String, data: Vec<u8> },
    Dir { name: String, entries: Vec<Entry> },
}

impl Entry {
    fn name(&self) -> &str {
        match self {
            Entry::File { name, .. } | Entry::Dir { name, .. } => name,
        }
    }
}

/// `VIFlatFileSystem`: the root directory, "/".
#[derive(Clone, Debug, Default)]
pub struct Pak {
    root: Vec<Entry>,
}

/// `strtok(copy, "/")`'s tokens: the path's elements, empty ones skipped.
fn tokens(path: &str) -> Vec<&str> {
    path.split('/').filter(|t| !t.is_empty()).collect()
}

impl Pak {
    /// `Load(data, size)`.
    pub fn load(data: &[u8]) -> Pak {
        let mut pak = Pak::default();
        let mut pos = 0usize;
        let be = |p: usize| data.get(p..p + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()) as usize);
        while pos < data.len() {
            let Some(name_len) = be(pos) else { break };
            pos += 4;
            if pos > data.len() {
                break;
            }
            // memcpy'd into a zeroed buffer: the name is the bytes up to a NUL
            let Some(raw) = data.get(pos..pos + name_len) else { break };
            let name: String = raw.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
            pos += name_len;
            let Some(len) = be(pos) else { break };
            pos += 4;
            let Some(body) = data.get(pos..pos + len) else { break };
            pos += len;
            add_file(&mut pak.root, &name, body);
        }
        pak
    }

    /// `Find(path)`: a leading "/" is the root itself; each element is
    /// matched without regard to case; a file matching an element is the
    /// answer even when the path goes on, as the game's walk stops there.
    pub fn find(&self, path: &str) -> Option<&[u8]> {
        let path = path.strip_prefix('/').unwrap_or(path);
        let mut entries = &self.root;
        for t in tokens(path) {
            match entries.iter().find(|e| e.name().eq_ignore_ascii_case(t))? {
                Entry::File { data, .. } => return Some(data),
                Entry::Dir { entries: e, .. } => entries = e,
            }
        }
        None
    }

    /// Every file's path, in the order the pak made them (for tests).
    #[cfg(test)]
    pub fn paths(&self) -> Vec<String> {
        fn walk(e: &[Entry], at: &str, out: &mut Vec<String>) {
            for x in e {
                match x {
                    Entry::File { name, .. } => out.push(format!("{at}{name}")),
                    Entry::Dir { name, entries } => walk(entries, &format!("{at}{name}/"), out),
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, "", &mut out);
        out
    }
}

/// `VIFlatFileDirectory::AddFile`: the first element names a file here (an
/// existing file of that name keeps its data), or a directory to go on in,
/// made when the path has a "/" in it and there is none. `Set` refuses an
/// empty file: it is made, nameless, and never found.
fn add_file(entries: &mut Vec<Entry>, path: &str, data: &[u8]) {
    let toks = tokens(path);
    let Some(&tok) = toks.first() else { return };
    if let Some(i) = entries.iter().position(|e| e.name().eq_ignore_ascii_case(tok)) {
        if let Entry::Dir { entries: sub, .. } = &mut entries[i] {
            add_file(sub, &toks[1..].join("/"), data);
        }
        return;
    }
    if !path.contains('/') {
        let name = if data.is_empty() { String::new() } else { tok.to_string() };
        entries.push(Entry::File { name, data: data.to_vec() });
    } else {
        let mut sub = Vec::new();
        add_file(&mut sub, &toks[1..].join("/"), data);
        entries.push(Entry::Dir { name: tok.to_string(), entries: sub });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, data: &[u8]) -> Vec<u8> {
        let mut b = (name.len() as u32).to_be_bytes().to_vec();
        b.extend(name.as_bytes());
        b.extend((data.len() as u32).to_be_bytes());
        b.extend(data);
        b
    }

    #[test]
    fn records_make_a_tree_found_without_regard_to_case() {
        let mut b = record("data/ui/login.xml", b"<a/>");
        b.extend(record("stationdata/servers/stationLiveHosts.txt", b"1.2.3.4:9735"));
        b.extend(record("data/ui/LOGIN.xml", b"second"));
        b.extend(record("empty.txt", b""));
        let p = Pak::load(&b);
        assert_eq!(p.find("data/ui/Login.XML"), Some(&b"<a/>"[..]), "the first of two names that differ in case");
        assert_eq!(p.find("/stationdata/servers/stationlivehosts.txt"), Some(&b"1.2.3.4:9735"[..]));
        assert_eq!(p.find("stationdata//servers/stationLiveHosts.txt"), Some(&b"1.2.3.4:9735"[..]), "strtok skips an empty element");
        assert_eq!(p.find("/stationdata/servers/stationAltHosts.txt"), None);
        assert_eq!(p.find("empty.txt"), None, "an empty file is never found");
        assert_eq!(p.find("data/ui"), None, "a directory is not a file");
        // a record cut short stops the load: what came before stays
        let mut cut = record("a.txt", b"one");
        cut.extend(&record("b.txt", b"two")[..10]);
        let p = Pak::load(&cut);
        assert_eq!((p.find("a.txt"), p.find("b.txt")), (Some(&b"one"[..]), None));
    }

    /// The disc's STATION.PAK: its 25 files, every page found under the
    /// names the renderer asks for (`Conn.xml` is the pak's `conn.xml`).
    #[test]
    fn the_discs_pak() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let b = iso::Iso::open(&iso).unwrap().read("STATION.PAK").unwrap();
        let p = Pak::load(&b);
        assert_eq!(p.paths().len(), 25);
        for page in ["data/ui/login.xml", "data/ui/Conn.xml", "data/ui/IDCreate.xml", "data/ui/IDCreat2.xml", "data/ui/AcctMgmt.xml", "data/ui/PassChng.xml", "data/ui/FrgtPass.xml"] {
            assert!(p.find(page).is_some_and(|d| d.starts_with(b"<PS2GUIPage")), "{page}");
        }
        assert_eq!(p.find("/stationdata/servers/stationLiveHosts.txt"), Some(&b"64.37.156.200:9735\r\n"[..]));
        assert!(p.find("/stationdata/config/options.cfg").is_some_and(|d| d.starts_with(b"NewsURL:http://patch.everquestonlineadventures.com:7000/")));
        assert_eq!(p.find("data/ui/UpGlobalCollect.xml"), None, "the page UpdateBilling names that the pak lacks");
    }
}
