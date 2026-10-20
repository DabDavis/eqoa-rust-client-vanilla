//! The music's disc reads in a page (`crate::bgm`): the files the device
//! asked for, read whole off the player's image and handed to the cache.

use crate::bgm::Bgm;

/// Starts the reads asked for since the last call, off `file`.
pub fn fetch(bgm: &Bgm, file: &web_sys::File) {
    for (lsn, size) in bgm.take_wanted() {
        let (bgm, file) = (bgm.clone(), file.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let t = web_time::Instant::now();
            match super::disc::read_blob(&file, lsn as u64 * iso::SECTOR, size as usize).await {
                Ok(b) => {
                    super::log(&format!("music: {} read ({:.1} MB, {} ms)", bgm.name(lsn), b.len() as f64 / 1048576.0, t.elapsed().as_millis()));
                    bgm.insert(lsn, b);
                }
                Err(e) => {
                    super::log(&format!("music: {} could not be read ({e})", bgm.name(lsn)));
                    bgm.fail(lsn);
                }
            }
        });
    }
}
