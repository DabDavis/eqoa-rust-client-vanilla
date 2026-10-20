//! The built-in atlas (`vi::ui::atlas`, the Go client's table in the
//! decomp's nine fields) against SUPPORT's own, read out of a decrypted
//! image at run/support.bin: every entry, every field, the alpha by its
//! bits. SKIPs without the image. A table with one field moved, an alpha a
//! bit off or two entries swapped must disagree.

use vi::ui::UiTexEntry;

/// The entries that differ, by index.
fn differing(a: &[UiTexEntry], b: &[UiTexEntry]) -> Vec<usize> {
    let key = |e: &UiTexEntry| (e.x00, e.row, e.uv0, e.uv1, e.mode, e.alpha.to_bits(), e.flags);
    let mut d: Vec<usize> = (0..a.len().min(b.len())).filter(|&i| key(&a[i]) != key(&b[i])).collect();
    d.extend(a.len().min(b.len())..a.len().max(b.len()));
    d
}

#[test]
fn the_built_in_atlas_is_supports() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Ok(image) = std::fs::read(root.join("run/support.bin")) else {
        eprintln!("SKIP: needs a decrypted SUPPORT image at run/support.bin");
        return;
    };
    let theirs = vi::ui::read_atlas(&image).expect("the atlas in the image");
    let ours = vi::ui::atlas();
    assert_eq!(ours.len(), 252);
    assert_eq!(differing(&ours, &theirs), Vec::<usize>::new());

    let mut moved = ours.clone();
    moved[136].uv1[0] += 1;
    assert_eq!(differing(&moved, &theirs), vec![136]);
    let mut dim = ours.clone();
    dim[7].alpha = f32::from_bits(dim[7].alpha.to_bits() - 1);
    assert_eq!(differing(&dim, &theirs), vec![7]);
    let mut swapped = ours.clone();
    swapped.swap(115, 117);
    assert_eq!(differing(&swapped, &theirs), vec![115, 117]);
    assert_eq!(differing(&ours[..251], &theirs), vec![251]);
}
