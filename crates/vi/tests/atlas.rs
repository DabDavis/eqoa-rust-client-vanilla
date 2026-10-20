//! `read_atlas` on the player's SUPPORT image (`run/support.bin`, else the
//! disc's own, unpacked by `dnas`): `GetUITexture`'s 252 entries, each
//! numbering itself, mode 0 and alpha 1, every row a row of TEXTURES.TXT and
//! every rectangle (an origin and a size) on the 512-texel art. SKIPs
//! without an image. The check that keeps another build's data out must
//! refuse an image whose table is off by one entry, one whose entry 200 is
//! not its own, and a short one.

mod common;

#[test]
fn supports_atlas_reads_whole() {
    let Some(image) = common::support_image() else {
        eprintln!("SKIP: needs a SUPPORT image (run/support.bin, or the disc at run/eqoa.iso)");
        return;
    };
    let atlas = vi::ui::read_atlas(&image).expect("the Frontiers atlas");
    assert_eq!(atlas.len(), 252);
    for (i, e) in atlas.iter().enumerate() {
        assert_eq!((e.x00, e.mode, e.alpha), (i as i32, 0, 1.0), "entry {i}");
        assert!((0..400).contains(&e.row), "entry {i}'s row {}", e.row);
        assert!(e.uv0.iter().all(|c| (0..512).contains(c)) && e.uv1.iter().all(|&s| s >= 0), "entry {i}'s rectangle {:?} {:?}", e.uv0, e.uv1);
    }

    // the table moved by one entry (36 bytes), as another build would have it
    let base = 0x004E_DAF8 - 0x003E_3D00;
    let mut moved = image.clone();
    moved.copy_within(base + 36..base + 252 * 36, base);
    assert!(vi::ui::read_atlas(&moved).is_none(), "a table off by one entry");
    let mut one = image.clone();
    one[base + 200 * 36] ^= 1;
    assert!(vi::ui::read_atlas(&one).is_none(), "entry 200 not numbered 200");
    assert!(vi::ui::read_atlas(&image[..base + 251 * 36]).is_none(), "an image that ends inside the table");
}
