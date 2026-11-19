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
//! no races (the reference server, eqoa.live, one whose creation takes nothing past the
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

/// Keeps `cs`'s item nodes posed by its animations at every distance (petition
/// 2310, the owner, 2026-11-18: "Fix it in vanilla", a departure from the
/// game, which drew it so too). `ProcessPlayList` gives a node at or past the
/// skeleton's level its rest frame, at its art's scale; the pixie and the
/// fairy (a hand high on a skeleton two metres tall) draw what they hold at
/// 0.3 by scaling their item nodes, so past a node's level what it held was
/// drawn at 1.0, three and a third times the body: the fairy's shield (node
/// 16, level 0) from 5 away, its weapons (nodes 8 and 15, level 1) from 15,
/// zooming out on a Druid in Fey Mask. An item node's level is made one no
/// skeleton's level reaches; every other node rests as the game rests it.
pub fn keep_items_posed(cs: &mut CSprite<Ee>) {
    for &n in &cs.item_nodes[..3] {
        if let Some(node) = usize::try_from(n).ok().and_then(|i| cs.h.nodes.get_mut(i)) {
            node.lod = i32::MAX;
        }
    }
}

/// Where a body whose item nodes are no hands holds a player's items: for
/// each item slot (main hand, off hand, shield) the node it hangs from and the
/// frame of a node made there for it, in that node's space. The Jal'Raeth's
/// three (go-eqoa-server's race 30: robes, channeler, knight; one 42-node
/// skeleton) name their item nodes 38 and 37, which hang off the root at the
/// knee, where their knights' blade discs float (the owner, 2026-11-11: "they
/// hold weapons weird"): a sword hung there floated beside the crossed legs.
/// Their arms have the human's bone names (hand 10 and 21, forearm 20, the
/// namesakes of HumanMale's 13, 22 and 21, which its item nodes 18, 27 and 28
/// hang from) and, as the human's, every node's axes are the model's in the
/// bind pose, so each made node is the human's item node's frame under its
/// namesake parent (the off hand's turned half round its z as the human's
/// 27 is), moved out by the Jal'Raeth's longer hand (0.18 from wrist to
/// fingertip against the human's 0.10) and forearm (0.34 against 0.24).
/// `nodes`: the skeleton's own count, which tells a sprite already given them.
struct HandSeats {
    bodies: &'static [u32],
    nodes: usize,
    /// Per item slot: the parent node, the frame's rotation (x, y, z, w) and
    /// its place.
    seats: [(i32, [f32; 4], [f32; 3]); 3],
}

/// The hand seats (`HandSeats`), by skeleton.
///
/// The merfolk's four (go-eqoa-server's race 32, the Mermaid, petition 1620:
/// the merman, the mermaid and the two sirens of Odus; one 24-node skeleton)
/// name the right hand's last node, 18, for both hands and none for the
/// shield, so a sword and a shield would hang together in the right hand.
/// Their arms run as the human's do (right arm 14-18 at -x, left 9-13 at +x:
/// collar, shoulder, elbow, wrist, fingertips; every node with the model's
/// axes in the bind pose), with no bone name of the human's; each made node is
/// the human's item node's frame under the namesake of its parent by place
/// (wrists 17 and 12, as HumanMale's hands 13 and 22, its off hand turned half
/// round its z; the left forearm 11, as its 21), the hand and the forearm about
/// the human's lengths (0.09 wrist to fingertip, 0.27 elbow to wrist).
const HAND_SEATS: &[HandSeats] = &[
    HandSeats {
        bodies: &[0xF51B_AB09, 0x4B24_FF7E, 0x491F_6347],
        nodes: 42,
        seats: [(10, [0.0, 0.0, 0.0, 1.0], [-0.08, -0.035, 0.0]), (21, [0.0, 0.0, 1.0, 0.0], [0.08, -0.035, 0.0]), (20, [0.0, 0.0, 0.0, 1.0], [0.2, 0.11, 0.0])],
    },
    HandSeats {
        bodies: MERFOLK,
        nodes: 24,
        seats: [(17, [0.0, 0.0, 0.0, 1.0], [-0.045, -0.02, 0.0]), (12, [0.0, 0.0, 1.0, 0.0], [0.045, -0.02, 0.0]), (11, [0.0, 0.0, 0.0, 1.0], [0.15, 0.08, 0.0])],
    },
];

/// The merfolk's bodies: go-eqoa-server's race 32's merman (its male's),
/// mermaid (its female's), the two sirens and the sea king (Quintashi's body,
/// the same skeleton) its options offer.
pub const MERFOLK: &[u32] = &[0xA316_EB78, 0x74BA_7C3D, 0xBF21_FEF5, 0x1740_427C, 0xD275_EE8F];

/// Bodies whose hair is a part of their mesh that comes out through a helm, and
/// where it lies: a palette entry and a rect of texture coordinates in its
/// texture (u0, v0, u1, v1; `ALL` the whole entry) each. The merman's, the
/// mermaid's and the two sirens' (go-eqoa-server's race 32; the owner,
/// 2026-11-12: "the mermaid helms arent sitting right"), whose long hair
/// streamed out of the back of every helm and whose crown showed through the top
/// of the smaller ones: a rect of their one texture, entry 0. The dryads' (race
/// 26; the owner, 2026-11-16: "cll is a dryad with the helm not sitting
/// right"), whose locks and long tail stood out of the top, the sides and the
/// back of every helm: the dryad's own body's a rect of her body's atlas
/// (entry 0: the box of hair beside the bark, 289 triangles) and her two strands
/// in a hair strip (entry 6); her kin's (the Erych guardian, the Haliai, the
/// Erych arbiter) every entry drawn in their hair strip (`87BFA9B5` white,
/// `9F87C7AF` sea green, `D499494B` red), the fringe at the brow too. Under a
/// helm their hair is left out, as a player race's `$Hair` is (the sea king has
/// none).
const HAIR: [f32; 4] = [0.0, 0.0, 63.0 / 255.0, 128.0 / 255.0];
const ALL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
/// A part of a body's hair (`HAIR_UNDER_HELM`): its palette entry, the rect of
/// texture coordinates it lies in, and the nodes its triangles lie on (each
/// corner weighted most to one of them; `ANY` wherever they lie).
type Hair = (i32, [f32; 4], &'static [u8]);
const ANY: &[u8] = &[];
const HAIR_UNDER_HELM: &[(u32, &[Hair])] = &[
    (0xA316_EB78, &[(0, HAIR, ANY)]),
    (0x74BA_7C3D, &[(0, HAIR, ANY)]),
    (0xBF21_FEF5, &[(0, HAIR, ANY)]),
    // the enchanting siren's tiara too (a strip below the hair, on her head and
    // neck alone), which stood out of the front of every helm
    (0x1740_427C, &[(0, HAIR, ANY), (0, [0.0, 128.0 / 255.0, 63.0 / 255.0, 150.0 / 255.0], ANY)]),
    (0x70EF_CE05, &[(0, [0.0, 0.30, 0.69, 0.93], ANY), (6, ALL, ANY)]),
    (0x21F3_11BB, &[(0, ALL, ANY), (1, ALL, ANY), (6, ALL, ANY)]),
    (0xACE7_1E26, &[(1, ALL, ANY), (6, ALL, ANY)]),
    (0xB90A_DDD2, &[(0, ALL, ANY), (1, ALL, ANY), (8, ALL, ANY), (9, ALL, ANY), (10, ALL, ANY)]),
    // the brownie's spikes (entry 2) and the fringe at its brow (1), both in a hair strip
    (0x4376_EB52, &[(1, ALL, ANY), (2, ALL, ANY)]),
    // the brownie elder's red cap, a box of its one atlas
    (0x4946_8331, &[(0, [0.26, 0.25, 0.50, 0.49], ANY)]),
    // the goblins' topknots, tufts, mohawk and braid, each an entry drawn in an NPC hair
    // strip (black `B5632329`, auburn `EF0C9AEB`) on the head (a robed grey goblin's
    // strip on its sleeves, 581B6A80's and 687E7645's entries 9 and 0, stays)
    (0x6C13_08E0, &[(7, ALL, ANY)]),
    (0x581B_6A80, &[(7, ALL, ANY)]),
    (0x9637_1804, &[(0, ALL, ANY)]),
    (0xBCDF_0C04, &[(7, ALL, ANY)]),
    (0xE961_79D4, &[(6, ALL, ANY)]),
    (0xA5F9_2F43, &[(0, ALL, ANY)]),
    (0xEDB4_E0AD, &[(1, ALL, ANY)]),
    (0x687E_7645, &[(4, ALL, ANY)]),
    (0xEC08_85EB, &[(0, ALL, ANY)]),
    (0x1F8E_11D3, &[(4, ALL, ANY)]),
    (0x1245_E56B, &[(2, ALL, ANY)]),
    (0x27A4_E554, &[(7, ALL, ANY)]),
    (0x507C_4A52, &[(7, ALL, ANY)]),
    // the maned lion's and the white tiger's manes where they lie on the head (node
    // 5): what hangs from the neck and the shoulders stays, as hair below a helm
    (0x5D40_C8AB, &[(1, ALL, &[5])]),
    (0x77A7_E7B8, &[(2, ALL, &[5])]),
    // the Deathfist slayer's hair (entry 8) and its four loose strands (0-3)
    (0xC76F_1FA8, &[(0, ALL, ANY), (1, ALL, ANY), (2, ALL, ANY), (3, ALL, ANY), (8, ALL, ANY)]),
    // the orcs' long hair and its loose strands
    (0x84F4_7767, &[(1, ALL, ANY), (2, ALL, ANY), (3, ALL, ANY), (4, ALL, ANY), (5, ALL, ANY)]),
    (0xB2FB_D7EF, &[(1, ALL, ANY), (2, ALL, ANY), (3, ALL, ANY), (4, ALL, ANY), (5, ALL, ANY)]),
    (0x0A03_EB92, &[(0, ALL, ANY), (1, ALL, ANY), (2, ALL, ANY), (3, ALL, ANY), (4, ALL, ANY)]),
    (0xDA51_5962, &[(0, ALL, ANY), (1, ALL, ANY), (2, ALL, ANY), (3, ALL, ANY), (4, ALL, ANY)]),
];

/// Takes body `id`'s hair (`HAIR_UNDER_HELM`) out of `model`, a player's
/// character's wearing a helm: each triangle of a listed entry whose texture
/// coordinates' centre (its first layer's) lies in that entry's rect, and whose
/// corners lie on the part's nodes when it names any.
pub fn hide_hair(model: &mut esf::Model, id: u32) {
    let Some(&(_, parts)) = HAIR_UNDER_HELM.iter().find(|(b, _)| *b == id) else { return };
    let (uv, bones, weights) = (model.uv.clone(), model.bones.clone(), model.weights.clone());
    let mut gone = 0;
    for b in model.batches.iter_mut() {
        let rects: Vec<&Hair> = parts.iter().filter(|(e, _, _)| *e == b.material).collect();
        if rects.is_empty() {
            continue;
        }
        let Some(l) = b.layers.first().cloned() else { continue };
        let before = b.indices.len();
        b.indices = b
            .indices
            .chunks_exact(3)
            .filter(|t| {
                let c = t.iter().fold([0.0f32; 2], |a, &v| {
                    let q = l.apply_uv(uv[v as usize]);
                    [a[0] + q[0].rem_euclid(1.0) / 3.0, a[1] + q[1].rem_euclid(1.0) / 3.0]
                });
                let on = |nodes: &[u8]| {
                    nodes.is_empty()
                        || t.iter().all(|&v| {
                            let (bo, w) = (bones[v as usize], weights[v as usize]);
                            (0..4).max_by_key(|&k| w[k]).is_some_and(|k| nodes.contains(&bo[k]))
                        })
                };
                !rects.iter().any(|(_, r, n)| c[0] >= r[0] && c[0] < r[2] && c[1] >= r[1] && c[1] < r[3] && on(n))
            })
            .flatten()
            .copied()
            .collect();
        gone += (before - b.indices.len()) / 3;
    }
    model.triangles -= gone as u32;
}

/// Bodies a server's race offers whose mesh holds a weapon of its own, and the
/// palette entries it is drawn in: the sea king's trident (Quintashi's body,
/// entries 4-6, every triangle on its right hand's fingertips, 18), left out of
/// a player's character, who holds its own items (`drop_own_weapons`).
const OWN_WEAPONS: &[(u32, &[i32])] = &[(0xD275_EE8F, &[4, 5, 6])];

/// Takes the triangles of body `id`'s own weapon (`OWN_WEAPONS`) out of
/// `model`, a player's character's.
pub fn drop_own_weapons(model: &mut esf::Model, id: u32) {
    let Some((_, entries)) = OWN_WEAPONS.iter().find(|(b, _)| *b == id) else { return };
    for b in model.batches.iter_mut().filter(|b| entries.contains(&b.material)) {
        model.triangles -= (b.indices.len() / 3) as u32;
        b.indices.clear();
    }
}

/// The hand seats of body `model` (`HAND_SEATS`).
fn hand_seats(model: u32) -> Option<&'static HandSeats> {
    HAND_SEATS.iter().find(|h| h.bodies.contains(&model))
}

/// The nodes made under `cs`'s hands for its main and off hand (`HAND_SEATS`),
/// when body `model` has hand seats and `cs` has been given them (`grant`, a
/// player's): what holds an item in each hand, so their parents are its hands.
pub fn hand_items(model: u32, cs: &CSprite<Ee>) -> Option<[i32; 2]> {
    let seats = hand_seats(model)?;
    (cs.h.nodes.len() == seats.nodes + seats.seats.len()).then(|| [cs.item_nodes[0], cs.item_nodes[1]])
}

/// Gives `cs` a node for each item slot where `HAND_SEATS` says, under its
/// hands, and names them its item nodes; once only (a sprite with more nodes
/// than its skeleton's has them already), and on the skeleton the table was
/// measured on alone.
fn seat_hands(cs: &mut CSprite<Ee>, seats: &HandSeats) {
    if cs.h.nodes.len() != seats.nodes {
        return;
    }
    for (slot, &(parent, q, t)) in seats.seats.iter().enumerate() {
        let Some(p) = cs.h.nodes.get(parent as usize) else { return };
        use vi::fpu::F;
        let pb = p.bind;
        let rot = vi::quat::Quat { x: F::new(q[0]), y: F::new(q[1]), z: F::new(q[2]), w: F::new(q[3]) };
        let local = vi::geom::Matrix44::transform_quat(&rot, F::ONE, &vi::vect::Vect3::of(t[0], t[1], t[2]));
        let bind = vi::geom::Matrix44::mul(&local, &vi::geom::Matrix44::transform_quat(&pb.rot, pb.scale, &pb.trans));
        let (bq, bs, bt) = bind.convert_to();
        let i = cs.h.add(parent);
        if i < 0 {
            return;
        }
        cs.h.set_node_bind_pose(i as usize, bq, bs, bt);
        let n = &mut cs.h.nodes[i as usize];
        n.frame = n.rest;
        n.fix_scale = vi::vect::Vect3::of(1.0, 1.0, 1.0);
        cs.item_nodes[slot] = i;
    }
}

/// `cs`, made of `model`, holds items when it is a player's character
/// (`player`) in a body one of the server's races wears (`races`), its shield
/// on the off hand's hand when it names no node for one; otherwise it keeps
/// its header's word and its nodes. Either way its item nodes stay posed by
/// its animations at any distance (`keep_items_posed`).
/// Whether it holds items after.
pub fn grant(cs: &mut CSprite<Ee>, model: u32, player: bool, races: Option<&Creation>) -> bool {
    if !player || !worn_by_a_race(model, races) {
        keep_items_posed(cs);
        return cs.holds_items;
    }
    if !cs.holds_items {
        cs.holds_items = true;
        if cs.item_nodes[2] < 0
            && let Some(off) = usize::try_from(cs.item_nodes[1]).ok().and_then(|i| cs.h.nodes.get(i))
        {
            // the root is no hand: an off hand's node hung off it takes the shield
            // itself (the Jal'Raeth's, whose item nodes float beside them as their
            // knights' blades do, are given nodes in their hands below instead)
            cs.item_nodes[2] = if off.parent > 0 { off.parent } else { cs.item_nodes[1] };
        }
    }
    // item nodes that are no hands: nodes of their own under the hands
    if let Some(seats) = hand_seats(model) {
        seat_hands(cs, seats);
    }
    keep_items_posed(cs);
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

    /// What the fairy and the pixie hold, at their hands' size at every
    /// distance (petition 2310: a Druid in Fey Mask, whose shield grew as the
    /// camera pulled back): their animations draw their three item nodes at
    /// 0.3, and posed at each of the skeleton's levels (-1, nearer than its
    /// first, to 3, past its last) after `grant` every one of them stays at
    /// 0.3, a player's and an NPC's alike. The control: the body as loaded,
    /// without `grant`, draws the fairy's shield node (level 0) at its art's
    /// 1.0 from level 0 and its hands' (level 1) from level 1, three and a
    /// third times the body. Needs `run/eqoa.iso`.
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
        // the world scale of the main hand's, the off hand's and the shield's
        // item nodes posed at level `lod` in action (None: standing); `granted`
        // false: the body as loaded (the control)
        let scales = |id: u32, player: bool, granted: bool, lod: i32, action: Option<i32>| {
            let (mut sc, mut cache, mut rand) = (AnimScene::<Ee>::default(), AnimCache::new(), Rand(1));
            let mut cs = load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut sc, &mut cache, &mut rand).unwrap();
            if granted {
                grant(&mut cs, id, player, Some(&listed));
            }
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
                cs.h.process(lod, &sc);
                t += 16;
            }
            let s = |n: i32| {
                let m = cs.h.nodes[n as usize].world.to_cols();
                (m[0][0] * m[0][0] + m[0][1] * m[0][1] + m[0][2] * m[0][2]).sqrt()
            };
            [s(cs.item_nodes[0]), s(cs.item_nodes[1]), s(cs.item_nodes[2])]
        };
        for action in [None, Some(1)] {
            for lod in -1..=3 {
                for (id, name) in [(FAIRY, "fairy"), (PIXIE, "pixie")] {
                    for player in [true, false] {
                        let got = scales(id, player, true, lod, action);
                        assert!(got.iter().all(|s| (s - 0.3).abs() < 0.01), "{name} (player {player}) at level {lod}, {action:?}: {got:.3?}, each 0.3");
                    }
                }
            }
            let near = scales(FAIRY, false, false, -1, action);
            assert!(near.iter().all(|s| (s - 0.3).abs() < 0.01), "the control near, {action:?}: {near:.3?}");
            let [_, _, shield] = scales(FAIRY, false, false, 0, action);
            assert!((shield - 1.0).abs() < 0.01, "the control: the shield node rests at its art's 1.0 from level 0 ({shield:.3}), {action:?}");
            let [main, off, _] = scales(FAIRY, false, false, 1, action);
            assert!((main - 1.0).abs() < 0.01 && (off - 1.0).abs() < 0.01, "the control: the hands rest at 1.0 from level 1 ({main:.3}, {off:.3}), {action:?}");
        }
    }

    /// The Jal'Raeth (go-eqoa-server's race 30) hold a player's items in their
    /// hands: on each of their three bodies `grant` makes three nodes, under the
    /// namesakes of the nodes HumanMale's item nodes hang from (its hands 13 and 22,
    /// its left forearm 21), once only, and names them the item nodes; posed in the
    /// idle and in the first attack each sits at its hand (within a hand's length
    /// of it), where the body's own item nodes (the control) float at the knee,
    /// half a metre and more away in the swing (in the idle the hands rest on the
    /// knees). An NPC in the body keeps its own nodes. Needs
    /// `run/eqoa.iso`.
    #[test]
    fn a_jalraeth_holds_its_items_in_its_hands() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        let human = crate::raceemote::Rig::load(&lib, 0x70D8_98C6).unwrap();
        for &id in HAND_SEATS[0].bodies {
            let listed = races(&[[id, id]]);
            let rig = crate::raceemote::Rig::load(&lib, id).unwrap();
            let (mut sc, mut cache, mut rand) = (AnimScene::<Ee>::default(), AnimCache::new(), Rand(1));
            let mut cs = load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut sc, &mut cache, &mut rand).unwrap();
            let own = cs.item_nodes;
            assert_eq!(cs.h.nodes.len(), 42);
            let mut npc = load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut AnimScene::<Ee>::default(), &mut AnimCache::new(), &mut Rand(1)).unwrap();
            grant(&mut npc, id, false, Some(&listed));
            assert_eq!((npc.h.nodes.len(), npc.item_nodes), (42, own), "{id:#010x}: an NPC's as it was");
            assert!(grant(&mut cs, id, true, Some(&listed)));
            let made = cs.item_nodes;
            assert_eq!(&made[..3], &[42, 43, 44], "{id:#010x}");
            assert!(grant(&mut cs, id, true, Some(&listed)));
            assert_eq!((cs.h.nodes.len(), cs.item_nodes), (45, made), "{id:#010x}: again, as it was");
            for (slot, humans) in [18usize, 27, 28].into_iter().enumerate() {
                let parent = cs.h.nodes[made[slot] as usize].parent as usize;
                let hp = human.parent[humans] as usize;
                assert_eq!(rig.refs[parent], human.refs[hp], "{id:#010x} slot {slot}: under the namesake of the human's node {hp}");
            }
            for action in [None, Some(23)] {
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
                let at = |n: i32| {
                    let m = cs.h.nodes[n as usize].world.to_cols();
                    glam::Vec3::new(m[3][0], m[3][1], m[3][2])
                };
                let scale = cs.h.default_scale.0;
                for (slot, &item) in made.iter().enumerate().take(3) {
                    let parent = cs.h.nodes[item as usize].parent;
                    let d = at(item).distance(at(parent));
                    assert!(d < 0.25 * scale, "{id:#010x} {action:?} slot {slot}: {d:.3} from its hand");
                }
                let knee = at(own[0]).distance(at(made[0]));
                eprintln!("{id:#010x} {action:?}: the body's own item node {knee:.3} from the hand's");
                if action.is_some() {
                    assert!(knee > 0.5, "{id:#010x} {action:?}: the control, its own item node {knee:.3} from the hand's");
                }
            }
        }
    }

    /// The merfolk (go-eqoa-server's race 32, the Mermaid) hold a player's
    /// items in their hands: on each of their four bodies `grant` makes three
    /// nodes, under the right wrist (17), the left wrist (12) and the left
    /// forearm (11), once only; treading water and in their attack (44) each
    /// sits at its parent, and the off hand's and the shield's are on the left
    /// of the body (+x in the model), the main hand's on the right. The
    /// control: the bodies' own nodes put the off hand's item in the right
    /// hand, with the main hand's. An NPC in the body keeps its own nodes.
    /// Needs `run/eqoa.iso`.
    #[test]
    fn the_merfolk_hold_items_in_both_hands() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        for &id in MERFOLK {
            let listed = races(&[[0xA316_EB78, 0x74BA_7C3D], [0xBF21_FEF5, 0x1740_427C], [0xD275_EE8F, 0xD275_EE8F]]);
            let (mut sc, mut cache, mut rand) = (AnimScene::<Ee>::default(), AnimCache::new(), Rand(1));
            let mut cs = load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut sc, &mut cache, &mut rand).unwrap();
            let own = cs.item_nodes;
            // the sea king (Quintashi's) names its head for both hands and its forearm for the shield
            let seaking = id == 0xD275_EE8F;
            assert_eq!((cs.h.nodes.len(), &own[..3]), (24, if seaking { &[20, 20, 11][..] } else { &[18, 18, -1][..] }), "{id:#010x}");
            let mut npc = load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut AnimScene::<Ee>::default(), &mut AnimCache::new(), &mut Rand(1)).unwrap();
            grant(&mut npc, id, false, Some(&listed));
            assert_eq!((npc.h.nodes.len(), npc.item_nodes), (24, own), "{id:#010x}: an NPC's as it was");
            assert!(grant(&mut cs, id, true, Some(&listed)), "{id:#010x}: listed or offered");
            let made = cs.item_nodes;
            assert_eq!(&made[..3], &[24, 25, 26], "{id:#010x}");
            assert_eq!([24, 25, 26].map(|n| cs.h.nodes[n].parent), [17, 12, 11], "{id:#010x}");
            assert!(grant(&mut cs, id, true, Some(&listed)));
            assert_eq!((cs.h.nodes.len(), cs.item_nodes), (27, made), "{id:#010x}: again, as it was");
            let bind_x = |n: i32| cs.h.nodes[n as usize].bind.trans.x.0;
            assert!(bind_x(made[0]) < 0.0 && bind_x(made[1]) > 0.0 && bind_x(made[2]) > 0.0, "{id:#010x}: right, left, left");
            assert!(seaking || bind_x(own[1]) < 0.0, "the control: the body's own off hand is its right");
            for action in [None, Some(23)] {
                let mut t = 0;
                cs.set_locomotion(13, &sc, &mut rand);
                while t <= 900 {
                    sc.time = t;
                    match action {
                        Some(a) if t == 0 => {
                            cs.set_action(a, &sc, &mut rand);
                        }
                        _ => {}
                    }
                    cs.h.process(0, &sc);
                    t += 16;
                }
                let at = |n: i32| {
                    let m = cs.h.nodes[n as usize].world.to_cols();
                    glam::Vec3::new(m[3][0], m[3][1], m[3][2])
                };
                let scale = cs.h.default_scale.0;
                for (slot, &item) in made.iter().enumerate().take(3) {
                    let parent = cs.h.nodes[item as usize].parent;
                    let d = at(item).distance(at(parent));
                    assert!(d < 0.25 * scale, "{id:#010x} {action:?} slot {slot}: {d:.3} from its parent");
                }
            }
        }
    }

    /// A merfolk player's hair is left out under a helm (`hide_hair`; the owner,
    /// 2026-11-12: "the mermaid helms arent sitting right"): on the merman, the
    /// mermaid and the sirens the hair's triangles go (the enchanting siren's
    /// tiara too) and nothing else of the body; the sea king, the human and a body
    /// of no race keep every triangle. Needs `run/eqoa.iso`.
    #[test]
    fn merfolk_hair_goes_under_a_helm() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        for (id, lo, hi) in [(0xA316_EB78u32, 100, 400), (0x74BA_7C3D, 150, 400), (0xBF21_FEF5, 150, 400), (0x1740_427C, 180, 450), (0xD275_EE8F, 0, 0), (0x70D8_98C6, 0, 0)] {
            let m = lib.build_model(lib.lookup(id as i32).unwrap()).unwrap();
            let mut h = m.clone();
            hide_hair(&mut h, id);
            let gone = (m.triangles - h.triangles) as usize;
            assert!((lo..=hi).contains(&gone), "{id:08X}: {gone} triangles of hair");
            let left: usize = h.batches.iter().map(|b| b.indices.len() / 3).sum();
            assert_eq!(left, h.triangles as usize, "{id:08X}: the count kept");
        }
    }

    /// A dryad player's hair is left out under a helm (`hide_hair`; the owner,
    /// 2026-11-16: "cll is a dryad with the helm not sitting right"): on the dryad
    /// her locks and tail (289 triangles of her atlas's hair box and her two
    /// strands), on her kin every triangle in their hair strips; nine in ten of the
    /// vertices gone are on the head or the neck (nodes 2 to 7, by the vertex's
    /// heaviest weight; the rest the dryad's tail, down her back to her shoulders),
    /// so nothing of the bark or the leaves goes. Needs `run/eqoa.iso`.
    #[test]
    fn dryad_hair_goes_under_a_helm() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        for (id, n) in [(0x70EF_CE05u32, 291), (0x21F3_11BB, 128), (0xACE7_1E26, 80), (0xB90A_DDD2, 418)] {
            let mut on = std::collections::BTreeMap::new();
            let m = lib.build_model(lib.lookup(id as i32).unwrap()).unwrap();
            let mut h = m.clone();
            hide_hair(&mut h, id);
            let gone = (m.triangles - h.triangles) as usize;
            assert_eq!(gone, n, "{id:08X}: triangles of hair");
            let left: usize = h.batches.iter().map(|b| b.indices.len() / 3).sum();
            assert_eq!(left, h.triangles as usize, "{id:08X}: the count kept");
            // each triangle gone, on the head or the neck
            for (a, b) in m.batches.iter().zip(&h.batches) {
                let mut kept: Vec<&[u32]> = b.indices.chunks_exact(3).collect();
                for t in a.indices.chunks_exact(3) {
                    if let Some(i) = kept.iter().position(|k| *k == t) {
                        kept.swap_remove(i);
                        continue;
                    }
                    for &v in t {
                        let (bo, w) = (m.bones[v as usize], m.weights[v as usize]);
                        let k = (0..4).max_by_key(|&k| w[k]).unwrap();
                        *on.entry(bo[k]).or_insert(0) += 1;
                    }
                }
            }
            // the dryad's tail lies on her shoulders (11, 20) and upper arms (12, 21)
            let all: usize = on.values().sum();
            let head: usize = on.range(2..=7).map(|(_, n)| n).sum();
            eprintln!("{id:08X}: the hair's vertices by node {on:?}");
            assert!(head * 100 >= all * 90, "{id:08X}: {head} of {all} of the hair's vertices on the head and the neck");
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
