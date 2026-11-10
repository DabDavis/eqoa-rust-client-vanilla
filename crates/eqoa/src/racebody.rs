//! A body a server gives one of its own races holds a player's items (vanilla
//! only; petition 1278, a Gargoyle: "gargoyles dont show equiped weapons").
//!
//! `VICSprite::AttachItem` attaches nothing on a sprite whose CSprite header
//! (0x2710, version 4 up) says `npc` 0: `ParseCSpriteObj` reads the field and
//! `VICSprite::Init` keeps it as `m_flags & 1`, which `AttachItem` tests before
//! anything else, so no item hangs on any node and the hands keep the attack
//! type 11 (hand to hand) and the no projectile `Clear` gave them (decomp
//! `src/viesfparse.cpp`, `src/vicsprite.cpp`; `vi::csprite::CSprite::
//! holds_items`). CLIENT tests nothing first: `VICreatureEntity::ApplyItems`
//! hands the look the channel's items and `VIClientPart06D00::Apply` calls
//! `AttachItem` for each (`modules/client/src/vicreatureentity.cpp`,
//! `viswing.cpp`), and character select calls it the same way
//! (`viwndcharacterselect.cpp`). 298 of CHAR.ESF's 568 CSprites say 0, the
//! creatures a form or an illusion makes of a player among them (Mouse Form's
//! 0x8150E6AF: `play::held`), and so does the gargoyles' body, 0xB4D2D132,
//! which go-eqoa-server's Gargoyle race wears: on a PS2 a Gargoyle player's
//! weapons are drawn nowhere, as no gargoyle's are. Every other body the
//! server's races wore on 2026-11-08 (racedefs 11 to 29 and the froglok) says
//! 1 and names a node for each of the three item slots.
//!
//! Here a player's character (name colour 2, and the pregame's figures) drawn
//! in a body one of the server's own races wears holds items as a race's body
//! does: [`grant`] sets `holds_items`, so its items hang on the nodes its
//! ASlotList names (the gargoyle's names its two hands, 11 and 21) and its
//! hands take their weapons' attack types and projectiles, as `AttachItem`
//! sets them (the gargoyle's play list has two attacks, 22 and 23, which every
//! weapon's action borrows, `SetDefaultEntries`: its swings stay its own).
//! Where such a body names no node for the shield (the gargoyle's names two
//! slots), the shield goes on the hand that holds the off hand's item, that
//! slot's node's parent (the gargoyle's left hand, 20): of the body's own
//! nodes the one that puts it nearest where a human wears it, on the forearm
//! the off hand hangs from (judged by eye with `eqoa dress <body> ... race=1`:
//! the off hand's node itself sets it low by the claws, the forearm's at the
//! elbow). A body that holds items by its header changes in nothing, so one
//! that names no shield node shows none, as on a PS2.
//!
//! The bodies are the ones the server lists in creation's races
//! (`net::creation`, 0x7E1F: the session's in the world, the create pages'
//! offer in the pregame). A body none of its races wears (a form, an
//! illusion's creature, any creature) keeps its header's word, so a form still
//! holds nothing; an NPC keeps what the PS2 gave it; and a server that lists
//! no races (Sandstorm, eqoa.live, one whose creation takes nothing past the
//! game's) is the game as it was.

use net::creation::Creation;
use vi::csprite::CSprite;
use vi::fpu::Ee;

/// Whether `model` is a body one of the server's races wears, by what it
/// listed (`races`; none: no race of its own), or one its races' options offer
/// a player (another body of the same creature, `raceopts`: the gargoyles' six).
pub fn worn_by_a_race(model: u32, races: Option<&Creation>) -> bool {
    model != 0 && (races.is_some_and(|c| c.races.iter().any(|r| r.body.contains(&model))) || crate::raceopts::offered_body(model))
}

/// Bodies whose shield node their own animations leave at another size than
/// their other item nodes, and the FixScale that puts it right: the fairy's
/// (go-eqoa-server's race 19, 0x7A85646C, on both discs). The pixie and the
/// fairy are a hand high on a skeleton two metres tall, and their animations
/// draw what they hold at 0.3 by scaling the item nodes; the pixie's do so on
/// all three, the fairy's on its two hands' and not on its shield's, node 16
/// (no vertex is skinned to it), so a shield hung there was drawn three and a
/// third times the size of the fairy (`eqoa dress 7A85646C ... shield=...`).
const SHIELD_FIX: &[(u32, f32)] = &[(0x7A85_646C, 0.3)];

/// `cs`, made of `model`, holds items when it is a player's character
/// (`player`) in a body one of the server's races wears (`races`), its shield
/// on the off hand's hand when it names no node for one, and its shield node
/// at its hands' size where its animations leave it at another
/// (`SHIELD_FIX`); otherwise it keeps its header's word and its nodes.
/// Whether it holds items after.
pub fn grant(cs: &mut CSprite<Ee>, model: u32, player: bool, races: Option<&Creation>) -> bool {
    if !player || !worn_by_a_race(model, races) {
        return cs.holds_items;
    }
    if !cs.holds_items {
        cs.holds_items = true;
        if cs.item_nodes[2] < 0
            && let Some(off) = usize::try_from(cs.item_nodes[1]).ok().and_then(|i| cs.h.nodes.get(i))
        {
            // the root is no hand: an off hand's node hung off it (the Jal'Raeth's,
            // whose item nodes float beside them as their knights' blades do) takes
            // the shield itself
            cs.item_nodes[2] = if off.parent > 0 { off.parent } else { cs.item_nodes[1] };
        }
    }
    if let Some(&(_, fix)) = SHIELD_FIX.iter().find(|(b, _)| *b == model)
        && let Some(n) = usize::try_from(cs.item_nodes[2]).ok().and_then(|i| cs.h.nodes.get_mut(i))
    {
        n.fix_scale = vi::vect::Vect3::of(fix, fix, fix);
    }
    cs.holds_items
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::creation::Race;
    use vi::character::{AnimCache, load_csprite};
    use vi::csprite::Rand;
    use vi::fpu::F;
    use vi::hsprite::{AnimScene, HSprite, Node};

    const GARGOYLE: u32 = 0xB4D2_D132;
    const MOUSE_FORM: u32 = 0x8150_E6AF;

    fn races(bodies: &[[u32; 2]]) -> Creation {
        let race = |(k, &body): (usize, &[u32; 2])| Race { id: 11 + k as i32, like: 9, city: 0, bonus: [0; 7], class_mask: 0x7FFF, body, name: format!("R{k}"), short: "R".into(), desc: String::new() };
        Creation { all_classes: true, races: bodies.iter().enumerate().map(race).collect() }
    }

    /// The sprite a body's header makes, holding no items (`npc` 0), its
    /// item slots named as the gargoyle's are: its two hands' finger bones,
    /// 11 and 21, each a child of its hand (10, 20), and no shield node.
    fn npc_body() -> CSprite<Ee> {
        let mut cs = CSprite::<Ee>::new(HSprite::new(0), 0, 9, 0, F::ONE, false);
        cs.h.nodes = (0..22).map(|i| Node { parent: i - 1, ..Default::default() }).collect();
        cs.item_nodes[..2].copy_from_slice(&[11, 21]);
        cs
    }

    /// A player in a body a race of the server's wears holds its items on
    /// the nodes the body names, its shield on the off hand's hand where it
    /// names no shield node; a female body counts as the male's does. What
    /// keeps its header's word and its nodes: an NPC or a pet in that body, a
    /// player in a body no race wears (a form), any body on a server that
    /// listed no races, and a body 0. A body that holds items already is left
    /// alone, its missing shield node too, and one that names a shield node
    /// keeps it. The control: the test without the body (a player in any
    /// body) hands the form its weapons.
    #[test]
    fn a_races_body_holds_a_players_items() {
        let listed = races(&[[GARGOYLE, 0], [0x2217_6258, 0xB749_BF57]]);
        let mut cs = npc_body();
        assert!(grant(&mut cs, GARGOYLE, true, Some(&listed)));
        assert_eq!((cs.item_node(0), cs.item_node(1), cs.item_node(2)), (Some(11), Some(21), Some(20)), "the hands it names, the shield on the off hand's hand");
        assert!(grant(&mut cs, GARGOYLE, true, Some(&listed)), "again: as it was");
        assert_eq!(cs.item_node(2), Some(20));
        assert!(grant(&mut npc_body(), 0xB749_BF57, true, Some(&listed)), "a race's female body");
        for (model, player, list, why) in [
            (GARGOYLE, false, Some(&listed), "an NPC or a pet in it"),
            (MOUSE_FORM, true, Some(&listed), "a form no race wears"),
            (GARGOYLE, true, None, "a server with no races of its own"),
            (0, true, Some(&races(&[[0, 0]])), "no body"),
        ] {
            let mut cs = npc_body();
            assert!(!grant(&mut cs, model, player, list), "{why}");
            assert_eq!((cs.item_node(0), cs.item_nodes[2]), (None, -1), "{why}");
        }
        let mut shielded = npc_body();
        shielded.item_nodes[2] = 5;
        grant(&mut shielded, GARGOYLE, true, Some(&listed));
        assert_eq!(shielded.item_node(2), Some(5), "a shield node of its own kept");
        let mut holds = npc_body();
        holds.holds_items = true;
        assert!(grant(&mut holds, GARGOYLE, true, Some(&listed)));
        assert_eq!(holds.item_node(2), None, "a body that holds items by its header: no shield node made");
        let mut human = CSprite::<Ee>::new(HSprite::new(0), 1, 0, 0, F::ONE, true);
        assert!(grant(&mut human, 0x70D8_98C6, false, None), "a body that holds items keeps holding them");

        // the control: by the player alone, the form holds the bard's weapons
        let by_player = |cs: &mut CSprite<Ee>, player: bool| {
            cs.holds_items |= player;
            cs.item_node(0)
        };
        assert_eq!(by_player(&mut npc_body(), true), Some(11), "the control: a form given weapons");
    }

    /// On the disc: the gargoyles' body says it holds nothing and names its
    /// two hands, and only `grant` with the body listed makes it hold them,
    /// its shield on its left hand; Mouse Form's (0x8150E6AF) stays bare for a
    /// player beside it; the human's held all along, on its own three nodes.
    /// Needs `run/eqoa.iso`.
    #[test]
    fn the_gargoyles_body_on_the_disc() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        let (mut sc, mut cache) = (AnimScene::<Ee>::default(), AnimCache::new());
        let mut load = |id: u32| load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut sc, &mut cache, &mut Rand(1)).unwrap();
        let listed = races(&[[GARGOYLE, GARGOYLE]]);
        let mut garg = load(GARGOYLE);
        assert!(!garg.holds_items, "the header's npc is 0");
        assert_eq!(&garg.item_nodes[..3], &[11, 21, -1], "both hands, no shield");
        assert_eq!(garg.item_node(0), None, "as a PS2 attaches nothing");
        assert!(grant(&mut garg, GARGOYLE, true, Some(&listed)));
        assert_eq!([garg.item_node(0), garg.item_node(1), garg.item_node(2)], [Some(11), Some(21), Some(20)], "the shield on its left hand");
        assert_eq!(garg.h.nodes[21].parent, 20);
        let mut mouse = load(MOUSE_FORM);
        assert!(!grant(&mut mouse, MOUSE_FORM, true, Some(&listed)), "a form holds nothing");
        let mut human = load(0x70D8_98C6);
        assert!(grant(&mut human, 0x70D8_98C6, true, Some(&listed)));
        assert_eq!([human.item_node(0), human.item_node(1), human.item_node(2)], [Some(18), Some(27), Some(28)], "the human as before");
    }

    /// The fairy's shield, as a player of the Fairy race holds it, at its hands'
    /// size: on the disc its animations draw its two hands' item nodes at 0.3
    /// and leave its shield node, 16, at 1.0, three and a third times larger;
    /// `grant` gives that node a FixScale of 0.3, so posed in its idle and its
    /// walk the shield node is drawn at the off hand's size. The controls: an
    /// NPC fairy's node keeps the art's 1.0, and the pixie, whose animations
    /// already draw its shield node at 0.3, is left as it is. Needs
    /// `run/eqoa.iso`.
    #[test]
    fn the_fairys_shield_at_its_hands_size() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        const FAIRY: u32 = 0x7A85_646C;
        const PIXIE: u32 = 0x94BA_77CD;
        let listed = races(&[[FAIRY, FAIRY], [PIXIE, PIXIE]]);
        // the world scale of item nodes `nodes` posed in action (None: standing)
        let scales = |id: u32, player: bool, action: Option<i32>| {
            let (mut sc, mut cache, mut rand) = (AnimScene::<Ee>::default(), AnimCache::new(), Rand(1));
            let mut cs = load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut sc, &mut cache, &mut rand).unwrap();
            grant(&mut cs, id, player, Some(&listed));
            let mut t = 0;
            while t <= 900 {
                sc.time = t;
                match action {
                    Some(a) if t == 0 => {
                        cs.set_action(a, &sc, &mut rand);
                    }
                    Some(_) => {}
                    None => cs.set_locomotion(0, &sc, &mut rand),
                }
                cs.h.process(0, &sc);
                t += 16;
            }
            let s = |n: i32| {
                let m = cs.h.nodes[n as usize].world.to_cols();
                (m[0][0] * m[0][0] + m[0][1] * m[0][1] + m[0][2] * m[0][2]).sqrt()
            };
            (s(cs.item_nodes[1]), s(cs.item_nodes[2]))
        };
        for action in [None, Some(1)] {
            let (off, shield) = scales(FAIRY, true, action);
            assert!((off - 0.3).abs() < 0.01 && (shield - off).abs() < 0.01, "{action:?}: the fairy's shield at its off hand's size ({shield:.3} against {off:.3})");
            let (_, npc) = scales(FAIRY, false, action);
            assert!((npc - 1.0).abs() < 0.01, "{action:?}: the control, an NPC's at the art's {npc:.3}");
            let (off, shield) = scales(PIXIE, true, action);
            assert!((off - 0.3).abs() < 0.01 && (shield - 0.3).abs() < 0.01, "{action:?}: the pixie's as its art has them");
        }
    }

    /// Every CSprite on the disc, with the bodies go-eqoa-server's races
    /// wore on 2026-11-08 listed (racedefs 11 to 29 and the froglok): for an
    /// NPC, and on a server that lists none, `grant` changes no sprite's
    /// items; for a player, it changes exactly the sprites that hold none by
    /// their header and are listed, which is the gargoyles' body alone. So
    /// every body that held items before holds them on the same nodes. The
    /// control: the same sweep with every disc body listed changes the 294
    /// that hold none and name a node for an item (298 hold none; four name
    /// no item node at all). Needs `run/eqoa.iso`.
    #[test]
    fn only_the_gargoyles_body_changes() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        let file = lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        let c = lib.file(file);
        let mut bodies: Vec<(u32, esf::NodeId)> = c.dict.iter().filter(|(_, n)| c.node(**n).ty == 0x2700).map(|(&k, &n)| (k as u32, n)).collect();
        bodies.sort_unstable();
        let servers = races(&[
            [0x141B_FE0C, 0x141B_FE0C], // froglok
            [0xFFB1_EA2C, 0xFFB1_EA2C], // hill giant
            [0x2217_6258, 0xB749_BF57], // kerran
            [0x6C13_08E0, 0x6C13_08E0], // goblin
            [0x3151_DF13, 0x8EFD_12EA], // aviak
            [0x9112_20E2, 0x9112_20E2], // lizardman
            [0x31A1_5F17, 0x31A1_5F17], // kappa
            [0x4376_EB52, 0x4376_EB52], // brownie
            [0x94BA_77CD, 0x94BA_77CD], // pixie
            [0x7A85_646C, 0x7A85_646C], // fairy
            [0xCF41_5561, 0xCF41_5561], // gnoll
            [0x8214_A32D, 0x8214_A32D], // orc
            [0xCFC4_78EE, 0xCFC4_78EE], // kobold
            [0xC5CB_F4CF, 0xC5CB_F4CF], // minotaur
            [0xBADC_25C9, 0xBADC_25C9], // ratman
            [0xBE10_226F, 0xBE10_226F], // satyr
            [0x70EF_CE05, 0x70EF_CE05], // dryad
            [0x6AF9_8A81, 0x6AF9_8A81], // skeleton
            [0x4A4D_88FB, 0x4A4D_88FB], // fungusman
            [GARGOYLE, GARGOYLE],       // gargoyle
        ]);
        let every = races(&bodies.iter().map(|&(id, _)| [id, id]).collect::<Vec<_>>());
        let (mut sc, mut cache) = (AnimScene::<Ee>::default(), AnimCache::new());
        let held = |cs: &CSprite<Ee>| [cs.item_node(0), cs.item_node(1), cs.item_node(2)];
        let mut changed = |list: &Creation, player: bool| {
            let mut out = Vec::new();
            for &(id, node) in &bodies {
                let Some(mut cs) = load_csprite(&lib, esf::Ref { file, node }, &mut sc, &mut cache, &mut Rand(1)) else { continue };
                let before = held(&cs);
                grant(&mut cs, id, player, Some(list));
                if held(&cs) != before {
                    out.push(id);
                }
            }
            out
        };
        assert_eq!(changed(&servers, false), Vec::<u32>::new(), "an NPC's: none");
        assert_eq!(changed(&races(&[]), true), Vec::<u32>::new(), "a server with no races: none");
        assert_eq!(changed(&servers, true), vec![GARGOYLE], "a player's: the gargoyles' body alone");
        assert_eq!(changed(&every, true).len(), 294, "the control: every body that holds nothing and names an item node, listed");
    }
}
