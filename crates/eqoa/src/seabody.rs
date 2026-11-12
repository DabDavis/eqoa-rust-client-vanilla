//! A player's own character in a sea creature's body treads water standing and
//! swims moving, on land too (the GM Jibrish's petition 1620: the Mermaid race,
//! go-eqoa-server's 32, in the merfolk's bodies).
//!
//! The sea creatures' bodies (the merfolk, sirens, sharks, eels: 29 of
//! CHAR.ESF's) have one play list: 0 idle, 12 swim, 13 tread water, 14 death,
//! 34 hit and 44, the swimming attack. Their walk and run are the idle
//! (`SetDefaultEntries` fills 1 to 6 from entry 0), and so is every swing a
//! weapon asks (23, 22 and the rest), so a body standing or moving on land
//! drifts in its idle and swings nothing. `VICSprite::SetAnimation` turns any
//! attack (22 to 53 but 34) into 44 only while the body plays 12, 13 or 44.
//! The server sends such a player to every viewer treading water and swimming
//! (go-eqoa-server `entity/swimmerbody.go`, the channel's byte 58), so viewers
//! see its swings; here the player's own client does the same with the gait
//! its walk gives its own sprite (`UpdateSprite`), its swing then its attack.
//! A PS2 draws its own character in its walk's gait, the idle, and its swings
//! as nothing: it cannot be changed. The walk itself, its speed and what goes
//! on the wire (0x40's gait) are as they were.

use vi::csprite::CSprite;
use vi::fpu::Ee;

/// Whether `cs` is a sea creature's body: one whose walk, run and first swing
/// are its idle (it has none of its own: `SetDefaultEntries` filled them from
/// entry 0) and whose swimming attack, 44, is its own. Its swim and tread may be
/// its idle's clip (the merfolk's are), so they are not asked.
pub fn swims(cs: &CSprite<Ee>) -> bool {
    let e = &cs.entries;
    let idle = |i: usize| e[i].animation == e[0].animation;
    idle(1) && idle(3) && idle(23) && e[44].animation[0] != -1 && !idle(44)
}

/// The gait a sea creature's body plays for its walk's `gait`: treading water
/// for a stand or a turn, swimming for any walk, run or sidestep; a fall, a
/// landing, a death and the water's own as they are.
pub fn gait(gait: i32) -> i32 {
    match gait {
        0 | 4 | 5 => 13,
        1 | 2 | 3 | 10 | 11 => 12,
        g => g,
    }
}

/// The gait `cs` is shown in for its walk's `g`: `gait` on a sea creature's
/// body, `g` on any other (the walker's own, `Walker::pose_sounding`).
#[cfg(test)]
pub fn shown(cs: &CSprite<Ee>, g: i32) -> i32 {
    if swims(cs) { gait(g) } else { g }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vi::character::{AnimCache, load_csprite};
    use vi::csprite::Rand;
    use vi::hsprite::AnimScene;

    /// CHAR.ESF off `run/eqoa.iso`, or none (the test skipped).
    fn char_library() -> Option<esf::Library> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return None;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").ok()?).ok()?);
        Some(lib)
    }

    fn sprite(lib: &esf::Library, id: u32) -> CSprite<Ee> {
        sprite_in(lib, id, &mut AnimScene::<Ee>::default())
    }

    fn sprite_in(lib: &esf::Library, id: u32, sc: &mut AnimScene<Ee>) -> CSprite<Ee> {
        let r = lib.lookup(id as i32).expect("body");
        load_csprite(lib, r, sc, &mut AnimCache::new(), &mut Rand(1)).expect("sprite")
    }

    /// The merfolk's four bodies, the Megaladon's and a shark's swim; the
    /// human's, a gnoll's and a gargoyle's (two attacks of its own, no swim)
    /// do not. Their gaits: a stand treads water, a walk or run swims, a death
    /// is a death.
    #[test]
    fn the_sea_creatures_bodies_swim() {
        let Some(lib) = char_library() else { return };
        for id in [0xA316EB78u32, 0x74BA7C3D, 0xBF21FEF5, 0x1740427C, 0x69842C02, 0xBEFB4465] {
            assert!(swims(&sprite(&lib, id)), "{id:08X} swims");
        }
        for id in [0x70D898C6u32, 0xCF415561, 0xB4D2D132] {
            assert!(!swims(&sprite(&lib, id)), "the control: {id:08X} walks");
        }
        let mermaid = sprite(&lib, 0x74BA7C3D);
        assert_eq!([0, 1, 3, 4, 7, 14, 12, 13].map(|g| shown(&mermaid, g)), [13, 12, 12, 13, 7, 14, 12, 13]);
        let human = sprite(&lib, 0x70D898C6);
        assert_eq!([0, 1, 3].map(|g| shown(&human, g)), [0, 1, 3]);
    }

    /// A mermaid treading water swings her own attack (`SetAnimation`'s 44),
    /// where standing in her walk's stand she swings her idle. The control:
    /// the human's swing is its own 23.
    #[test]
    fn a_mermaid_treading_water_swings_her_attack() {
        let Some(lib) = char_library() else { return };
        let mut rand = Rand(1);
        let mut sc = AnimScene::<Ee>::default();
        let mut mermaid = sprite_in(&lib, 0x74BA7C3D, &mut sc);
        let mut stood = sprite_in(&lib, 0x74BA7C3D, &mut sc);
        mermaid.set_locomotion(shown(&mermaid, 0), &sc, &mut rand);
        stood.set_locomotion(0, &sc, &mut rand);
        sc.time = 500;
        mermaid.set_action(23, &sc, &mut rand);
        stood.set_action(23, &sc, &mut rand);
        let layer = if mermaid.skel_type == 1 { 1 } else { 0 };
        assert_eq!(mermaid.cur[layer].anim, 44, "her swing in the water");
        assert_ne!(stood.cur[layer].anim, 44, "the control: standing, the idle's swing");
    }
}
