//! Each server's own folder (the user's word, 2026-10-23: "save the patches
//! per server in their own folder"): what that server's patch server put on
//! the card, flat, as the PS2's `BASLUS-20744EQONLINE` folder holds it (SS,
//! S2, ss-art.csf, WORDS.TXT, station.pak, eqahosts.txt, whatever its
//! manifest names), and `patch.json`, the record of the last patch that
//! finished (the server, its manifest, when, each file's CRC and size), which
//! a problem report names. A PS2 has one card folder and patches it over to
//! play on another server; here each server keeps its own, so changing
//! servers never takes another's files.
//!
//! The patcher (`patcher`) writes it; the game reads it: the server's words
//! (`words`), and its S2 and art.

use std::path::{Path, PathBuf};

/// The record's name in a server's folder (never a card file).
pub const RECORD: &str = "patch.json";

/// `<the config directory>/servers/<host>/`: the folder of the server at
/// `host` (an address or a name, as Network Setup keeps it; anything but
/// letters, digits, '.', '-' and '_' becomes '_'). Without a config
/// directory (the test commands) the platform's own `eqoa-rs` directory.
pub fn folder(host: &str) -> PathBuf {
    let safe: String = host.trim().chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    let safe = if safe.is_empty() || safe.chars().all(|c| c == '.') { "_".to_string() } else { safe };
    base().join("servers").join(safe)
}

#[cfg(not(target_arch = "wasm32"))]
fn base() -> PathBuf {
    crate::settings::config_dir().or_else(crate::settings::user_dir).unwrap_or_else(|| PathBuf::from("eqoa-rs"))
}

#[cfg(target_arch = "wasm32")]
fn base() -> PathBuf {
    PathBuf::from("eqoa-rs")
}

/// Whether a patch may be kept: natively with a config directory (the game,
/// `$EQOA_CONFIG`); the test commands and a page keep nothing.
pub fn keeps() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return crate::settings::config_dir().is_some();
    #[cfg(target_arch = "wasm32")]
    false
}

/// A file of the server's folder, if it has one.
pub fn file(host: &str, name: &str) -> Option<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    return std::fs::read(folder(host).join(name)).ok();
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (host, name);
        None
    }
}

/// The server's WORDS.TXT, when its patch put one there: the game's words
/// for that server's session (the disc's otherwise).
pub fn words(host: &str) -> Option<Vec<u8>> {
    file(host, "WORDS.TXT")
}

/// One file a finished patch left.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub name: String,
    pub crc: u32,
    pub size: u64,
}

/// `patch.json`: the last patch that finished.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub server: String,
    pub manifest: String,
    /// seconds since 1970 (UTC)
    pub when: u64,
    pub files: Vec<Kept>,
}

impl Record {
    /// The record as JSON.
    pub fn to_json(&self) -> String {
        let files: Vec<String> = self.files.iter().map(|f| format!("    {{\"name\": {}, \"crc\": {}, \"size\": {}}}", quote(&f.name), f.crc, f.size)).collect();
        format!(
            "{{\n  \"server\": {},\n  \"manifest\": {},\n  \"when\": {},\n  \"files\": [\n{}\n  ]\n}}\n",
            quote(&self.server),
            quote(&self.manifest),
            self.when,
            files.join(",\n")
        )
    }

    /// The record out of `to_json`'s text (its own fields only; None for
    /// anything else).
    pub fn from_json(s: &str) -> Option<Record> {
        let field = |k: &str| -> Option<String> {
            let at = s.find(&format!("\"{k}\":"))? + k.len() + 3;
            Some(s[at..].trim_start().to_string())
        };
        let string = |rest: &str| -> Option<String> { unquote(rest) };
        let number = |rest: &str| -> Option<u64> { rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok() };
        let server = string(&field("server")?)?;
        let manifest = string(&field("manifest")?)?;
        let when = number(&field("when")?)?;
        let mut files = Vec::new();
        let list = &s[s.find("\"files\":")?..];
        for part in list.split('{').skip(1) {
            let name = unquote(part.split("\"name\":").nth(1)?.trim_start())?;
            let crc = number(part.split("\"crc\":").nth(1)?.trim_start())? as u32;
            let size = number(part.split("\"size\":").nth(1)?.trim_start())?;
            files.push(Kept { name, crc, size });
        }
        Some(Record { server, manifest, when, files })
    }

    /// The report's line: the server's patch and when.
    pub fn describe(&self) -> String {
        let names: Vec<String> = self.files.iter().map(|f| format!("{} ({} bytes, CRC {})", f.name, f.size, f.crc)).collect();
        format!("patched {} UTC from {}: {}", stamp(self.when), self.manifest, names.join(", "))
    }
}

/// The record of the last patch from `host` that finished, if any.
pub fn record(host: &str) -> Option<Record> {
    Record::from_json(&String::from_utf8_lossy(&file(host, RECORD)?))
}

/// Writes the record beside the server's files.
pub fn write_record(dir: &Path, r: &Record) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let tmp = dir.join(format!("{RECORD}.part"));
        std::fs::write(&tmp, r.to_json()).and_then(|_| std::fs::rename(&tmp, dir.join(RECORD))).map_err(|e| format!("{}: {e}", dir.join(RECORD).display()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (dir, r);
        Ok(())
    }
}

fn quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn unquote(s: &str) -> Option<String> {
    let mut it = s.strip_prefix('"')?.chars();
    let mut o = String::new();
    loop {
        match it.next()? {
            '"' => return Some(o),
            '\\' => match it.next()? {
                'u' => {
                    let h: String = it.by_ref().take(4).collect();
                    o.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?)?);
                }
                c => o.push(c),
            },
            c => o.push(c),
        }
    }
}

/// `YYYY-MM-DD HH:MM` of seconds since 1970, UTC.
pub fn stamp(t: u64) -> String {
    let (days, secs) = ((t / 86400) as i64, t % 86400);
    // civil from days (Howard Hinnant's)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server's folder is its own: two servers, two folders, a name that
    /// could climb out of the directory kept in it.
    #[test]
    fn each_server_has_its_folder() {
        let a = folder("192.0.2.1");
        let b = folder("eqoa.example.com");
        assert_ne!(a, b);
        assert!(a.ends_with("servers/192.0.2.1"), "{a:?}");
        assert!(b.ends_with("servers/eqoa.example.com"));
        // the control: what is not a plain name does not leave `servers`
        for bad in ["../../etc", "a/b", "c:\\x", "", ".."] {
            let f = folder(bad);
            assert_eq!(f.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()), Some("servers"), "{bad:?} -> {f:?}");
            assert!(!f.file_name().unwrap().to_str().unwrap().contains(['/', '\\']));
        }
    }

    /// The record reads back as written; anything else reads as none.
    #[test]
    fn the_record_reads_back() {
        let r = Record {
            server: "192.0.2.1".into(),
            manifest: "patch/eqoa/eqoa-frontierslive-update.xml.gz".into(),
            when: 1_792_740_000,
            files: vec![Kept { name: "S2".into(), crc: 789_917_974, size: 24_508 }, Kept { name: "WORDS.TXT".into(), crc: 4_000_000_000, size: 43_648 }],
        };
        assert_eq!(Record::from_json(&r.to_json()), Some(r.clone()));
        assert_eq!(Record::from_json("not a record"), None, "the control");
        assert_eq!(stamp(1_792_740_000), "2026-10-23 07:20");
        assert!(r.describe().contains("S2 (24508 bytes, CRC 789917974)"));
    }
}
