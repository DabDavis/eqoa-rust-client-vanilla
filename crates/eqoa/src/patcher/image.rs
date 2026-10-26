//! `VIPatchImage` (SETUP `setup.h`): the card's files in memory while the
//! patch runs, and `VISetup::ReadPatchData` / `WritePatchData` over a
//! server's folder (`servers`) instead of the memory card.

/// One card file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub name: String,
    pub data: Vec<u8>,
    /// the CRC-32 of `data`, as ReadPatchData leaves it
    pub crc: u32,
    /// downloaded: to be written
    pub dirty: bool,
    /// a Delete entry named it: to be removed
    pub deleted: bool,
}

/// The images, newest first (ReadPatchData prepends).
#[derive(Clone, Debug, Default)]
pub struct Images {
    pub list: Vec<Image>,
}

impl Images {
    /// `FindPatchImage`: the first image whose name occurs IN `path`
    /// (`strstr(path, name)`), so "dir/S2" finds "S2".
    pub fn find(&mut self, path: &str) -> Option<&mut Image> {
        self.list.iter_mut().find(|im| !im.name.is_empty() && path.contains(&im.name))
    }

    pub fn find_ref(&self, path: &str) -> Option<&Image> {
        self.list.iter().find(|im| !im.name.is_empty() && path.contains(&im.name))
    }

    /// `Http_get::WriteFile`: the fetched bytes into the image named after
    /// `path`'s last part (an image found by `find`, else a new one).
    pub fn write(&mut self, path: &str, data: Vec<u8>) {
        let name = path.rsplit(['/', '\\']).find(|t| !t.is_empty()).unwrap_or("").to_string();
        if name.is_empty() {
            return;
        }
        let crc = crc32(&data);
        match self.find(path) {
            Some(im) => {
                im.name = name;
                im.data = data;
                im.crc = crc;
                im.dirty = true;
            }
            None => self.list.push(Image { name, data, crc, dirty: true, deleted: false }),
        }
    }

    /// Whether the patch changed anything (`m_x218B4`: what makes the notes
    /// open by themselves).
    pub fn changed(&self) -> bool {
        self.list.iter().any(|im| im.dirty || im.deleted)
    }
}

/// zlib's CRC-32, as `VIPatchImage` keeps it.
pub fn crc32(data: &[u8]) -> u32 {
    let mut h = flate2::Crc::new();
    h.update(data);
    h.sum()
}

/// A plain file name a folder may hold: no path, no device, not one of
/// Windows' reserved names.
pub fn safe_name(n: &str) -> bool {
    if n.is_empty() || n == "." || n == ".." || n.contains(['/', '\\', ':']) || n.chars().any(|c| (c as u32) < 0x20) {
        return false;
    }
    let stem = n.split('.').next().unwrap_or("").to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9")
}

/// What a folder holds that is the card's: every plain file but the record
/// and a write cut off (`.part`).
pub fn is_card_file(name: &str) -> bool {
    safe_name(name) && name != crate::servers::RECORD && !name.ends_with(".part")
}

/// `ReadPatchData` over a folder: every card file, prepended in turn (so the
/// last read is first), at most 4 MB each (the card's read limit); the
/// progress through them (0..1).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_folder(dir: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<Images, String> {
    let mut names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).filter(|e| e.file_type().is_ok_and(|t| t.is_file())).filter_map(|e| e.file_name().into_string().ok()).filter(|n| is_card_file(n)).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    names.sort();
    let mut images = Images::default();
    let n = names.len().max(1);
    for (i, name) in names.into_iter().enumerate() {
        let mut data = std::fs::read(dir.join(&name)).map_err(|e| format!("{name}: {e}"))?;
        data.truncate(0x40_0000);
        let crc = crc32(&data);
        images.list.insert(0, Image { name, data, crc, dirty: false, deleted: false });
        progress((i + 1) as f32 / n as f32);
    }
    Ok(images)
}

/// `WritePatchData` over a folder: each downloaded image written whole
/// beside its file and moved over it, each deleted one removed; nothing at
/// all when nothing changed. Whether anything did.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_folder(dir: &std::path::Path, images: &Images, progress: &mut dyn FnMut(f32)) -> Result<bool, String> {
    if !images.changed() {
        progress(1.0);
        return Ok(false);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let total = images.list.len().max(1);
    for (i, im) in images.list.iter().enumerate() {
        if !safe_name(&im.name) {
            return Err(format!("refusing to write {:?}: not a plain file name", im.name));
        }
        if im.dirty && !im.deleted {
            super::say(&format!("patcher: POST PATCH: Writing (new/modified) file ({}), CRC = {}", im.name, im.crc));
            let path = dir.join(&im.name);
            let part = dir.join(format!("{}.part", im.name));
            std::fs::write(&part, &im.data).and_then(|_| std::fs::rename(&part, &path)).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if im.deleted {
            super::say(&format!("patcher: POST PATCH: Deleting file ({})", im.name));
            match std::fs::remove_file(dir.join(&im.name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{}: {e}", im.name)),
                _ => {}
            }
        }
        progress(i as f32 / total as f32);
    }
    progress(1.0);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `FindPatchImage` matches by the name occurring in the path; a write
    /// lands in the image of the path's last part.
    #[test]
    fn an_image_is_found_by_its_name_in_the_path() {
        let mut im = Images::default();
        im.list.push(Image { name: "S2".into(), data: b"old".to_vec(), crc: crc32(b"old"), ..Default::default() });
        assert!(im.find("dir/S2").is_some());
        assert!(im.find("SS").is_none(), "the control: another name");
        im.write("dir/S2", b"new".to_vec());
        assert_eq!(im.list.len(), 1);
        assert_eq!((im.list[0].data.as_slice(), im.list[0].dirty), (&b"new"[..], true));
        im.write("WORDS.TXT", b"w".to_vec());
        assert_eq!(im.list.len(), 2);
        assert!(im.changed());
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926, "zlib's CRC-32");
        assert!(!safe_name("../x") && !safe_name("CON.txt") && safe_name("ss-art.csf"));
        assert!(!is_card_file("patch.json") && !is_card_file("S2.part") && is_card_file("S2"));
    }
}
