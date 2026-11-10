//! Armour on a body whose CSprite has no text slots (a prototype: the user,
//! 2026-11-08, "Lay armour textures on", to be judged by its pictures).
//!
//! A body is dressed by its text slots (`vi::cust`): `SetArmorSlot` and
//! `SetRobe` put the armour's art and tint on the palette entry and layer the
//! CSprite's TSlotList (0x2930) names for each slot. Nine of the bodies
//! go-eqoa-server gives races of its own have no TSlotList at all (the kappa,
//! brownie, pixie, fairy, minotaur, dryad, skeleton, fungusman and gargoyle;
//! the hill giant too), so neither a PS2 nor this client drew anything they
//! wear. The disc has no armoured variant of them, and their materials are
//! atlases (one texture over the whole body, or a few that cut across the
//! slots), so no palette entry of theirs stands for a chest or a pair of legs.
//!
//! Here such a body wears the armour as a human does. Its torso, arms, hands,
//! legs and feet are found from its skeleton: the hands are the parents of
//! the item nodes its NodeIDList names (slots 0 and 1), the feet are where
//! the chains down to its footstep nodes (5 and 6) end, the head is the node
//! it names for the head (7); the chains between them and the root are the
//! arms, the legs and the spine, a leg's knee and ankle the joints nearest a
//! human's (so a hoofed leg's long bones fall as a human's do), the neck (the
//! spine above where the arms hang) the head's, as a human's is. Nodes off the
//! spine whose skin hugs the torso (a belly, a chest, a kilt's flaps) are the
//! torso's; ones that reach away from it (wings, a tail) are not. Each
//! triangle on the torso and the limbs is carried onto a human body
//! (HumanMale, HumanFemale for a female body) segment by segment (the spine,
//! the upper arm, the forearm, the hand, the thigh, the shin, the foot: along
//! the bone, and round it at the human's girth), and takes the text slot and
//! texture coordinates of the human's surface where it lands: the surface a
//! human shows in the same look (the human's mesh also carries a robe's skirt
//! and cuffs, which armour hides with see-through texels), and within one of
//! its UV islands per triangle, so no triangle spans two pieces of an armour
//! texture. The armour is then drawn over the body's own skin on those
//! triangles, a batch per slot with the human's slot layer (a human's armour
//! is a layer over its skin: a texel the human shows its skin through shows
//! the body's). The head, what reaches away from the torso, and translucent
//! or untextured parts keep the body's own look, and so do the parts of a
//! body's own model `KEPT` names (the fungusman's bag and its fungus).
//!
//! A triangle goes with the bones its corners are skinned to, but for one
//! weighted to the spine that runs along an arm's upper bone to the elbow
//! (`Rig::arm_along`): a body with no vertex on its upper arm (the
//! fungusman's skin runs from the shoulder, on the spine, straight to the
//! elbow) has its upper arm's triangles on the spine, and they are fitted to
//! the human's sleeve, not to its side.
//!
//! The skeleton's bones are too thin to carry armour laid on them: it wears
//! the human's armour itself (`SHELLED`, `Shell`), a human's surfaces for
//! each slot with art, on its bones (it is built on HumanMale's skeleton),
//! and its own bones under them are hidden; where nothing is worn they stay
//! bare.
//!
//! A body whose TSlotList names all five worn slots (every player race's) is
//! dressed by `vi::cust` exactly as before; one that names some keeps those as
//! `vi::cust` dresses them and is fitted for the ones it lacks alone
//! (`lacking`, `bare_part`). A player's helm hangs on the head of every body a
//! server's races wear that names no helm node of its own (`HEAD_SEATS`).
//! `set_enabled(false)` turns it off (for an Extended row: a puppet is made
//! anew when its look is applied).

use esf::{Era, Library};
use glam::Vec3;
use std::collections::HashMap;
use std::rc::Rc;
use vi::csprite::{CSprite, Rand};
use vi::cust::{LayerChange, Look, Tables};
use vi::fpu::Ee;

/// The palette entries the five made-up slots name: 0x5100 + slot. A
/// palette has a few dozen entries at most, so none is a body's own.
pub const ENTRY: i32 = 0x5100;

/// The worn slots (chest, bracer, gloves, legs, boots).
const SLOTS: usize = 5;

/// A human's knee and ankle, as fractions of the way from the hip joint
/// down to the end of the leg (HumanMale's: 1.02, 0.58, 0.12, 0).
const KNEE: f32 = 0.43;
const ANKLE: f32 = 0.88;

/// A node off the spine is the torso's when nine in ten of the vertices on
/// it are no farther from the spine than this times the torso's own skin at
/// that height (a chest's, a belly's and a kilt's are within it; a bee's
/// abdomen, a tail and wings reach past it).
const HUG: f32 = 1.2;

/// A spine triangle that reaches the elbow (a corner this fraction of the
/// way from the shoulder to the elbow, or past it) is an arm's when its
/// middle lies past `ARM_ALONG` of the way (`Rig::arm_along`).
const ELBOW: f32 = 0.9;
const ARM_ALONG: f32 = 0.25;

/// Parts of a body's own model that keep its look under armour, as the
/// palette entries they are drawn in, by body (its CHAR.ESF DictID): parts
/// no rule tells apart from the skin, each one the user named. The
/// fungusman's (the user, 2026-11-08: "the bag that is part of the model if
/// we could not cover that with armor, but the body still"; "the fungus that
/// protrudes to not take armor texture"): entry 0 its fungus, the cap and the
/// growths on its shoulders, wrists and shins and its claws, drawn with a glow
/// over the skin's own texture; entry 2 its bag and strap, a texture of their
/// own. Neither makes a rule: ten other slotless bodies on the disc draw a
/// glow over a texture their skin also shows plain, some over the whole body
/// (0x8A416240's 750 triangles), and a texture of a limb's own is as often its
/// skin (the dryad's hands and feet).
const KEPT: &[(u32, &[i32])] = &[(0x4A4D88FB, &[0, 2])];

/// The palette entries body `id` keeps its own look on (`KEPT`).
fn kept(id: i32) -> &'static [i32] {
    KEPT.iter().find(|(b, _)| *b == id as u32).map_or(&[], |(_, e)| e)
}

/// Bodies that wear armour as a human does, round them, not laid on their
/// own triangles: the skeleton's, whose bones are too thin to carry it (the
/// user, 2026-11-08: "lets fit armor on skeletons like humans or similar,
/// dont make the armor model formfitting"). It is built on the human's own
/// skeleton (HumanMale's forty nodes in the same bind pose), so it wears the
/// human's armour surfaces themselves (`Shell`), skinned to the same nodes;
/// on any other skeleton a body listed here is dressed as the others are. The
/// other three are the skeletons a player of the server's Skeleton race may
/// choose instead (its options, `raceopts`: old, mossy and frost bone), each on
/// the same skeleton, two drawn a tenth larger. The last four are the great
/// skeletons it may choose too (great bone, old bone, mossy and frost; mossy's
/// copy 0x556EA369, the same mesh, textures and clips, is no option), built on
/// the troll's skeleton (`SHELL_BODIES`), which wear the troll's.
const SHELLED: &[u32] = &[0x6AF98A81, 0x45A8B064, 0x198BAA64, 0x648DFC11, 0x6AF98A82, 0x197F51C7, 0x90F16A6F, 0x7D9F41B2];

/// Bodies whose helm goes on their node of the human's helm node's name (a child of
/// the head), else on the head node their NodeIDList names (slot 7), where they name
/// no helm node (slot 8): the Jal'Raeth's three (go-eqoa-server's race 30,
/// 2026-11-09: robes, channeler, knight; HumanMale's helm node 5 is their node 6), a
/// player's only; an NPC keeps none, as a PS2 draws it.
const HEAD_HELMS: &[u32] = &[0xF51B_AB09, 0x4B24_FF7E, 0x491F_6347];

/// The node a seat's helm hangs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// The body's node of the human's helm node's name (`named_helm_node`): a
    /// child of its head at the top of it, where SOE's 2002 goblins and orcs
    /// named their helm node (Frontiers' bodies keep the node and name none).
    Named,
    /// The node the body names for its head (NodeIDList slot 7).
    Head,
}

/// How a player's helm sits on a body one of the server's races may wear that
/// names no helm node of its own: the node it hangs on, and the seat past it
/// (`HelmFit`: the helm, as it sits on a human's helm node, stretched about the
/// node and moved, in the node's frame; every head node and named node of these
/// bodies has the model's axes in the bind pose, as the human's helm node has,
/// so the seat is a stretch and a move alone). A family's bodies share a
/// skeleton and a head.
#[derive(Clone, Copy, Debug)]
pub struct HeadSeat {
    pub bodies: &'static [u32],
    pub anchor: Anchor,
    pub scale: [f32; 3],
    pub offset: [f32; 3],
}

/// The seats (`HeadSeat`), by family, judged by eye with `eqoa dress <body> ...
/// helm=4 frame=head helmseat=...` (the contact sheets in the lane's notes):
/// the head inside the helm as a human's is, the face out of its opening, a
/// snout, a beak or a muzzle through the front, ears, horns and crests through
/// the sides and top. Where a body has a node of the human's helm node's name
/// at the top of its head (a goblin, an orc, a gargoyle, a giant, a brownie, a
/// dryad) it hangs there; the goblins' and the orcs' at SOE's own 2002 scale. On
/// the lizardman's skeleton (the gnolls and minotaurs share it), the kobold's
/// and the ratman's that node is at the neck or under the jaw, and the
/// froglok's, kerran's, aviak's, kappa's, pixie's, fairy's and satyr's have
/// none: those hang on the head node, moved up to the top of the head. The
/// fungusmen have none here: their cap is their head, and a helm either hides
/// under it or swallows the body (`HEADLESS`).
pub const HEAD_SEATS: &[HeadSeat] = &[
    // the giants (hill, ice, sand, fire, cyclops, the reaver and the king; the
    // women): the head four times a human's, the named node at the crown
    HeadSeat { bodies: &[0xFFB1_EA2C, 0x8117_C627, 0xE617_303B, 0x72A8_FDBB, 0x7F26_04DF, 0x05F5_9B65, 0x2D68_6972, 0xBF6A_DFDB, 0xF345_01CB, 0x286C_6E9E, 0x8E2A_6552, 0xE308_27CC], anchor: Anchor::Named, scale: [4.2; 3], offset: [0.0, 0.30, -0.05] },
    // the goblins: SOE's own seat, as the 2002 disc's goblins name their helm
    // node (the named node) and scale it 0.9
    HeadSeat { bodies: GOBLINS, anchor: Anchor::Named, scale: [0.9; 3], offset: [0.0, 0.0, 0.0] },
    // the gargoyles, their heads on the goblin's skeleton's
    HeadSeat { bodies: &[0xB4D2_D132, 0x4E2A_945C, 0xAEC5_D9F6, 0x0013_55C8, 0x6D95_9FD9, 0x9304_BAB5], anchor: Anchor::Named, scale: [1.1; 3], offset: [0.0, 0.04, 0.0] },
    // the orcs on the orc's skeleton: SOE's 2002 orc's (its helm node the named
    // node, scaled 1.19)
    HeadSeat { bodies: &[0x8214_A32D, 0x84F4_7767, 0x656D_D4C5, 0xB2FB_D7EF, 0x0A03_EB92, 0xDA51_5962], anchor: Anchor::Named, scale: [1.19; 3], offset: [0.0, 0.0, 0.0] },
    // the Deathfist orcs, on HumanMale's and HumanFemale's skeletons (the fire
    // weaver names a helm node of its own)
    HeadSeat { bodies: &[0x37FD_13F9, 0x3198_F5B1, 0x0205_D208, 0xC76F_1FA8, 0xE144_942F], anchor: Anchor::Named, scale: [1.05; 3], offset: [0.0, 0.0, 0.0] },
    // the brownies, on HumanMale's skeleton (the 2002 disc's on its own 2002
    // HumanMale's), a head more than half again a human's
    HeadSeat { bodies: &[0x4376_EB52, 0x4946_8331], anchor: Anchor::Named, scale: [1.7; 3], offset: [0.0, 0.16, -0.02] },
    // the dryads, on HumanFemale's
    HeadSeat { bodies: &[0x70EF_CE05, 0x21F3_11BB, 0xACE7_1E26, 0xB90A_DDD2], anchor: Anchor::Named, scale: [1.0; 3], offset: [0.0, 0.0, 0.0] },
    // the lizardmen, gnolls and minotaurs, one skeleton
    HeadSeat { bodies: &[0x9112_20E2, 0xD45B_9F1F, 0x2C1D_8724, 0x7468_EAE7, 0x16BB_9E24, 0xEEBF_6DA5], anchor: Anchor::Head, scale: [1.4; 3], offset: [0.0, 0.36, 0.10] },
    HeadSeat {
        bodies: &[
            0xCF41_5561, 0x223F_D6BC, 0xF5F3_A450, 0x097B_FC70, 0xB7B2_1DFC, 0x4A9B_AB9E, 0x17DA_0FEB, 0x7492_678E, 0x585E_5ED9, 0x16BA_0BD3, 0x3C6D_3677, 0xC2D7_DD2F, 0x71F1_63CA, 0xC3B9_4FA5, 0x147A_C3EE, 0x09E0_AE7D, 0xB156_28DA, 0xCE06_735E, 0x2EDA_50DC,
            0x16E3_20F7,
        ],
        anchor: Anchor::Head,
        scale: [1.45; 3],
        offset: [0.0, 0.35, 0.10],
    },
    HeadSeat { bodies: &[0xC5CB_F4CF, 0x0AE7_1D02], anchor: Anchor::Head, scale: [1.7; 3], offset: [0.0, 0.40, 0.14] },
    // the kobolds and the ratmen
    HeadSeat { bodies: &[0xCFC4_78EE, 0x6B9B_7016, 0x5973_25B5, 0x9CDF_3C2D], anchor: Anchor::Head, scale: [1.75; 3], offset: [0.0, 0.30, 0.05] },
    HeadSeat { bodies: &[0xBADC_25C9, 0x0ECC_836A, 0x133A_79CA, 0x46BA_26A9], anchor: Anchor::Head, scale: [1.2; 3], offset: [0.0, 0.22, 0.04] },
    // the frogloks: a head three times a human's across and half again as tall
    HeadSeat { bodies: &[0x141B_FE0C, 0x429A_04AB, 0x063C_5D97, 0x0F23_9012, 0x6F17_325E, 0x1663_EC1C], anchor: Anchor::Head, scale: [2.4, 1.6, 2.0], offset: [0.0, 0.48, 0.12] },
    HeadSeat {
        bodies: &[0x2217_6258, 0x4D9C_53EF, 0x77A7_E7B8, 0x78FA_8F20, 0x0DDD_4087, 0x5D40_C8AB, 0xB749_BF57, 0xB620_92E4, 0xE700_DE53, 0x39FE_AA5F, 0x7654_AC3C, 0xD8E0_F980],
        anchor: Anchor::Head,
        scale: [1.05; 3],
        offset: [0.0, 0.23, 0.05],
    },
    // the aviaks: the hens' heads (the hawk's and the green robe's) smaller
    HeadSeat { bodies: &[0x3151_DF13, 0x262C_572A, 0xDC64_1055, 0x41EF_F500, 0x3C3D_E6CA, 0xC04C_EC07, 0xB1EE_B204, 0x8B33_2785, 0x246B_5FED, 0x5585_143C], anchor: Anchor::Head, scale: [1.6; 3], offset: [0.0, 0.24, 0.08] },
    HeadSeat { bodies: &[0x8EFD_12EA, 0xF83B_D167], anchor: Anchor::Head, scale: [1.4; 3], offset: [0.0, 0.20, 0.04] },
    HeadSeat { bodies: &[0x31A1_5F17, 0xC3EC_4F58, 0xF674_7024, 0xF0FA_B933], anchor: Anchor::Head, scale: [1.0; 3], offset: [0.0, 0.19, 0.04] },
    HeadSeat { bodies: &[0x94BA_77CD, 0x7A85_646C], anchor: Anchor::Head, scale: [0.3; 3], offset: [0.0, 0.10, 0.007] },
    HeadSeat { bodies: &[0xBE10_226F, 0x097A_DB18], anchor: Anchor::Head, scale: [1.1; 3], offset: [0.0, 0.28, 0.03] },
];

/// The bodies a server's race may wear that have no head a helm can sit on:
/// the fungusmen's three, whose head is their cap (a helm hangs under it, out
/// of sight, or swallows the body to the waist). They wear none, as before.
#[cfg(test)]
pub const HEADLESS: &[u32] = &[0x4A4D_88FB, 0x5874_ACFC, 0xE8B0_ADC8];

/// The goblins' bodies: go-eqoa-server's race 13's own (0x6C1308E0) and every
/// one its options offer (its tribes, robed and not, and their chiefs).
pub const GOBLINS: &[u32] = &[
    0x6C13_08E0, 0x581B_6A80, 0x9637_1804, 0xBCDF_0C04, 0xE961_79D4, 0xA5F9_2F43, 0x6C7C_07ED, 0xBE92_7B85, 0xCEF5_874A, 0xEDB4_E0AD, 0x687E_7645, 0x2ECB_B95A, 0xB656_DFEA, 0xEC08_85EB, 0x1F8E_11D3, 0x3AB9_6672, 0xF54C_2142, 0x1245_E56B, 0x27A4_E554, 0x5937_EA22, 0x507C_4A52,
    0xEE1D_DFBA, 0x1CCD_F0EF, 0x2F34_B867,
];

/// Body `id`'s helm seat, when a server's race may wear it (`HEAD_SEATS`).
pub fn head_seat(id: u32) -> Option<&'static HeadSeat> {
    HEAD_SEATS.iter().find(|s| s.bodies.contains(&id))
}

/// The node a seat's helm hangs on, on body `id` (`cs` its sprite): its named node
/// or its head's; none when it has no such node.
fn seat_node(lib: &Library, id: u32, cs: &CSprite<Ee>, seat: &HeadSeat) -> Option<i32> {
    match seat.anchor {
        Anchor::Named => named_helm_node(lib, id, cs.sex != 0),
        Anchor::Head => Some(cs.node_ids[7]).filter(|&n| n >= 0),
    }
}

/// How body `id`'s helm is seated past the node `seat_node` names: a player's
/// in a body of `HEAD_SEATS` that names no helm node of its own, in the art it
/// is drawn in (the 2002 disc's goblins and some of its orcs name one, SOE's);
/// none for any other body, an NPC, or with the dressing turned off.
pub fn head_fit(id: u32, cs: &CSprite<Ee>, player: bool) -> Option<HelmFit> {
    if !player || !enabled() || cs.node_ids[8] >= 0 {
        return None;
    }
    head_seat(id).map(|s| HelmFit { scale: Vec3::from(s.scale), offset: Vec3::from(s.offset) })
}

/// Body `id`'s node of the same ref id as the human's helm node (HumanFemale's for
/// `female`, else HumanMale's: both name their helm node alike, and the pregame's
/// library, which holds the bodies it cuts out of CHAR.ESF and the game's own
/// races', may have no HumanFemale).
pub(crate) fn named_helm_node(lib: &Library, id: u32, female: bool) -> Option<i32> {
    let br = crate::raceemote::Rig::load(lib, id)?;
    let names: &[&str] = if female { &["HumanFemale", "HumanMale"] } else { &["HumanMale"] };
    names.iter().find_map(|name| {
        let hid = vi::hash::hash_resource_id(name);
        let h = Human::load_id(lib, hid)?;
        let hr = crate::raceemote::Rig::load(lib, hid)?;
        let r = hr.refs.get(usize::try_from(h.cs.node_ids[8]).ok()?).copied().flatten()?;
        br.by_ref.get(&r).map(|&n| n as i32)
    })
}

/// The player bodies other than the human's whose armour a body built on their
/// skeleton wears (`SHELLED`), in the order tried after the human's: the
/// troll's two males. The great skeletons are built on it: its forty-three
/// nodes and parents, every one in the same bind pose but the top of the skull
/// (0.5 to 1 cm off), which no armour is skinned to.
const SHELL_BODIES: &[u32] = &[0x4C6F_A532, 0xF41C_4DBA];

/// Whether a material is opaque and textured (not a wing's translucent
/// membrane, not a painted eye).
fn opaque(b: &esf::Batch) -> bool {
    b.layers.first().is_some_and(|l| l.blend == 0 && l.texture != 0)
}

/// Which of `model`'s triangles can wear armour, per batch: those of its
/// opaque materials, but the parts the body keeps (`kept`). Of a kept entry
/// whose texture the skin shares, a triangle drawn in the skin's colours
/// (its texels' mean nearer the skin's triangles' than the part's own, by
/// `colours`, a texture's texels) is the skin's: the fungusman's fungus
/// material also draws one strip of its right upper arm in the skin's bark,
/// and that strip wears armour as the arm round it does.
fn wearing(model: &esf::Model, kept: &[i32], colours: impl Fn(i32) -> Option<Rc<Texels>>) -> Vec<Vec<bool>> {
    let uv = |b: &esf::Batch, t: &[u32]| [0, 1, 2].map(|k| b.layers[0].apply_uv(model.uv[t[k] as usize]));
    // per texture a kept entry is drawn in: its texels, and the mean colour
    // of the part's triangles and of the skin's
    let mut means: HashMap<i32, (Rc<Texels>, Vec3, Vec3)> = HashMap::new();
    for b in model.batches.iter().filter(|b| opaque(b) && kept.contains(&b.material)) {
        let tex = b.layers[0].texture;
        if means.contains_key(&tex) {
            continue;
        }
        let Some(texels) = colours(tex) else { continue };
        let mean = |part: bool| {
            let all: Vec<Vec3> = model.batches.iter().filter(|o| opaque(o) && o.layers[0].texture == tex && kept.contains(&o.material) == part).flat_map(|o| o.indices.chunks_exact(3).map(|t| texels.mean(&uv(o, t)))).collect();
            (!all.is_empty()).then(|| all.iter().sum::<Vec3>() / all.len() as f32)
        };
        if let (Some(part), Some(skin)) = (mean(true), mean(false)) {
            means.insert(tex, (texels.clone(), part, skin));
        }
    }
    model
        .batches
        .iter()
        .map(|b| {
            b.indices
                .chunks_exact(3)
                .map(|t| {
                    if !opaque(b) {
                        return false;
                    }
                    if !kept.contains(&b.material) {
                        return true;
                    }
                    means.get(&b.layers[0].texture).is_some_and(|(texels, part, skin)| {
                        let c = texels.mean(&uv(b, t));
                        c.distance(*skin) < c.distance(*part)
                    })
                })
                .collect()
        })
        .collect()
}

/// A texture's colours, as decoded.
pub struct Texels {
    w: usize,
    h: usize,
    rgb: Vec<[u8; 3]>,
}

impl Texels {
    /// The mean colour of a triangle's points (`Alpha::points`), the
    /// texture repeated.
    fn mean(&self, uv: &[[f32; 2]; 3]) -> Vec3 {
        let pts = Alpha::points(uv);
        pts.iter()
            .map(|p| {
                let x = (p[0].rem_euclid(1.0) * self.w as f32) as usize % self.w;
                let y = (p[1].rem_euclid(1.0) * self.h as f32) as usize % self.h;
                Vec3::from(self.rgb[y * self.w + x].map(f32::from))
            })
            .sum::<Vec3>()
            / pts.len() as f32
    }
}

/// Texture `id`'s colours in the library's view now; None when it does not
/// resolve or decode.
fn colours_of(lib: &Library, id: i32) -> Option<Rc<Texels>> {
    let key = (id, lib.view());
    if let Some(t) = TEXELS.with(|t| t.borrow().get(&key).cloned()) {
        return t;
    }
    let made = lib.surface(id).and_then(|(c, s)| s.decode(&c.data)).map(|img| Rc::new(Texels { w: img.width as usize, h: img.height as usize, rgb: img.rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect() }));
    TEXELS.with(|t| t.borrow_mut().insert(key, made.clone()));
    made
}

thread_local! {
    /// Whether slotless bodies are dressed: the thread's that makes the
    /// puppets and figures (the game's), so one test turning it off is not
    /// seen by another running beside it.
    static ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Whether slotless bodies are dressed (on unless turned off).
pub fn enabled() -> bool {
    ENABLED.with(|e| e.get())
}

/// Turns the dressing of slotless bodies on or off, for the puppets made
/// from then on.
#[allow(dead_code)]
pub fn set_enabled(on: bool) {
    ENABLED.with(|e| e.set(on));
}

/// A body with no text slot at all: no TSlotList, or one that names none.
pub fn is_slotless(cs: &CSprite<Ee>) -> bool {
    cs.text_slots.iter().all(|s| s.0 == -1)
}

/// The text slots `vi::cust::apply_look_slots` dresses a slotless body by:
/// the five worn slots on the made-up entries, layer 0, nothing else (no
/// hair, no face).
pub fn slots() -> [(i32, i32); 19] {
    let mut s = [(-1, -1); 19];
    for (i, slot) in s.iter_mut().enumerate().take(SLOTS) {
        *slot = (ENTRY + i as i32, 0);
    }
    s
}

/// Each made-up slot's art after `changes` (the last texture given to it,
/// as on a body's own layers), and its colour.
fn worn(changes: &[LayerChange]) -> [(Option<u32>, Option<[u8; 4]>); SLOTS] {
    let mut out = [(None, None); SLOTS];
    for ch in changes {
        if let Some(o) = usize::try_from(ch.entry - ENTRY).ok().filter(|&i| i < SLOTS && ch.layer == 0).map(|i| &mut out[i]) {
            o.0 = ch.texture.or(o.0);
            o.1 = ch.color.or(o.1);
        }
    }
    out
}

/// The worn slots (chest, bracer, gloves, legs, boots) body `cs` draws nothing
/// for (`model` built from it): its TSlotList names no palette entry for the
/// slot, or the entry it names has no triangle with the layer it names. All
/// five on a slotless body, none on a player race's.
pub fn lacking(model: &esf::Model, cs: &CSprite<Ee>) -> [bool; SLOTS] {
    std::array::from_fn(|i| {
        let (entry, layer) = cs.text_slots[i];
        entry == -1 || !model.batches.iter().any(|b| b.material == entry && usize::try_from(layer).is_ok_and(|l| l < b.layers.len()) && !b.indices.is_empty())
    })
}

/// Where the look puts the armour on a slotless body: `model` (the body as
/// `build_model` made it, `id` its DictID) gains a batch for each slot the
/// look gives art to, drawn over the body's own triangles there; a body that
/// wears a human's armour (`SHELLED`) gains the human's surfaces there
/// instead, and loses its own triangles under them. A body whose TSlotList
/// names some of the five worn slots and not the others (`lacking`: most of
/// the creatures' bodies the server's races may choose, the froglok's, the
/// kerran's, the aviak's, the gnoll's, the orc's ...: a chest, bracers and
/// legs, no gloves or boots) keeps its own as `vi::cust` dresses them, as a
/// PS2 does, and gains the ones it lacks as a slotless body would, over the
/// triangles none of its own worn slots' entries draw (`bare_part`). False
/// when nothing was added (no art worn, a body this cannot fit, one with all
/// five slots of its own, or turned off).
/// The look's textures resolve as `has` says, by the tables `t`, as
/// `vi::cust::apply_look_with` dresses a body with slots.
#[allow(clippy::too_many_arguments)]
pub fn dress(lib: &Library, id: i32, model: &mut esf::Model, cs: &CSprite<Ee>, look: &Look, helm_worn: bool, has: impl Fn(u32) -> bool, t: &Tables) -> bool {
    if !enabled() {
        return false;
    }
    let slotless = is_slotless(cs);
    let lacks = lacking(model, cs);
    if !slotless && !lacks.contains(&true) {
        return false;
    }
    let changes = vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, look, helm_worn, &has, t);
    let art = worn(&changes).map(|(t, _)| t);
    if art.iter().all(Option::is_none) {
        return false;
    }
    // the legs in a robe's lower half (the same look less the robe dresses
    // them otherwise)
    let unrobed = vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, &Look { robe: -1, ..*look }, helm_worn, &has, t);
    let robed = art[3].is_some() && worn(&unrobed)[3].0 != art[3];
    let fit = fit_of(lib, id, model, cs, art, robed);
    if !slotless {
        let Some(fit) = fit else { return false };
        return apply(model, &bare_part(model, cs, &fit, lacks), &changes);
    }
    if SHELLED.contains(&(id as u32))
        && let Some(shell) = shell_of(lib, id, cs, art)
    {
        // the body's own triangles under the armour are hidden (a skeleton's
        // hips and finger bones reach out through a human's)
        if let Some(fit) = &fit {
            hide(lib, model, fit, &shell, art);
        }
        return put_on(model, &shell, &changes);
    }
    if NAMED_SHELLS.contains(&(id as u32))
        && let Some(shell) = named_shell_of(lib, id, cs, art)
    {
        return put_on(model, &shell, &changes);
    }
    let Some(fit) = fit else { return false };
    apply(model, &fit, &changes)
}

/// Bodies that wear a human's chest, bracer and gloves carried onto their own
/// skeleton by its bone names (`named_shell_of`): the Jal'Raeth's three
/// (go-eqoa-server's race 30, 2026-11-09), whose upper body has the human's names
/// (spine, arms, hands, head) and whose legs float crossed under a robe with names
/// of their own. Their item and footstep nodes hang off the root, so the fit by limbs
/// finds no hands on them, and no legs or boots are worn there.
const NAMED_SHELLS: &[u32] = &[0xF51B_AB09, 0x4B24_FF7E, 0x491F_6347];

/// The slots a named shell carries: chest, bracer, gloves.
const NAMED_SLOTS: usize = 3;

/// The human's armour for one look carried onto body `id` (`cs` its sprite) by
/// bone names: each corner of the human's shell (`Shell::of`) whose bones all have
/// a node of the same ref id on the body takes those nodes, and its place and
/// normal are carried from the human's bind pose to the body's (each bone's bind
/// on the body times its inverse bind on the human, blended by the weights), so
/// the armour hugs the body's own spine and arms at their lengths; a triangle with
/// a corner on a bone the body lacks (a leg's) is left out, and so are the legs and
/// boots (`NAMED_SLOTS`). None without a human or with nothing carried.
fn named_shell_of(lib: &Library, id: i32, cs: &CSprite<Ee>, art: Art) -> Option<Rc<Shell>> {
    let key = (id, lib.view(), art);
    if let Some(s) = NAMED.with(|s| s.borrow().get(&key).cloned()) {
        return s;
    }
    let made = (|| {
        let h = Human::load(lib, cs.sex != 0)?;
        let hid = if cs.sex != 0 { vi::hash::hash_resource_id("HumanFemale") } else { vi::hash::hash_resource_id("HumanMale") };
        let (hr, br) = (crate::raceemote::Rig::load(lib, hid)?, crate::raceemote::Rig::load(lib, id as u32)?);
        let bind = |n: &vi::hsprite::Node<Ee>| glam::Mat4::from_cols_array_2d(&n.inv_bind_world.to_cols());
        // human node -> (body node, body bind times human inverse bind)
        let map: Vec<Option<(u8, glam::Mat4)>> = (0..h.cs.h.nodes.len())
            .map(|i| {
                let j = hr.refs.get(i).copied().flatten().and_then(|r| br.by_ref.get(&r).copied())?;
                let (hn, bn) = (h.cs.h.nodes.get(i)?, cs.h.nodes.get(j)?);
                Some((u8::try_from(j).ok()?, bind(bn).inverse() * bind(hn)))
            })
            .collect();
        let full = Shell::of(lib, &h, art);
        let mut tris: [Vec<[Corner; 3]>; SLOTS] = Default::default();
        for (slot, out) in tris.iter_mut().enumerate().take(NAMED_SLOTS) {
            'tri: for t in &full.tris[slot] {
                let mut c3 = *t;
                for c in &mut c3 {
                    let (mut p, mut nrm, mut sum) = (Vec3::ZERO, Vec3::ZERO, 0.0);
                    for k in 0..4 {
                        if c.weights[k] == 0 {
                            continue;
                        }
                        let Some(Some((j, m))) = map.get(c.bones[k] as usize) else { continue 'tri };
                        let w = c.weights[k] as f32;
                        p += m.transform_point3(Vec3::from(c.pos)) * w;
                        nrm += m.transform_vector3(Vec3::from(c.normal)) * w;
                        sum += w;
                        c.bones[k] = *j;
                    }
                    if sum <= 0.0 {
                        continue 'tri;
                    }
                    c.pos = (p / sum).to_array();
                    c.normal = nrm.normalize_or_zero().to_array();
                }
                out.push(c3);
            }
        }
        tris.iter().any(|t| !t.is_empty()).then(|| Rc::new(Shell { tris, layers: full.layers }))
    })();
    NAMED.with(|s| {
        let mut s = s.borrow_mut();
        if s.len() >= 256 {
            s.clear();
        }
        s.insert(key, made.clone());
    });
    made
}

/// Takes out of `model`'s own batches its triangles under the shell's
/// armour: those the fit lays in a slot the shell puts armour in, where the
/// slot's art (`art`) is opaque; where a human would show its skin through
/// the armour the body shows its own (a skeleton, its bones).
fn hide(lib: &Library, model: &mut esf::Model, fit: &Fit, shell: &Shell, art: Art) {
    let mut gone: std::collections::HashSet<([u32; 3], Option<u8>)> = std::collections::HashSet::new();
    for i in (0..SLOTS).filter(|&i| !shell.tris[i].is_empty()) {
        let alpha = art[i].and_then(|t| alpha_of(lib, t as i32));
        gone.extend(fit.tris[i].iter().filter(|t| shows(&alpha, &t.uv)).map(|t| (t.v, t.lod)));
    }
    if gone.is_empty() {
        return;
    }
    for b in model.batches.iter_mut() {
        let before = b.indices.len();
        b.indices = b.indices.chunks_exact(3).filter(|t| !gone.contains(&([t[0], t[1], t[2]], b.lod))).flatten().copied().collect();
        model.triangles -= ((before - b.indices.len()) / 3) as u32;
    }
}

/// The layer changes on the made-up slots put on `model` as batches: per
/// slot with art, its triangles again (their own vertices, the human's
/// texture coordinates) with the human's slot layer in the slot's texture
/// and colour.
pub fn apply(model: &mut esf::Model, fit: &Fit, changes: &[LayerChange]) -> bool {
    let mut added = false;
    for (slot, (tex, color)) in worn(changes).into_iter().enumerate() {
        // no art for the slot (no set, or a set whose art is not there): the
        // body's own skin, as a human's slot keeps its own texture
        let Some(tex) = tex else { continue };
        let tris = &fit.tris[slot];
        if tris.is_empty() {
            continue;
        }
        let mut layer = fit.layers[slot].clone();
        layer.texture = tex as i32;
        if let Some(c) = color {
            layer.color = c;
        }
        let mut lods: Vec<Option<u8>> = tris.iter().map(|t| t.lod).collect();
        lods.sort();
        lods.dedup();
        for lod in lods {
            let mut indices = Vec::new();
            for t in tris.iter().filter(|t| t.lod == lod) {
                for k in 0..3 {
                    let v = t.v[k] as usize;
                    indices.push(model.pos.len() as u32);
                    model.pos.push(model.pos[v]);
                    model.uv.push(t.uv[k]);
                    model.normal.push(model.normal[v]);
                    model.color.push(model.color[v]);
                    model.bones.push(model.bones[v]);
                    model.weights.push(model.weights[v]);
                }
            }
            model.triangles += indices.len() as u32 / 3;
            model.batches.push(esf::Batch { material: ENTRY + slot as i32, layers: vec![layer.clone()], indices, lod });
        }
        added = true;
    }
    added
}

/// The part of `fit` (body `cs`'s, `model` built from it, fitted whole as a
/// slotless body is: its rig, girths and slots read off all its skin) that a
/// body with some worn slots of its own wears: the slots it lacks (`lacks`),
/// on the triangles of no palette entry its own worn slots name. A triangle
/// its own slot dresses keeps that slot's art, as on a PS2, even where a
/// human would wear another piece there (a froglok's feet in its legs' entry
/// wear its legs); one on its skin takes the piece the slot it lacks puts
/// there (its hands the gloves, its feet the boots), and one the fit gives a
/// slot it has stays its own.
fn bare_part(model: &esf::Model, cs: &CSprite<Ee>, fit: &Fit, lacks: [bool; SLOTS]) -> Fit {
    let own: Vec<i32> = (0..SLOTS).filter(|&i| !lacks[i]).map(|i| cs.text_slots[i].0).collect();
    let owned: std::collections::HashSet<([u32; 3], Option<u8>)> =
        model.batches.iter().filter(|b| own.contains(&b.material)).flat_map(|b| b.indices.chunks_exact(3).map(move |t| ([t[0], t[1], t[2]], b.lod))).collect();
    Fit { tris: std::array::from_fn(|i| if lacks[i] { fit.tris[i].iter().filter(|t| !owned.contains(&(t.v, t.lod))).cloned().collect() } else { Vec::new() }), layers: fit.layers.clone() }
}

/// A human's armour for one look, to put on a body whose skeleton is the
/// human's (`SHELLED`): per slot with art, the triangles of the human's slot
/// material that show in it (any of their points on an opaque texel: the
/// rest, a robe's skirt that armour hides, is left out), each corner as the
/// human has it (place, normal, colour, bones and weights) with the texture
/// coordinates its slot layer samples; and the human's layer per slot.
#[derive(Clone, Debug)]
pub struct Shell {
    pub tris: [Vec<[Corner; 3]>; SLOTS],
    pub layers: [esf::Layer; SLOTS],
}

/// A corner of a human's armour triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub bones: [u8; 4],
    pub weights: [u8; 4],
}

/// The shell of body `id` (`cs` its sprite) wearing `art`, made once per
/// body, art and look: of the first player body whose skeleton its is (the
/// human's, else one of `SHELL_BODIES`); None when it is none of theirs.
fn shell_of(lib: &Library, id: i32, cs: &CSprite<Ee>, art: Art) -> Option<Rc<Shell>> {
    let key = (id, lib.view(), art);
    if let Some(s) = SHELLS.with(|s| s.borrow().get(&key).cloned()) {
        return s;
    }
    let made = Human::shell_sources(lib, cs.sex != 0).find_map(|h| {
        let shell = Shell::of(lib, &h, art);
        same_skeleton(&h.cs, cs, shell.tris.iter().flatten().flatten().flat_map(|c| (0..4).filter(|&k| c.weights[k] > 0).map(|k| c.bones[k] as usize))).then(|| Rc::new(shell))
    });
    SHELLS.with(|s| {
        let mut s = s.borrow_mut();
        if s.len() >= 256 {
            s.clear();
        }
        s.insert(key, made.clone());
    });
    made
}

/// The node body `id` (`cs` its sprite, in the art it is drawn in) wears its
/// helm on (`SetHelm`'s, the NodeIDList's slot 8; -1 none): the one it names;
/// else, for a player's character (`player`) in a body that wears a human's
/// armour (`SHELLED`) on the human's own skeleton (the same nodes, the same
/// parents), the human's (on the troll's, the troll's). The 2002 disc's skeleton is built on its HumanMale's
/// skeleton (52 nodes, its head node 6) and names no helm node, where
/// Frontiers' names the human's own (5): in the 2002 disc's art a skeleton's
/// helm was not drawn (the user, 2026-11-08: "in char select i see skinny with
/// his helm on, but in game the helm doesnt show"). An NPC, any other body,
/// and the dressing turned off keep the body's word, as a PS2 draws it.
pub fn helm_node(lib: &Library, id: u32, cs: &CSprite<Ee>, player: bool) -> i32 {
    let own = cs.node_ids[8];
    // a body that names no helm node, a player's (`HEAD_HELMS`): its node of the
    // human's helm node's name, else its head's
    if own < 0 && player && enabled() && HEAD_HELMS.contains(&id) {
        return named_helm_node(lib, id, cs.sex != 0).unwrap_or(cs.node_ids[7]);
    }
    // a body a server's race may wear (`HEAD_SEATS`): its seat's node
    if own < 0
        && player
        && enabled()
        && let Some(n) = head_seat(id).and_then(|s| seat_node(lib, id, cs, s))
    {
        return n;
    }
    if own >= 0 || !player || !enabled() || !SHELLED.contains(&id) {
        return own;
    }
    Human::shell_sources(lib, cs.sex != 0).find(|h| same_skeleton(&h.cs, cs, std::iter::empty())).map_or(own, |h| h.cs.node_ids[8])
}

/// The helm body `id` (`cs` its sprite, `player` a player's character) wears
/// in armour set `set` and where (`SetHelm`): the set's helm sprite (`GetHelm`
/// by the tables `t`) built in the library's view now, every layer of it in
/// `tint` (`SetHelm` colours its copy), on the node `helm_node` names, seated
/// on the body as `helm_fit` says. None when the body has no node for it, the
/// set no helm, or its sprite is not there.
#[allow(clippy::too_many_arguments)]
pub fn helm_on(lib: &Library, id: u32, cs: &CSprite<Ee>, player: bool, set: u8, tint: [u8; 4], t: &Tables, fit: bool) -> Option<(i32, esf::Model)> {
    let node = helm_node(lib, id, cs, player);
    if node < 0 {
        return None;
    }
    let mut m = vi::cust::helm_with(set, t).and_then(|h| lib.lookup(h as i32)).and_then(|r| lib.build_model(r))?;
    for b in &mut m.batches {
        for l in &mut b.layers {
            l.color = tint;
        }
    }
    if fit && let Some(f) = helm_fit(lib, id, player).or_else(|| head_fit(id, cs, player)) {
        f.apply(&mut m);
    }
    Some((node, m))
}

/// How each skeleton a player may wear (`SHELLED`) wears its helm in each art,
/// past the node it hangs on: the helm stretched about that node across (x)
/// and front to back (z), never up, then moved front to back (z, forward
/// positive), as `HelmFit` does it. Measured off the meshes
/// (`tests::measure_helm_fits` prints this table): of the stretches and moves
/// that put every vertex of the skull at least `SKULL_INSIDE` inside each of
/// the eight helms, the one that moves the helms' vertices least (then the
/// least stretch, then the least move). The skull rides its helm's node
/// rigidly in every action, so one pose holds for all.
///
/// The 2002 disc's skeletons name no helm node, so theirs hangs on the 2002
/// HumanMale's (`helm_node`), which is a human's head's and carries none of
/// the stretch SOE gave the skeleton's own in Frontiers (a FixScale of 1.1
/// across and front to back): the back of the skull came out through every
/// helm by 4 to 13.5 mm (the owner, 2026-11-09: "the skeleton helm has its
/// head clipping out the back slightly"). Frontiers' skeleton needs nothing
/// past that FixScale (0.6 mm inside at the closest); the three skeletons its
/// options offer have skulls a tenth larger on the same node, whose cheeks
/// came out up to 3 mm at the face's edges. The four great skeletons, on the
/// troll's skeleton in Frontiers (their own helm node, which SOE stretched
/// 1.4 across and 1.2 up and front to back) and on the 2002 HumanMale's in
/// the 2002 art (that human's node 6, as the skeleton's), came out at the
/// cheeks too: 3.9 and 1.4 mm.
const SEATS: &[(u32, Era, [f32; 2], f32)] = &[
    // (body, art, [stretch across, front to back], move front to back);
    // Frontiers' skeleton (0x6AF98A81): 0.6 mm inside every helm at the closest
    // the old and the mossy bone: cheeks out 3.0 mm (helm 6); moved 9.6 mm at most
    (0x45A8B064, Era::Frontiers, [1.04, 1.03], 0.0015),
    (0x198BAA64, Era::Frontiers, [1.04, 1.03], 0.0015),
    // the frost bone: cheeks out 1.8 mm (helm 2); moved 3.5 mm
    (0x648DFC11, Era::Frontiers, [1.00, 1.00], 0.0035),
    // the great skeletons, all four alike (great, old, mossy and frost bone):
    // cheeks out 3.9 mm (helm 6); moved 8.0 mm at most
    (0x6AF98A82, Era::Frontiers, [1.02, 1.02], 0.0040),
    (0x197F51C7, Era::Frontiers, [1.02, 1.02], 0.0040),
    (0x90F16A6F, Era::Frontiers, [1.02, 1.02], 0.0040),
    (0x7D9F41B2, Era::Frontiers, [1.02, 1.02], 0.0040),
    // the 2002 disc's: the back of the skull out 10.8 to 13.5 mm (helm 3), 4 mm
    // and more through every helm; moved 12.4, 15.8, 16.3 and 16.3 mm at most
    (0x6AF98A81, Era::Y2002, [1.07, 1.05], -0.0050),
    (0x45A8B064, Era::Y2002, [1.07, 1.09], -0.0030),
    (0x198BAA64, Era::Y2002, [1.09, 1.09], -0.0035),
    (0x648DFC11, Era::Y2002, [1.09, 1.09], -0.0035),
    // the 2002 disc's great skeletons, all four alike: cheeks out 1.4 mm
    // (helm 4); moved 2.3 mm at most
    (0x6AF98A82, Era::Y2002, [1.01, 1.00], 0.0020),
    (0x197F51C7, Era::Y2002, [1.01, 1.00], 0.0020),
    (0x90F16A6F, Era::Y2002, [1.01, 1.00], 0.0020),
    (0x7D9F41B2, Era::Y2002, [1.01, 1.00], 0.0020),
];

/// How far inside each helm every vertex of a skeleton's skull is kept (the
/// tests that measure `SEATS` hold it to this).
#[cfg(test)]
const SKULL_INSIDE: f32 = 0.0005;

/// How body `id`'s helm is seated past the node it hangs on: a player's
/// skeleton (`SEATS`, in the art the body is drawn in now: the file the
/// library finds it in); none for any other body, an NPC (as the PS2 draws
/// it), or with the dressing turned off.
pub fn helm_fit(lib: &Library, id: u32, player: bool) -> Option<HelmFit> {
    if !player || !enabled() || !SHELLED.contains(&id) {
        return None;
    }
    let era = lib.era_of(lib.lookup(id as i32)?.file);
    SEATS.iter().find(|s| s.0 == id && s.1 == era).map(|&(_, _, [x, z], dz)| HelmFit { scale: Vec3::new(x, 1.0, z), offset: Vec3::new(0.0, 0.0, dz) })
}

/// A helm seated on a head it was not made for, in the helm's own space (its
/// node's frame): each vertex `p` goes to `p * scale + offset`, stretched
/// about the node as a FixScale stretches it, then moved; its normal turned
/// as that stretch turns a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelmFit {
    pub scale: Vec3,
    pub offset: Vec3,
}

impl HelmFit {
    /// Where the fit puts a point of the helm.
    pub fn point(&self, p: Vec3) -> Vec3 {
        p * self.scale + self.offset
    }

    /// The helm's model seated: its vertices moved, its normals turned (a
    /// stretch turns a normal by its inverse) and kept their length.
    pub fn apply(&self, m: &mut esf::Model) {
        for p in &mut m.pos {
            *p = self.point(Vec3::from(*p)).to_array();
        }
        for n in &mut m.normal {
            let v = Vec3::from(*n);
            *n = ((v / self.scale).normalize_or_zero() * v.length()).to_array();
        }
    }
}

/// Whether two sprites' skeletons are one for vertices skinned to `used`:
/// the same nodes and parents, and those nodes in the same bind pose (a
/// vertex skinned to them on one sits on the other's bones as it did). The
/// skeleton's differs from HumanMale's in one node only, the top of its
/// skull, 3 cm higher, which no armour is skinned to.
fn same_skeleton(a: &CSprite<Ee>, b: &CSprite<Ee>, mut used: impl Iterator<Item = usize>) -> bool {
    let (a, b) = (&a.h.nodes, &b.h.nodes);
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| x.parent == y.parent)
        && used.all(|i| {
            let (p, q) = (a.get(i).map(|n| n.inv_bind_world.to_cols()), b.get(i).map(|n| n.inv_bind_world.to_cols()));
            p.zip(q).is_some_and(|(p, q)| p.iter().flatten().zip(q.iter().flatten()).all(|(u, v)| (u - v).abs() < 1e-4))
        })
}

impl Shell {
    fn of(lib: &Library, h: &Human, art: Art) -> Shell {
        let m = &h.model;
        let layer = |i: usize| {
            let (entry, layer) = h.cs.text_slots[i];
            m.batches.iter().find(|b| b.material == entry).and_then(|b| b.layers.get(layer as usize)).cloned()
        };
        let mut tris: [Vec<[Corner; 3]>; SLOTS] = Default::default();
        for (i, slot) in tris.iter_mut().enumerate() {
            let (Some(tex), Some(l)) = (art[i], layer(i)) else { continue };
            let alpha = alpha_of(lib, tex as i32);
            for (_, t) in near_triangles(m).filter(|(b, _)| b.material == h.cs.text_slots[i].0) {
                let uv = t.map(|v| l.apply_uv(m.uv[v]));
                if alpha.as_ref().is_none_or(|a| a.any(&uv)) {
                    slot.push([0, 1, 2].map(|k| Corner { pos: m.pos[t[k]], normal: m.normal[t[k]], color: m.color[t[k]], uv: uv[k], bones: m.bones[t[k]], weights: m.weights[t[k]] }));
                }
            }
        }
        let fallback = esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
        Shell { tris, layers: std::array::from_fn(|i| layer(i).unwrap_or_else(|| fallback.clone())) }
    }
}

/// The layer changes on the made-up slots put on `model` as a shell's
/// batches: per slot with art, the human's triangles there, new vertices of
/// the body's, with the human's slot layer in the slot's texture and colour;
/// each triangle twice, its inside too (turned round, its normals reversed).
/// On a human the armour is a skin over its body and the renderer culls back
/// faces as the PS2's strip programs do; round a skeleton's thin bones the
/// shell is hollow, and from where its far side's inside faces the eye (in
/// at the collar, through a see-through texel) that side was culled and the
/// armour seen through (the user, 2026-11-08: "the backface culling is
/// letting me see through the armor").
pub fn put_on(model: &mut esf::Model, shell: &Shell, changes: &[LayerChange]) -> bool {
    let mut added = false;
    for (slot, (tex, color)) in worn(changes).into_iter().enumerate() {
        let Some(tex) = tex else { continue };
        let tris = &shell.tris[slot];
        if tris.is_empty() {
            continue;
        }
        let mut layer = shell.layers[slot].clone();
        layer.texture = tex as i32;
        if let Some(c) = color {
            layer.color = c;
        }
        let mut indices = Vec::new();
        let inside = |c: &Corner| Corner { normal: c.normal.map(|n| -n), ..*c };
        for t in tris {
            for c in [t[0], t[1], t[2], inside(&t[0]), inside(&t[2]), inside(&t[1])] {
                indices.push(model.pos.len() as u32);
                model.pos.push(c.pos);
                model.uv.push(c.uv);
                model.normal.push(c.normal);
                model.color.push(c.color);
                model.bones.push(c.bones);
                model.weights.push(c.weights);
            }
        }
        model.triangles += 2 * tris.len() as u32;
        model.batches.push(esf::Batch { material: ENTRY + slot as i32, layers: vec![layer], indices, lod: None });
        added = true;
    }
    added
}

/// One of a body's triangles in a slot: its three vertices (the model's
/// indices), the texture coordinates the human has where they land, and the
/// level of detail its batch is drawn at.
#[derive(Clone, Debug, PartialEq)]
pub struct Tri {
    pub v: [u32; 3],
    pub uv: [[f32; 2]; 3],
    pub lod: Option<u8>,
}

/// A slotless body fitted to the human for one look: its triangles per
/// slot, and the human's layer per slot (the one its TSlotList names, its
/// texture coordinates' transform folded into `Tri::uv`).
#[derive(Clone, Debug)]
pub struct Fit {
    pub tris: [Vec<Tri>; SLOTS],
    pub layers: [esf::Layer; SLOTS],
}

/// The art per slot a fit is made for (None: the human's own there).
type Art = [Option<u32>; SLOTS];

/// What was made, by what it was made from (None: it could not be).
type Cache<K, V> = std::cell::RefCell<HashMap<K, Option<Rc<V>>>>;

thread_local! {
    /// The human fitted to, by art and sex.
    static REFS: Cache<(Era, bool), Reference> = std::cell::RefCell::new(HashMap::new());
    /// A body's triangles carried onto the human, by body and art.
    static CARRIED: Cache<(i32, Era), Carried> = std::cell::RefCell::new(HashMap::new());
    /// Fits made, by body, art, what each slot wears and whether the legs
    /// are in a robe.
    static FITS: Cache<(i32, Era, Art, bool), Fit> = std::cell::RefCell::new(HashMap::new());
    /// Textures' see-through texels, by DictID and art.
    static ALPHAS: Cache<(i32, Era), Alpha> = std::cell::RefCell::new(HashMap::new());
    /// Shells made, by body, art and what each slot wears.
    static SHELLS: Cache<(i32, Era, Art), Shell> = std::cell::RefCell::new(HashMap::new());
    static NAMED: Cache<(i32, Era, Art), Shell> = std::cell::RefCell::new(HashMap::new());
    /// The colours of the textures kept parts are drawn in, by DictID and art.
    static TEXELS: Cache<(i32, Era), Texels> = std::cell::RefCell::new(HashMap::new());
}

/// The fit of body `id` (`model` and `cs` built from it in the library's
/// view now) wearing `art`, its legs in a robe's lower half when `robed`,
/// made once per body, art and look.
pub fn fit_of(lib: &Library, id: i32, model: &esf::Model, cs: &CSprite<Ee>, art: Art, robed: bool) -> Option<Rc<Fit>> {
    let era = lib.view();
    let key = (id, era, art, robed);
    if let Some(f) = FITS.with(|f| f.borrow().get(&key).cloned()) {
        return f;
    }
    let female = cs.sex != 0;
    let r = REFS.with(|r| r.borrow().get(&(era, female)).cloned()).unwrap_or_else(|| {
        let made = Human::load(lib, female).and_then(|h| Reference::of(lib, &h)).map(Rc::new);
        REFS.with(|r| r.borrow_mut().insert((era, female), made.clone()));
        made
    });
    let fit = r.and_then(|r| {
        let c = CARRIED.with(|c| c.borrow().get(&(id, era)).cloned()).unwrap_or_else(|| {
            let made = Carried::of(&r, model, cs, &wearing(model, kept(id), |t| colours_of(lib, t))).map(Rc::new);
            CARRIED.with(|c| c.borrow_mut().insert((id, era), made.clone()));
            made
        })?;
        let alpha: [Option<Rc<Alpha>>; SLOTS] = std::array::from_fn(|i| alpha_of(lib, art[i].map_or(r.own[i], |t| t as i32)));
        Some(Rc::new(fit(&r, &c, &alpha, robed)))
    });
    FITS.with(|f| {
        let mut f = f.borrow_mut();
        // a fit takes a few milliseconds to make again: keep at most a few
        // hundred looks' worth
        if f.len() >= 256 {
            f.clear();
        }
        f.insert(key, fit.clone());
    });
    fit
}

/// A texture's see-through texels (the alpha `Surface::decode` gives as
/// stored: 0x80 opaque; the renderer's alpha test passes from 0x40).
struct Alpha {
    w: usize,
    h: usize,
    a: Vec<u8>,
}

impl Alpha {
    /// Whether the texel at `uv` shows (the texture repeated).
    fn at(&self, uv: [f32; 2]) -> bool {
        let x = (uv[0].rem_euclid(1.0) * self.w as f32) as usize % self.w;
        let y = (uv[1].rem_euclid(1.0) * self.h as f32) as usize % self.h;
        self.a[y * self.w + x] >= 0x40
    }

    /// Whether a triangle shows: four of its corners, its edges' middles and
    /// its middle.
    fn shows(&self, uv: &[[f32; 2]; 3]) -> bool {
        Alpha::points(uv).iter().filter(|&&p| self.at(p)).count() >= 4
    }

    /// Whether any of those points of a triangle shows.
    fn any(&self, uv: &[[f32; 2]; 3]) -> bool {
        Alpha::points(uv).iter().any(|&p| self.at(p))
    }

    /// A triangle's corners, its edges' middles and its middle.
    fn points(uv: &[[f32; 2]; 3]) -> [[f32; 2]; 7] {
        let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let c = [(uv[0][0] + uv[1][0] + uv[2][0]) / 3.0, (uv[0][1] + uv[1][1] + uv[2][1]) / 3.0];
        [uv[0], uv[1], uv[2], mid(uv[0], uv[1]), mid(uv[1], uv[2]), mid(uv[2], uv[0]), c]
    }
}

/// Texture `id`'s see-through texels in the library's view now; None when
/// it does not resolve or decode (then it is taken to show everywhere).
fn alpha_of(lib: &Library, id: i32) -> Option<Rc<Alpha>> {
    let key = (id, lib.view());
    if let Some(a) = ALPHAS.with(|a| a.borrow().get(&key).cloned()) {
        return a;
    }
    let made = lib.surface(id).and_then(|(c, s)| s.decode(&c.data)).map(|img| {
        Rc::new(Alpha { w: img.width as usize, h: img.height as usize, a: img.rgba.chunks_exact(4).map(|p| p[3]).collect() })
    });
    ALPHAS.with(|a| a.borrow_mut().insert(key, made.clone()));
    made
}

/// Whether a triangle textured `id` at `uv` shows, not wholly see-through
/// (for the capes' tests, `cape::poke`: a robe's skirt only under a robe).
#[cfg(test)]
pub(crate) fn texture_shows(lib: &Library, id: i32, uv: &[[f32; 2]; 3]) -> bool {
    shows(&alpha_of(lib, id), uv)
}

fn shows(a: &Option<Rc<Alpha>>, uv: &[[f32; 2]; 3]) -> bool {
    a.as_ref().is_none_or(|a| a.shows(uv))
}

/// The human body a slotless one is fitted to.
struct Human {
    model: esf::Model,
    cs: CSprite<Ee>,
}

impl Human {
    fn load(lib: &Library, female: bool) -> Option<Human> {
        let names: &[&str] = if female { &["HumanFemale", "HumanMale"] } else { &["HumanMale"] };
        names.iter().find_map(|n| Human::load_id(lib, vi::hash::hash_resource_id(n)))
    }

    /// Body `id`, when it has the five worn slots.
    fn load_id(lib: &Library, id: u32) -> Option<Human> {
        let r = lib.lookup(id as i32)?;
        let model = lib.build_model(r)?;
        let mut anims = vi::hsprite::AnimScene::<Ee>::default();
        let cs = vi::character::load_csprite(lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut Rand(1))?;
        (0..SLOTS).all(|i| cs.text_slots[i].0 != -1).then_some(Human { model, cs })
    }

    /// The player bodies a `SHELLED` body's armour may come from, in order: the
    /// human's (`load`), then `SHELL_BODIES`', each loaded only when asked for.
    fn shell_sources(lib: &Library, female: bool) -> impl Iterator<Item = Human> + '_ {
        let mut human = Some(female);
        let mut rest = SHELL_BODIES.iter();
        std::iter::from_fn(move || {
            if let Some(f) = human.take()
                && let Some(h) = Human::load(lib, f)
            {
                return Some(h);
            }
            rest.by_ref().find_map(|&id| Human::load_id(lib, id))
        })
    }
}

/// Where a node of a skeleton is: on the spine (the root up to the head's
/// parent, the bones between it and a limb, and what hugs the torso), on a
/// limb (and which of its three segments: upper arm, forearm, hand; thigh,
/// shin, foot), on the head, or on something else hanging off the spine (a
/// wing, a tail).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Part {
    Body,
    Limb(Limb, u8),
    Head,
    Other,
}

/// The four limbs: the arm whose hand holds the main item (NodeIDList slot
/// 0), the other's (1), the leg of footstep node 5 and of 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Limb {
    Arm(u8),
    Leg(u8),
}

/// What a triangle is fitted within: the body, or one limb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Group {
    Body,
    Limb(Limb),
}

impl Part {
    fn group(self) -> Option<Group> {
        match self {
            Part::Body => Some(Group::Body),
            Part::Limb(l, _) => Some(Group::Limb(l)),
            _ => None,
        }
    }

    /// The segment a vertex on this part is carried along.
    fn seg(self) -> Option<Seg> {
        match self {
            Part::Body => Some(Seg::Body),
            Part::Limb(l, k) => Some(Seg::Limb(l, k)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Seg {
    Body,
    Limb(Limb, u8),
}

impl Seg {
    fn group(self) -> Group {
        match self {
            Seg::Body => Group::Body,
            Seg::Limb(l, _) => Group::Limb(l),
        }
    }
}

/// A skeleton read for the fit: each node's part, each node's parent, and
/// each segment's two ends in the bind pose (model space).
#[derive(Debug)]
struct Rig {
    part: Vec<Part>,
    parent: Vec<i32>,
    segs: HashMap<Seg, (Vec3, Vec3)>,
    head: usize,
    /// The spine nodes the arms hang from.
    shoulders: Vec<usize>,
}

impl Rig {
    /// From each node's parent and bind position and the NodeIDList; None
    /// when the list does not name the hands, the feet and the head, or the
    /// chains between them are not arms, legs and a spine.
    fn of(parent: &[i32], at: &[Vec3], ids: &[i32; 10]) -> Option<Rig> {
        let n = parent.len();
        let node = |i: i32| usize::try_from(i).ok().filter(|&i| i < n);
        let up = |i: usize| node(parent[i]);
        let children = |i: usize| (0..n).filter(move |&k| parent[k] == i as i32);
        let mut part = vec![None::<Part>; n];
        fn mark(part: &mut [Option<Part>], parent: &[i32], i: usize, p: Part) {
            part[i] = Some(p);
            for k in 0..parent.len() {
                if parent[k] == i as i32 {
                    mark(part, parent, k, p);
                }
            }
        }
        // the node of a subtree farthest from `i`
        let farthest = |i: usize| {
            let mut best = (0.0f32, i);
            let mut stack = vec![i];
            while let Some(k) = stack.pop() {
                let d = at[k].distance(at[i]);
                if d > best.0 {
                    best = (d, k);
                }
                stack.extend(children(k));
            }
            best.1
        };
        let head = node(ids[7])?;
        mark(&mut part, parent, head, Part::Head);
        let mut k = up(head)?;
        loop {
            part[k] = Some(Part::Body);
            match up(k) {
                Some(p) => k = p,
                None => break,
            }
        }
        let root = k;
        let mut segs = HashMap::new();
        let mut shoulders = Vec::new();
        // the arms: a hand is the parent of its item node
        for s in 0..2u8 {
            let item = node(ids[s as usize])?;
            let hand = up(item)?;
            let fore = up(hand)?;
            let upper = up(fore)?;
            if [hand, fore, upper].iter().any(|&i| part[i].is_some()) {
                return None;
            }
            let l = Limb::Arm(s);
            mark(&mut part, parent, upper, Part::Limb(l, 0));
            mark(&mut part, parent, fore, Part::Limb(l, 1));
            mark(&mut part, parent, hand, Part::Limb(l, 2));
            // the bones between the upper arm and the spine (a collarbone)
            let mut k = up(upper)?;
            while part[k].is_none() {
                part[k] = Some(Part::Body);
                k = up(k)?;
            }
            shoulders.push(k);
            segs.insert(Seg::Limb(l, 0), (at[upper], at[fore]));
            segs.insert(Seg::Limb(l, 1), (at[fore], at[hand]));
            segs.insert(Seg::Limb(l, 2), (at[hand], at[farthest(hand)]));
        }
        // the feet: the footstep nodes; where both slots name one (the
        // ratman trickster's NodeIDList names its left foot, 41, twice), the
        // other is the node mirrored across the root, within a centimetre
        let mut feet = [ids[5], ids[6]];
        if feet[0] == feet[1]
            && let Some(f) = node(feet[0])
        {
            let m = Vec3::new(2.0 * at[root].x - at[f].x, at[f].y, at[f].z);
            feet[1] = (0..n).filter(|&k| k != f && at[k].distance(m) < 0.01).min_by(|&a, &b| at[a].distance(m).total_cmp(&at[b].distance(m))).map_or(-1, |k| k as i32);
        }
        // the legs: the chain from the root down to the footstep node's
        // farthest end; the hip is its first joint off the middle, the knee
        // and the ankle the joints below it nearest a human's
        for s in 0..2u8 {
            let end = farthest(node(feet[s as usize])?);
            let mut chain = vec![end];
            while let Some(p) = up(*chain.last()?) {
                chain.push(p);
            }
            chain.reverse();
            if chain[0] != root {
                return None;
            }
            let drop = (at[root].y - at[end].y).max(1e-3);
            let hip = (1..chain.len()).find(|&i| (at[chain[i]].x - at[root].x).abs() > 0.03 * drop)?;
            let height = (at[chain[hip]].y - at[end].y).max(1e-3);
            let frac = |i: usize| (at[chain[hip]].y - at[chain[i]].y) / height;
            let nearest = |from: usize, f: f32| (from..chain.len()).min_by(|&a, &b| (frac(a) - f).abs().total_cmp(&(frac(b) - f).abs()));
            let knee = nearest(hip + 1, KNEE).filter(|&k| k + 1 < chain.len())?;
            let ankle = nearest(knee + 1, ANKLE)?;
            let (hip, knee, ankle) = (chain[hip], chain[knee], chain[ankle]);
            if [hip, knee, ankle].iter().any(|&i| part[i].is_some()) {
                return None;
            }
            // a foot with no bone past the ankle: a stub forward and down
            let toe = match farthest(ankle) {
                t if t != ankle => at[t],
                _ => at[ankle] + Vec3::new(0.0, -0.25, 0.25) * at[knee].distance(at[ankle]),
            };
            let l = Limb::Leg(s);
            mark(&mut part, parent, hip, Part::Limb(l, 0));
            mark(&mut part, parent, knee, Part::Limb(l, 1));
            mark(&mut part, parent, ankle, Part::Limb(l, 2));
            let mut k = up(hip)?;
            while part[k].is_none() {
                part[k] = Some(Part::Body);
                k = up(k)?;
            }
            segs.insert(Seg::Limb(l, 0), (at[hip], at[knee]));
            segs.insert(Seg::Limb(l, 1), (at[knee], at[ankle]));
            segs.insert(Seg::Limb(l, 2), (at[ankle], toe));
        }
        segs.insert(Seg::Body, (at[root], at[head]));
        // a body that stands on two legs: its hands well off the ground (a
        // four-legged body's "hands" are its forefeet)
        let ground = [Seg::Limb(Limb::Leg(0), 2), Seg::Limb(Limb::Leg(1), 2)].iter().map(|s| segs[s].1.y.min(segs[s].0.y)).fold(f32::MAX, f32::min);
        let tall = at[head].y - ground;
        if [Seg::Limb(Limb::Arm(0), 2), Seg::Limb(Limb::Arm(1), 2)].iter().any(|s| segs[s].0.y - ground < 0.25 * tall) {
            return None;
        }
        Some(Rig { part: part.into_iter().map(|p| p.unwrap_or(Part::Other)).collect(), parent: parent.to_vec(), segs, head, shoulders })
    }

    /// The arm whose upper bone a triangle (its corners `p`, in the bind
    /// pose) runs along, from the shoulder to the elbow: a corner at the
    /// elbow (`ELBOW` of the way there or past it), and its middle past
    /// `ARM_ALONG` of the way, not past the elbow, and nearer that bone than
    /// the spine's. A body whose upper arm has no vertex of its own skins
    /// such triangles to the spine at the shoulder (the fungusman's run from
    /// the shoulder straight to the elbow); on the arm they take the human's
    /// sleeve, where on the spine they took its side, stretched along the
    /// arm. A triangle of the torso by the shoulder, or of a kappa's shell
    /// beside the arm, reaches no elbow.
    fn arm_along(&self, p: [Vec3; 3]) -> Option<u8> {
        let (a, b) = self.segs[&Seg::Body];
        let spine = Frame::new(a, b);
        let mid = (p[0] + p[1] + p[2]) / 3.0;
        (0..2u8).find(|&s| {
            let (a, b) = self.segs[&Seg::Limb(Limb::Arm(s), 0)];
            let arm = Frame::new(a, b);
            let t = |q: Vec3| (q - a).dot(arm.d) / arm.len;
            p.iter().any(|&q| t(q) >= ELBOW) && t(mid) > ARM_ALONG && t(mid) <= 1.0 && seg_distance(&arm, mid) < seg_distance(&spine, mid)
        })
    }

    /// The neck made the head's: the spine between the head and the node
    /// the arms hang from (a human's neck is its head's material, and its
    /// armour stops below it).
    fn neck(&mut self) {
        let mut k = self.parent[self.head];
        while let Ok(i) = usize::try_from(k) {
            if self.shoulders.contains(&i) || self.part[i] != Part::Body {
                break;
            }
            self.part[i] = Part::Head;
            k = self.parent[i];
        }
    }

    fn of_sprite(cs: &CSprite<Ee>) -> Option<Rig> {
        let parent: Vec<i32> = cs.h.nodes.iter().map(|n| n.parent).collect();
        let at: Vec<Vec3> = cs.h.nodes.iter().map(|n| Vec3::new(n.bind.trans.x.0, n.bind.trans.y.0, n.bind.trans.z.0)).collect();
        Rig::of(&parent, &at, &cs.node_ids)
    }

    /// The nodes off the spine whose skin hugs the torso made the torso's:
    /// each subtree of `Other` nodes hanging off a `Body` one, when nine in
    /// ten of the vertices on it (`on`: a vertex, the node it is on) are no
    /// farther from the spine (`body`, the torso's segment) than `HUG` times
    /// the farthest of the torso's own at their height (in eight bands from
    /// its lowest vertex to its highest; past them, the nearest band).
    fn fold(&mut self, body: &Frame, on: &[(Vec3, usize)]) {
        let n = self.part.len();
        let round = |p: Vec3| {
            let q = p - body.a;
            (q.dot(body.d), (q.dot(body.e1).powi(2) + q.dot(body.e2).powi(2)).sqrt())
        };
        let torso: Vec<(f32, f32)> = on.iter().filter(|(_, b)| self.part.get(*b) == Some(&Part::Body)).map(|(p, _)| round(*p)).collect();
        if torso.is_empty() {
            return;
        }
        const BANDS: usize = 8;
        let (lo, hi) = torso.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &(t, _)| (lo.min(t), hi.max(t)));
        let band = |t: f32| (((t - lo) / (hi - lo).max(1e-6) * BANDS as f32) as isize).clamp(0, BANDS as isize - 1) as usize;
        let mut skin = [0.0f32; BANDS];
        for &(t, r) in &torso {
            skin[band(t)] = skin[band(t)].max(r);
        }
        // a band with no torso skin takes the nearest band's that has some
        let had = skin;
        for (i, s) in skin.iter_mut().enumerate() {
            if *s == 0.0 {
                *s = (0..BANDS).filter(|&j| had[j] > 0.0).min_by_key(|&j| j.abs_diff(i)).map_or(0.0, |j| had[j]);
            }
        }
        for k in 0..n {
            let off_spine = self.part[k] == Part::Other && usize::try_from(self.parent[k]).ok().is_some_and(|p| self.part[p] == Part::Body);
            if !off_spine {
                continue;
            }
            let mut tree = vec![false; n];
            let mut stack = vec![k];
            while let Some(i) = stack.pop() {
                tree[i] = true;
                stack.extend((0..n).filter(|&c| self.parent[c] == i as i32));
            }
            let mut d: Vec<f32> = on
                .iter()
                .filter(|(_, b)| tree.get(*b) == Some(&true))
                .map(|(p, _)| {
                    let (t, r) = round(*p);
                    r / skin[band(t)].max(1e-4)
                })
                .collect();
            if d.is_empty() {
                continue;
            }
            d.sort_by(f32::total_cmp);
            if d[(d.len() * 9 / 10).min(d.len() - 1)] <= HUG {
                for (i, t) in tree.iter().enumerate() {
                    if *t {
                        self.part[i] = Part::Body;
                    }
                }
            }
        }
    }
}

/// A segment's frame: from its first end along it (`d`, `len` long), and
/// round it (`e1` toward the front, or up for a bone that points forward;
/// `e2` across), with the girth of the vertices on it along each.
#[derive(Clone, Copy, Debug)]
struct Frame {
    a: Vec3,
    d: Vec3,
    len: f32,
    e1: Vec3,
    e2: Vec3,
    r: [f32; 2],
}

impl Frame {
    fn new(a: Vec3, b: Vec3) -> Frame {
        let len = a.distance(b).max(1e-3);
        let d = if a.distance(b) > 1e-6 { (b - a) / a.distance(b) } else { Vec3::NEG_Y };
        let side = |r: Vec3| r - d * r.dot(d);
        let f = side(Vec3::Z);
        let e1 = if f.length() > 0.35 { f.normalize() } else { side(Vec3::Y).normalize() };
        Frame { a, d, len, e1, e2: d.cross(e1), r: [1.0, 1.0] }
    }

    /// Along, and round in girths.
    fn local(&self, p: Vec3) -> Vec3 {
        let q = p - self.a;
        Vec3::new(q.dot(self.d) / self.len, q.dot(self.e1) / self.r[0], q.dot(self.e2) / self.r[1])
    }

    fn world(&self, l: Vec3) -> Vec3 {
        self.a + self.d * (l.x * self.len) + self.e1 * (l.y * self.r[0]) + self.e2 * (l.z * self.r[1])
    }
}

/// Each segment's frame, its girth measured over the vertices on it.
fn frames(rig: &Rig, verts: impl Iterator<Item = (Vec3, Seg)>) -> HashMap<Seg, Frame> {
    let mut f: HashMap<Seg, Frame> = rig.segs.iter().map(|(&s, &(a, b))| (s, Frame::new(a, b))).collect();
    let mut sums: HashMap<Seg, (f32, f32, u32)> = HashMap::new();
    for (p, s) in verts {
        if let Some(fr) = f.get(&s) {
            let q = p - fr.a;
            let e = sums.entry(s).or_default();
            e.0 += q.dot(fr.e1).powi(2);
            e.1 += q.dot(fr.e2).powi(2);
            e.2 += 1;
        }
    }
    for (s, fr) in f.iter_mut() {
        if let Some(&(a, b, n)) = sums.get(s).filter(|v| v.2 > 0) {
            fr.r = [(a / n as f32).sqrt().max(0.005), (b / n as f32).sqrt().max(0.005)];
        }
    }
    f
}

/// A vertex's heaviest bone (none for a vertex on no bone).
fn bone_of(model: &esf::Model, v: usize) -> Option<usize> {
    let (b, w) = (model.bones[v], model.weights[v]);
    (0..4).filter(|&k| b[k] != 255 && w[k] > 0).max_by_key(|&k| (w[k], std::cmp::Reverse(k))).map(|k| b[k] as usize)
}

/// A vertex's part: its heaviest bone's.
fn part_of(model: &esf::Model, rig: &Rig, v: usize) -> Part {
    bone_of(model, v).and_then(|b| rig.part.get(b).copied()).unwrap_or(Part::Other)
}

/// A triangle's group: where most of its three vertices' weight is
/// (`weight_group`), but an arm's for one on the spine that runs along that
/// arm's upper bone to the elbow (`Rig::arm_along`).
fn group_of(model: &esf::Model, rig: &Rig, t: [usize; 3]) -> Option<Group> {
    let g = weight_group(model, rig, t)?;
    match rig.arm_along(t.map(|v| pos(model, v))) {
        Some(s) if g == Group::Body => Some(Group::Limb(Limb::Arm(s))),
        _ => Some(g),
    }
}

/// Where most of a triangle's three vertices' weight is, None when at least
/// as much is on the head or on something off the spine.
fn weight_group(model: &esf::Model, rig: &Rig, t: [usize; 3]) -> Option<Group> {
    let mut by: HashMap<Option<Group>, u32> = HashMap::new();
    for &v in &t {
        let (b, w) = (model.bones[v], model.weights[v]);
        for k in 0..4 {
            if b[k] != 255 && w[k] > 0 {
                let g = rig.part.get(b[k] as usize).and_then(|p| p.group());
                *by.entry(g).or_default() += w[k] as u32;
            }
        }
    }
    let off = by.get(&None).copied().unwrap_or(0);
    // the heaviest, the body on a tie, else the first group in order (not
    // the map's order, which differs from run to run)
    let (_, g, w) = by.into_iter().filter_map(|(g, w)| g.map(|g| (matches!(g, Group::Body), g, w))).max_by_key(|&(body, g, w)| (w, body, std::cmp::Reverse(g)))?;
    (w > off).then_some(g)
}

/// The closest point of triangle `p` to `q`, as barycentric weights, and
/// the squared distance to it.
fn closest(q: Vec3, p: &[Vec3; 3]) -> (f32, [f32; 3]) {
    // Ericson, Real-Time Collision Detection, 5.1.5
    let (a, b, c) = (p[0], p[1], p[2]);
    let (ab, ac, ap) = (b - a, c - a, q - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    let w = if d1 <= 0.0 && d2 <= 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        let bp = q - b;
        let (d3, d4) = (ab.dot(bp), ac.dot(bp));
        if d3 >= 0.0 && d4 <= d3 {
            [0.0, 1.0, 0.0]
        } else {
            let vc = d1 * d4 - d3 * d2;
            if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
                let v = d1 / (d1 - d3);
                [1.0 - v, v, 0.0]
            } else {
                let cp = q - c;
                let (d5, d6) = (ab.dot(cp), ac.dot(cp));
                if d6 >= 0.0 && d5 <= d6 {
                    [0.0, 0.0, 1.0]
                } else {
                    let vb = d5 * d2 - d1 * d6;
                    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
                        let w = d2 / (d2 - d6);
                        [1.0 - w, 0.0, w]
                    } else {
                        let va = d3 * d6 - d5 * d4;
                        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
                            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
                            [0.0, 1.0 - w, w]
                        } else {
                            let denom = 1.0 / (va + vb + vc);
                            let (v, w) = (vb * denom, vc * denom);
                            [1.0 - v - w, v, w]
                        }
                    }
                }
            }
        }
    };
    let x = a * w[0] + b * w[1] + c * w[2];
    (x.distance_squared(q), w)
}

/// The triangles of `model` drawn at its nearest level (or at every level).
fn near_triangles(model: &esf::Model) -> impl Iterator<Item = (&esf::Batch, [usize; 3])> {
    model.batches.iter().filter(|b| b.lod.is_none_or(|l| l == 0)).flat_map(|b| b.indices.chunks_exact(3).map(move |t| (b, [t[0] as usize, t[1] as usize, t[2] as usize])))
}

fn pos(model: &esf::Model, v: usize) -> Vec3 {
    Vec3::from(model.pos[v])
}

/// A triangle of the human's slot materials: in model space, with its
/// texture coordinates (as the slot's layer samples them), slot, UV island,
/// and whether another layer of its material (a human's skin) shows on it.
struct Target {
    p: [Vec3; 3],
    uv: [[f32; 2]; 3],
    slot: usize,
    island: usize,
    skin: bool,
}

/// The human: its slot triangles per group, the frames of its segments,
/// each slot's layer and the texture that layer has of its own.
struct Reference {
    frames: HashMap<Seg, Frame>,
    targets: HashMap<Group, Vec<Target>>,
    layers: [esf::Layer; SLOTS],
    own: [i32; SLOTS],
}

impl Reference {
    fn of(lib: &Library, h: &Human) -> Option<Reference> {
        let mut rig = Rig::of_sprite(&h.cs)?;
        // on the human, what hangs off the spine (a robe's flaps, the chest
        // node) is the body's: its armour is there
        for p in rig.part.iter_mut() {
            if *p == Part::Other {
                *p = Part::Body;
            }
        }
        let m = &h.model;
        let slot_of = |b: &esf::Batch| (0..SLOTS).find(|&i| h.cs.text_slots[i].0 == b.material);
        let mut layers: Vec<esf::Layer> = Vec::new();
        for i in 0..SLOTS {
            let (entry, layer) = h.cs.text_slots[i];
            let l = m.batches.iter().find(|b| b.material == entry).and_then(|b| b.layers.get(layer as usize)).cloned()?;
            layers.push(l);
        }
        // the UV islands: triangles of one material sharing an edge whose
        // ends have one place and one texture coordinate
        let mut weld: HashMap<([u32; 3], [u32; 2], i32), usize> = HashMap::new();
        let mut tris: Vec<(&esf::Batch, usize, [usize; 3], [usize; 3])> = Vec::new();
        for (b, t) in near_triangles(m) {
            let Some(slot) = slot_of(b) else { continue };
            let mut key = |v: usize| {
                let n = weld.len();
                *weld.entry((m.pos[v].map(f32::to_bits), m.uv[v].map(f32::to_bits), b.material)).or_insert(n)
            };
            let w = [key(t[0]), key(t[1]), key(t[2])];
            tris.push((b, slot, t, w));
        }
        let mut island: Vec<usize> = (0..tris.len()).collect();
        fn find(x: &mut [usize], mut i: usize) -> usize {
            while x[i] != i {
                x[i] = x[x[i]];
                i = x[i];
            }
            i
        }
        let mut edges: HashMap<(usize, usize), usize> = HashMap::new();
        for (k, (_, _, _, w)) in tris.iter().enumerate() {
            for e in 0..3 {
                let (a, b) = (w[e].min(w[(e + 1) % 3]), w[e].max(w[(e + 1) % 3]));
                match edges.get(&(a, b)) {
                    Some(&o) => {
                        let (x, y) = (find(&mut island, o), find(&mut island, k));
                        island[x] = y;
                    }
                    None => {
                        edges.insert((a, b), k);
                    }
                }
            }
        }
        let mut targets: HashMap<Group, Vec<Target>> = HashMap::new();
        let mut on: Vec<(Vec3, Seg)> = Vec::new();
        for (k, &(b, slot, t, _)) in tris.iter().enumerate() {
            let Some(g) = group_of(m, &rig, t) else { continue };
            for &v in &t {
                if let Some(s) = part_of(m, &rig, v).seg() {
                    on.push((pos(m, v), s));
                }
            }
            let sl = h.cs.text_slots[slot].1 as usize;
            // the skin: an opaque layer of the material other than the slot's
            let skin = b.layers.iter().enumerate().any(|(j, l)| j != sl && l.blend == 0 && l.texture != 0 && alpha_of(lib, l.texture).is_some_and(|a| a.shows(&t.map(|v| l.apply_uv(m.uv[v])))));
            let l = &layers[slot];
            targets.entry(g).or_default().push(Target { p: t.map(|v| pos(m, v)), uv: t.map(|v| l.apply_uv(m.uv[v])), slot, island: find(&mut island, k), skin });
        }
        let frames = frames(&rig, on.into_iter());
        let own: [i32; SLOTS] = std::array::from_fn(|i| layers[i].texture);
        let layers: [esf::Layer; SLOTS] = layers.try_into().ok()?;
        Some(Reference { frames, targets, layers, own })
    }
}

/// A body's triangles that can wear armour.
struct Carried {
    tris: Vec<CarriedTri>,
}

/// A triangle (its vertices), its group, its level, and its three corners
/// carried onto the human.
struct CarriedTri {
    v: [usize; 3],
    group: Group,
    lod: Option<u8>,
    q: [Vec3; 3],
}

impl Carried {
    /// `wearing`: per batch, per triangle, whether it can wear armour
    /// (`wearing`).
    fn of(r: &Reference, model: &esf::Model, cs: &CSprite<Ee>, wearing: &[Vec<bool>]) -> Option<Carried> {
        let mut rig = Rig::of_sprite(cs)?;
        rig.neck();
        let worn: Vec<([usize; 3], Option<u8>)> = model
            .batches
            .iter()
            .zip(wearing)
            .flat_map(|(b, on)| b.indices.chunks_exact(3).zip(on).filter(|(_, on)| **on).map(|(t, _)| ([t[0] as usize, t[1] as usize, t[2] as usize], b.lod)))
            .collect();
        let verts: Vec<usize> = {
            let mut v: Vec<usize> = worn.iter().flat_map(|(t, _)| *t).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        // what hugs the torso is the torso's (its girth first, from the
        // vertices on the spine)
        let body = frames(&rig, verts.iter().filter(|&&v| part_of(model, &rig, v) == Part::Body).map(|&v| (pos(model, v), Seg::Body)))[&Seg::Body];
        let on: Vec<(Vec3, usize)> = verts.iter().filter_map(|&v| bone_of(model, v).map(|b| (pos(model, v), b))).collect();
        rig.fold(&body, &on);
        let frames = frames(&rig, verts.iter().filter_map(|&v| part_of(model, &rig, v).seg().map(|s| (pos(model, v), s))));
        // a corner carried onto the human: along its own segment when that
        // is in the triangle's group, else along the group's segment nearest
        let carry = |v: usize, g: Group| -> Option<Vec3> {
            let p = pos(model, v);
            let s = part_of(model, &rig, v).seg().filter(|s| s.group() == g).or_else(|| {
                let d = |s: &Seg| frames.get(s).map_or(f32::MAX, |f| seg_distance(f, p));
                rig.segs.keys().copied().filter(|s| s.group() == g).min_by(|a, b| d(a).total_cmp(&d(b)).then(a.cmp(b)))
            })?;
            Some(r.frames.get(&s)?.world(frames.get(&s)?.local(p)))
        };
        let tris = worn
            .into_iter()
            .filter_map(|(t, lod)| {
                let g = group_of(model, &rig, t)?;
                Some(CarriedTri { v: t, group: g, lod, q: [carry(t[0], g)?, carry(t[1], g)?, carry(t[2], g)?] })
            })
            .collect();
        Some(Carried { tris })
    }
}

/// A body's carried triangles fitted to the human's surface as it shows in
/// a look (`alpha`: the see-through texels of each slot's art there): each
/// takes the slot of the shown triangle nearest its middle, and texture
/// coordinates from the nearest shown triangles of that one's UV island.
/// With `robed`, a leg's triangles are fitted to what a human in the robe
/// shows round its legs: above the skirt's hem the skirt (the legs slot's
/// triangles no skin shows on: the legs are inside it), below it the legs
/// and feet a knee-length robe leaves out, or the skirt.
fn fit(r: &Reference, c: &Carried, alpha: &[Option<Rc<Alpha>>; SLOTS], robed: bool) -> Fit {
    let shown: HashMap<Group, Vec<&Target>> = r.targets.iter().map(|(&g, ts)| (g, ts.iter().filter(|t| t.skin || shows(&alpha[t.slot], &t.uv)).collect())).collect();
    // (the groups in order: which of two targets at one distance is nearest
    // goes by their order)
    let mut groups: Vec<&Group> = r.targets.keys().collect();
    groups.sort();
    let skirt: Vec<&Target> = if robed { groups.iter().flat_map(|g| &r.targets[g]).filter(|t| t.slot == 3 && !t.skin && shows(&alpha[3], &t.uv)).collect() } else { Vec::new() };
    let hem = hem(&skirt);
    let mid = |t: &Target| (t.p[0] + t.p[1] + t.p[2]) / 3.0;
    // under the hem, per leg: what shows there, and the skirt
    let below: HashMap<Group, Vec<&Target>> = shown
        .iter()
        .filter(|(g, _)| matches!(g, Group::Limb(Limb::Leg(_))))
        .map(|(&g, ts)| (g, ts.iter().copied().filter(|t| mid(t).y < hem).chain(skirt.iter().copied()).collect()))
        .collect();
    let mut out: [Vec<Tri>; SLOTS] = Default::default();
    for CarriedTri { v: t, group: g, lod, q } in &c.tris {
        let c = (q[0] + q[1] + q[2]) / 3.0;
        let targets = match g {
            Group::Limb(Limb::Leg(_)) if !skirt.is_empty() => {
                if c.y >= hem {
                    &skirt
                } else {
                    &below[g]
                }
            }
            _ => match shown.get(g).filter(|ts| !ts.is_empty()) {
                Some(ts) => ts,
                None => continue,
            },
        };
        let nearest = |p: Vec3, island: Option<usize>| {
            targets
                .iter()
                .filter(|x| island.is_none_or(|i| x.island == i))
                .map(|x| {
                    let (d, w) = closest(p, &x.p);
                    (d, w, *x)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
        };
        let Some((_, _, best)) = nearest(c, None) else { continue };
        let mut uv = [[0.0; 2]; 3];
        for k in 0..3 {
            let (_, w, x) = nearest(q[k], Some(best.island)).expect("the best target's island holds it");
            uv[k] = [0, 1].map(|j| x.uv[0][j] * w[0] + x.uv[1][j] * w[1] + x.uv[2][j] * w[2]);
        }
        out[best.slot].push(Tri { v: t.map(|v| v as u32), uv, lod: *lod });
    }
    Fit { tris: out, layers: r.layers.clone() }
}

/// Where a skirt stops hiding the legs: going down from its top, the first
/// height at which less than half the way round the body (in sixteenths,
/// round the human's middle) has a shown triangle of it (a knee-length robe's
/// panels hang lower, but a human's legs show between them).
fn hem(skirt: &[&Target]) -> f32 {
    let (lo, hi) = skirt.iter().flat_map(|t| t.p.iter().map(|p| p.y)).fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
    if skirt.is_empty() {
        return lo;
    }
    let spans: Vec<(f32, f32, usize)> = skirt
        .iter()
        .map(|t| {
            let m = (t.p[0] + t.p[1] + t.p[2]) / 3.0;
            let a = (m.x.atan2(m.z) / std::f32::consts::TAU + 0.5) * 16.0;
            let (y0, y1) = t.p.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
            (y0, y1, (a as usize).min(15))
        })
        .collect();
    let mut h = hi;
    while h > lo {
        let mut round = [false; 16];
        for &(y0, y1, a) in &spans {
            if y0 <= h && h <= y1 {
                round[a] = true;
            }
        }
        if round.iter().filter(|&&b| b).count() < 8 {
            return h;
        }
        h -= 0.01;
    }
    lo
}

/// How far `p` is from a segment's bone (its ends included).
fn seg_distance(f: &Frame, p: Vec3) -> f32 {
    let t = (p - f.a).dot(f.d).clamp(0.0, f.len);
    p.distance(f.a + f.d * t)
}

/// For a look at the fit (`dressview`'s pictures): each triangle of each of
/// `model`'s batches (body `id`'s), where it is fitted: 0 the body, 1 and 2
/// the arms, 3 and 4 the legs, 5 the head, 6 a part that wears nothing
/// (translucent, untextured), 7 something off the spine, 8 a part the body
/// keeps its own look on (`KEPT`). None for a body this cannot fit.
pub fn debug_groups(lib: &Library, id: i32, model: &esf::Model, cs: &CSprite<Ee>) -> Option<Vec<Vec<u8>>> {
    let mut rig = Rig::of_sprite(cs)?;
    rig.neck();
    let wearing = wearing(model, kept(id), |t| colours_of(lib, t));
    let verts: Vec<usize> = model.batches.iter().zip(&wearing).flat_map(|(b, on)| b.indices.chunks_exact(3).zip(on).filter(|(_, on)| **on).flat_map(|(t, _)| t.iter().map(|&v| v as usize))).collect();
    let body = frames(&rig, verts.iter().filter(|&&v| part_of(model, &rig, v) == Part::Body).map(|&v| (pos(model, v), Seg::Body)))[&Seg::Body];
    let on: Vec<(Vec3, usize)> = verts.iter().filter_map(|&v| bone_of(model, v).map(|b| (pos(model, v), b))).collect();
    rig.fold(&body, &on);
    Some(
        model
            .batches
            .iter()
            .zip(&wearing)
            .map(|(b, on)| {
                b.indices
                    .chunks_exact(3)
                    .zip(on)
                    .map(|(t, on)| {
                        if !on {
                            return if opaque(b) { 8 } else { 6 };
                        }
                        let t = [t[0] as usize, t[1] as usize, t[2] as usize];
                        match group_of(model, &rig, t) {
                            Some(Group::Body) => 0,
                            Some(Group::Limb(Limb::Arm(s))) => 1 + s,
                            Some(Group::Limb(Limb::Leg(s))) => 3 + s,
                            None if t.iter().filter(|&&v| part_of(model, &rig, v) == Part::Head).count() >= 2 => 5,
                            None => 7,
                        }
                    })
                    .collect()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A human-shaped skeleton (the root, a spine of three, the head; two
    /// arms of collarbone, upper arm, forearm, hand and item node; two legs
    /// of thigh, shin, ankle and toe) and a tail off the root.
    fn skeleton() -> (Vec<i32>, Vec<Vec3>, [i32; 10]) {
        let mut parent = Vec::new();
        let mut at = Vec::new();
        let mut add = |p: i32, x: f32, y: f32, z: f32| {
            parent.push(p);
            at.push(Vec3::new(x, y, z));
            parent.len() as i32 - 1
        };
        let root = add(-1, 0.0, 1.0, 0.0);
        let s1 = add(root, 0.0, 1.2, 0.0);
        let chest = add(s1, 0.0, 1.4, 0.0);
        let head = add(chest, 0.0, 1.6, 0.0);
        let mut hands = [0; 2];
        for (k, sx) in [-1.0f32, 1.0].into_iter().enumerate() {
            let col = add(chest, 0.05 * sx, 1.4, 0.0);
            let upper = add(col, 0.15 * sx, 1.4, 0.0);
            let fore = add(upper, 0.4 * sx, 1.4, 0.0);
            let hand = add(fore, 0.65 * sx, 1.4, 0.0);
            hands[k] = add(hand, 0.7 * sx, 1.4, 0.0);
        }
        let mut toes = [0; 2];
        for (k, sx) in [-1.0f32, 1.0].into_iter().enumerate() {
            let thigh = add(root, 0.1 * sx, 0.95, 0.0);
            let shin = add(thigh, 0.1 * sx, 0.55, 0.0);
            let ankle = add(shin, 0.1 * sx, 0.1, 0.0);
            toes[k] = add(ankle, 0.1 * sx, 0.0, 0.12);
        }
        let tail = add(root, 0.0, 0.95, -0.2);
        add(tail, 0.0, 0.9, -0.6);
        (parent, at, [hands[0], hands[1], -1, chest, -1, toes[0], toes[1], head, -1, -1])
    }

    /// The parts as the NodeIDList's anchors find them: the collarbones are
    /// the body's, the toes the feet's, the tail nobody's; and a list that
    /// does not name a hand finds nothing (the control).
    #[test]
    fn a_skeleton_reads_as_a_body() {
        let (parent, at, ids) = skeleton();
        let rig = Rig::of(&parent, &at, &ids).expect("a rig");
        assert_eq!(&rig.part[..4], &[Part::Body, Part::Body, Part::Body, Part::Head]);
        assert_eq!(rig.part[4], Part::Body, "a collarbone");
        assert_eq!(rig.part[5], Part::Limb(Limb::Arm(0), 0));
        assert_eq!(rig.part[6], Part::Limb(Limb::Arm(0), 1));
        assert_eq!(rig.part[7], Part::Limb(Limb::Arm(0), 2));
        assert_eq!(rig.part[8], Part::Limb(Limb::Arm(0), 2), "the item node is the hand's");
        let thigh = 14;
        assert_eq!(rig.part[thigh..thigh + 4], [Part::Limb(Limb::Leg(0), 0), Part::Limb(Limb::Leg(0), 1), Part::Limb(Limb::Leg(0), 2), Part::Limb(Limb::Leg(0), 2)]);
        assert_eq!(rig.part[22..], [Part::Other, Part::Other], "the tail");
        assert_eq!(rig.segs[&Seg::Body], (at[0], at[3]));
        assert_eq!(rig.segs[&Seg::Limb(Limb::Leg(0), 2)], (at[16], at[17]), "ankle to toe");
        let mut no_hand = ids;
        no_hand[1] = -1;
        assert!(Rig::of(&parent, &at, &no_hand).is_none());
        // the arms hang from the chest, right under the head: no neck
        let mut rig = rig;
        rig.neck();
        assert_eq!(rig.part[2], Part::Body);
    }

    /// A body on four legs is not read as one that wears armour: its
    /// "hands" (the forefeet) are on the ground (the control: the same
    /// skeleton with its hands raised is read).
    #[test]
    fn four_legs_wear_nothing() {
        let (parent, mut at, ids) = skeleton();
        // the arms hung down to the ground, the hands at the feet
        for (k, sx) in [(4usize, -1.0f32), (9, 1.0)] {
            for (j, y) in [1.3f32, 1.0, 0.5, 0.05, 0.0].into_iter().enumerate() {
                at[k + j] = Vec3::new(0.2 * sx, y, 0.5);
            }
        }
        assert!(Rig::of(&parent, &at, &ids).is_none());
        let (parent, at, ids) = skeleton();
        assert!(Rig::of(&parent, &at, &ids).is_some());
    }

    /// A neck between the shoulders and the head is the head's.
    #[test]
    fn a_neck_is_the_heads() {
        let (mut parent, mut at, mut ids) = skeleton();
        // a neck node between the chest (2) and the head (3)
        let neck = parent.len();
        parent.push(2);
        at.push(Vec3::new(0.0, 1.5, 0.0));
        parent[3] = neck as i32;
        ids[7] = 3;
        let mut rig = Rig::of(&parent, &at, &ids).expect("a rig");
        assert_eq!(rig.part[neck], Part::Body, "read from the skeleton alone, the spine's");
        rig.neck();
        assert_eq!(rig.part[neck], Part::Head);
        assert_eq!(rig.part[2], Part::Body, "the arms hang from the chest");
    }

    /// A hoofed leg (hip, knee forward, hock back, hoof) falls as a human's:
    /// the knee is the joint at 43% of the way down, the shin runs from it
    /// down to the hoof, past the hock; read from the bottom up instead, the
    /// hock would be the knee (the control: the hock is the shin's).
    #[test]
    fn a_hoofed_leg_falls_as_a_humans() {
        let (mut parent, mut at, mut ids) = skeleton();
        for s in 0..2usize {
            let sx = if s == 0 { -1.0 } else { 1.0 };
            let hip = parent.len();
            parent.extend([0, hip as i32, hip as i32 + 1, hip as i32 + 2]);
            at.extend([Vec3::new(0.13 * sx, 0.92, 0.0), Vec3::new(0.13 * sx, 0.56, 0.17), Vec3::new(0.13 * sx, 0.31, -0.07), Vec3::new(0.13 * sx, 0.06, -0.05)]);
            ids[5 + s] = hip as i32 + 3;
        }
        // the human legs left in place but not named: unassigned, off the root
        let rig = Rig::of(&parent, &at, &ids).expect("a rig");
        let first = parent.len() - 8;
        assert_eq!(rig.part[first], Part::Limb(Limb::Leg(0), 0), "the hip's bone is the thigh");
        assert_eq!(rig.part[first + 1], Part::Limb(Limb::Leg(0), 1), "the knee's the shin");
        assert_eq!(rig.part[first + 2], Part::Limb(Limb::Leg(0), 1), "the hock is the shin's");
        assert_eq!(rig.part[first + 3], Part::Limb(Limb::Leg(0), 2), "the hoof the foot");
    }

    /// Off the spine, a node whose skin hugs the torso becomes the torso's,
    /// and one whose skin reaches away (a tail) stays nobody's.
    #[test]
    fn what_hugs_the_torso_is_the_torsos() {
        let (parent, at, ids) = skeleton();
        let mut rig = Rig::of(&parent, &at, &ids).expect("a rig");
        let body = Frame::new(at[0], at[3]);
        // the torso's skin: 0.15 round the spine (on the root and the chest)
        let torso: Vec<(Vec3, usize)> = (0..12).map(|k| (Vec3::new(0.15 * (k as f32).cos(), 0.95 + 0.05 * k as f32, 0.15 * (k as f32).sin()), if k < 6 { 0 } else { 2 })).collect();
        // a tail's skin 0.2 to 0.56 behind the spine, down past the torso
        let tail: Vec<(Vec3, usize)> = (0..10).map(|k| (Vec3::new(0.0, 0.95 - 0.02 * k as f32, -0.2 - 0.04 * k as f32), 22 + (k >= 5) as usize)).collect();
        rig.fold(&body, &[torso.clone(), tail].concat());
        assert_eq!(rig.part[22], Part::Other);
        // the same node with its skin on the belly, under the torso's
        let belly: Vec<(Vec3, usize)> = (0..10).map(|k| (Vec3::new(0.01 * k as f32, 1.1, 0.14), 22)).collect();
        rig.fold(&body, &[torso, belly].concat());
        assert_eq!(rig.part[22..], [Part::Body, Part::Body]);
    }

    /// A frame carries a point to the same place on a segment twice as long
    /// and twice as thick; a point on the bone stays on it.
    #[test]
    fn frames_carry_along_and_round() {
        let mut a = Frame::new(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
        let mut b = Frame::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 0.0));
        a.r = [0.1, 0.1];
        b.r = [0.2, 0.2];
        assert_eq!(a.e1, Vec3::Z, "toward the front");
        let p = Vec3::new(0.05, 0.5, 0.1);
        let q = b.world(a.local(p));
        assert!(q.distance(Vec3::new(1.1, 1.0, 0.2)) < 1e-5, "{q}");
        assert!(b.world(a.local(Vec3::new(0.0, 0.25, 0.0))).distance(Vec3::new(1.0, 0.5, 0.0)) < 1e-5);
        // a bone pointing forward is measured round from up
        assert_eq!(Frame::new(Vec3::ZERO, Vec3::Z).e1, Vec3::Y);
    }

    /// The closest point of a triangle, inside it and off a corner.
    #[test]
    fn closest_points() {
        let t = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let (d, w) = closest(Vec3::new(0.25, 0.25, 1.0), &t);
        assert!((d - 1.0).abs() < 1e-6 && (w[1] - 0.25).abs() < 1e-6 && (w[2] - 0.25).abs() < 1e-6);
        let (d, w) = closest(Vec3::new(2.0, -1.0, 0.0), &t);
        assert!((d - 2.0).abs() < 1e-6 && w == [0.0, 1.0, 0.0], "{d} {w:?}");
    }

    /// A triangle shows when most of its points are on opaque texels.
    #[test]
    fn see_through_texels() {
        // the left half opaque, the right see-through
        let a = Alpha { w: 4, h: 4, a: (0..16).map(|i| if i % 4 < 2 { 0x80 } else { 0 }).collect() };
        assert!(a.shows(&[[0.0, 0.0], [0.4, 0.0], [0.0, 0.9]]));
        assert!(!a.shows(&[[0.6, 0.0], [0.9, 0.0], [0.6, 0.9]]));
        // repeated past 1
        assert!(a.shows(&[[1.0, 1.0], [1.4, 1.0], [1.0, 1.9]]));
    }

    /// The bodies of the server's races with no text slot (the kappa,
    /// brownie, pixie, fairy, minotaur, dryad, skeleton, fungusman,
    /// gargoyle) and the hill giant.
    const NINE: [u32; 10] = [0x31A15F17, 0x4376EB52, 0x94BA77CD, 0x7A85646C, 0xC5CBF4CF, 0x70EFCE05, 0x6AF98A81, 0x4A4D88FB, 0xB4D2D132, 0xFFB1EA2C];

    /// The files a body needs off the disc, when it is there.
    fn disc() -> Option<Library> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut d = iso::Iso::open(&iso).expect("the disc");
        let mut lib = Library::new();
        for f in ["CHARCUST.CSF", "CHARFACE.CSF", "CHAR.ESF"] {
            lib.add(f, esf::Container::open(d.read(f).expect(f)).expect(f));
        }
        Some(lib)
    }

    /// Two models alike to the bit: vertices, batches, layers, counts.
    fn same(a: &esf::Model, b: &esf::Model) -> bool {
        let bits2 = |v: &[[f32; 2]]| v.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
        let bits3 = |v: &[[f32; 3]]| v.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
        let bits4 = |v: &[[f32; 4]]| v.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
        let batches = |m: &esf::Model| m.batches.iter().map(|b| (b.material, b.layers.clone(), b.indices.clone(), b.lod)).collect::<Vec<_>>();
        bits3(&a.pos) == bits3(&b.pos)
            && bits2(&a.uv) == bits2(&b.uv)
            && bits3(&a.normal) == bits3(&b.normal)
            && bits4(&a.color) == bits4(&b.color)
            && a.bones == b.bones
            && a.weights == b.weights
            && batches(a) == batches(b)
            && (a.parts, a.triangles, a.skinned, a.missing) == (b.parts, b.triangles, b.skinned, b.missing)
            && a.lod_dists.iter().map(|d| d.to_bits()).eq(b.lod_dists.iter().map(|d| d.to_bits()))
    }

    /// Every CSprite on the disc dressed by `play::dress_look` against the
    /// path before it (`apply_look_with`'s changes put on by `play::dress`),
    /// in looks of every kind: a body with all five worn slots of its own
    /// (every player race's) comes out byte for byte as before in all of them
    /// (it is never fitted); one lacking some or all (`lacking`) keeps its own
    /// batches and vertices as `apply_look_with` dresses them (a slotless one's
    /// as they were, the skeleton's losing only triangles, those under its
    /// armour) and only gains made-up batches after them, each of a slot it
    /// lacks, and in a look with no art is as before. The nine and the hill
    /// giant are fitted in every look, and each gains armour in a set; every
    /// other body in plate. The control: with the dressing off, a slotless body
    /// in plate is as before.
    #[test]
    fn only_bodies_lacking_slots_change() {
        let Some(lib) = disc() else { return };
        let c = lib.file(lib.file_named("CHAR.ESF").expect("CHAR.ESF"));
        let mut ids: Vec<i32> = c.dict.iter().filter(|(_, n)| c.node(**n).ty == 0x2700).map(|(&k, _)| k).collect();
        ids.sort_unstable();
        let mut looks: Vec<Look> = vec![Look { robe: -1, ..Default::default() }];
        looks.extend((1..9u8).map(|s| Look { armor: [s; 6], armor_tint: [s; 6], hair: [s % 8, 2, 3], face: s % 4, robe: -1, ..Default::default() }));
        looks.extend((0..4i8).map(|r| Look { armor: [4, 3, 2, 1, 5, 6], robe: r, robe_tint: 3, ..Default::default() }));
        looks.push(Look { armor: [4; 6], armor_color: Some([[0x30, 0x70, 0xD0, 255]; 6]), robe: -1, ..Default::default() });
        let t = Tables::retail();
        let has = |id: u32| lib.surface(id as i32).is_some();
        let (mut slotted, mut slotless, mut partial, mut dressed) = (0, 0, 0, Vec::new());
        for id in ids {
            let r = lib.lookup(id).expect("a CSprite");
            let Some(model) = lib.build_model(r) else { continue };
            let mut anims = vi::hsprite::AnimScene::default();
            let Some(cs) = vi::character::load_csprite(&lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut Rand(1)) else { continue };
            let lacks = lacking(&model, &cs);
            let whole = !lacks.contains(&true);
            let nine = NINE.contains(&(id as u32));
            let tried: Vec<&Look> = if whole || nine { looks.iter().collect() } else { vec![&looks[0], &looks[4]] };
            let mut gained = false;
            for (k, look) in tried.iter().enumerate() {
                let helm = k % 3 == 1;
                let mut before = model.clone();
                crate::play::dress(&mut before, &vi::cust::apply_look_with(&cs, look, helm, has, &t));
                let mut after = model.clone();
                crate::play::dress_look(&lib, id, &mut after, &cs, look, helm, &t, true);
                if whole {
                    assert!(same(&before, &after), "{id:#010x} has its five slots and changed in {look:?}");
                    continue;
                }
                if is_slotless(&cs) {
                    assert!(same(&before, &model), "a slotless body has nothing of its own to dress");
                }
                let n = before.batches.len();
                assert_eq!(after.batches[..n].len(), n);
                let mut own = after.clone();
                own.batches.truncate(n);
                own.pos.truncate(before.pos.len());
                own.uv.truncate(before.uv.len());
                own.normal.truncate(before.normal.len());
                own.color.truncate(before.color.len());
                own.bones.truncate(before.bones.len());
                own.weights.truncate(before.weights.len());
                own.triangles = before.triangles;
                if SHELLED.contains(&(id as u32)) {
                    // only triangles taken out, each batch's rest in order
                    for (b, o) in before.batches.iter().zip(own.batches.iter_mut()) {
                        let mut left = o.indices.chunks_exact(3).peekable();
                        for t in b.indices.chunks_exact(3) {
                            if left.peek() == Some(&t) {
                                left.next();
                            }
                        }
                        assert!(left.next().is_none(), "{id:#010x}: a triangle of its own put back changed");
                        o.indices = b.indices.clone();
                    }
                }
                assert!(same(&before, &own), "{id:#010x}: its own batches and vertices as they were");
                assert!(after.batches[n..].iter().all(|b| (0..SLOTS).any(|i| lacks[i] && b.material == ENTRY + i as i32)), "{id:#010x}: made-up batches of the slots it lacks ({lacks:?}) alone");
                if look.armor[..5].iter().all(|&s| s == 0) && look.robe < 0 {
                    assert!(same(&before, &after), "{id:#010x}: no art, no change");
                }
                gained |= after.batches.len() > n;
            }
            if whole {
                slotted += 1;
                continue;
            }
            if is_slotless(&cs) {
                slotless += 1;
            } else {
                partial += 1;
            }
            if gained {
                dressed.push(id as u32);
            }
            if nine {
                assert!(gained, "{id:#010x} wears nothing");
            }
        }
        eprintln!("{slotted} bodies with their five slots unchanged; {slotless} with none and {partial} with some, {} of them dressed", dressed.len());
        assert!(slotted >= 60 && partial >= 90 && dressed.len() >= NINE.len());
        // the control: turned off, the kappa in plate is as it was
        set_enabled(false);
        let r = lib.lookup(NINE[0] as i32).unwrap();
        let model = lib.build_model(r).unwrap();
        let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        let mut off = model.clone();
        crate::play::dress_look(&lib, NINE[0] as i32, &mut off, &cs, &looks[4], false, &t, true);
        set_enabled(true);
        assert!(same(&model, &off));
    }

    /// The kappa fitted to the human: its torso, arms, legs and feet take
    /// the chest, bracer, gloves, legs and boots slots, its head none; in
    /// plate the chest is the slot with the most triangles. The fit is made
    /// once per look (the same `Rc` again).
    #[test]
    fn the_kappa_wears_each_slot() {
        let Some(lib) = disc() else { return };
        let id = NINE[0] as i32;
        let r = lib.lookup(id).unwrap();
        let model = lib.build_model(r).unwrap();
        let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        let t = Tables::retail();
        let has = |id: u32| lib.surface(id as i32).is_some();
        let look = Look { armor: [4; 6], robe: -1, ..Default::default() };
        let art = worn(&vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, &look, false, has, &t)).map(|(t, _)| t);
        let fit = fit_of(&lib, id, &model, &cs, art, false).expect("a fit");
        let n: Vec<usize> = fit.tris.iter().map(Vec::len).collect();
        assert!(n.iter().all(|&k| k > 0), "{n:?}");
        assert!(n[0] > n[1] && n[0] > n[2] && n[0] > n[4], "{n:?}");
        assert!(Rc::ptr_eq(&fit, &fit_of(&lib, id, &model, &cs, art, false).unwrap()));
        // its head and neck keep their own look (no fitted triangle has two
        // corners on them: the six at the collar, in the head's material, sit
        // on the spine), and its torso's palette entry wears armour
        let mut rig = Rig::of_sprite(&cs).unwrap();
        rig.neck();
        let on_head = |v: u32| bone_of(&model, v as usize).is_some_and(|b| rig.part[b] == Part::Head);
        assert!(fit.tris.iter().flatten().all(|t| t.v.iter().filter(|&&v| on_head(v)).count() < 2));
        let of = |e: i32| model.batches.iter().filter(|b| b.material == e).flat_map(|b| b.indices.iter().copied()).collect::<std::collections::HashSet<u32>>();
        let (head, torso) = ([of(2), of(3)].concat_sets(), of(0));
        let in_head = fit.tris.iter().flatten().filter(|t| t.v.iter().all(|v| head.contains(v))).count();
        assert!(in_head <= 6, "{in_head} of the head's triangles fitted");
        assert!(fit.tris.iter().flatten().any(|t| t.v.iter().all(|v| torso.contains(v))));
        // the control: read without the neck rule, the head's material wears more
        assert!(Rig::of_sprite(&cs).unwrap().part[4] == Part::Body && rig.part[4] == Part::Head);
    }

    /// A triangle skinned to the spine that runs along an arm from the
    /// shoulder to the elbow is the arm's (the fungusman's upper arm, from its
    /// shoulder on the spine straight to its elbow); one by the shoulder
    /// joint, or out along the arm short of the elbow, stays the torso's, and
    /// one skinned to the arm is the arm's (the controls).
    #[test]
    fn a_spine_triangle_out_along_an_arm_is_the_arms() {
        let (parent, at, ids) = skeleton();
        let rig = Rig::of(&parent, &at, &ids).expect("a rig");
        let mut m = esf::Model::default();
        fn vert(m: &mut esf::Model, p: [f32; 3], bone: u8) -> usize {
            m.pos.push(p);
            m.uv.push([0.0; 2]);
            m.normal.push([0.0, 1.0, 0.0]);
            m.color.push([1.0; 4]);
            m.bones.push([bone, 255, 255, 255]);
            m.weights.push([127, 0, 0, 0]);
            m.pos.len() - 1
        }
        // the chest is node 2; the right arm's upper arm 10, forearm 11
        // shoulder to elbow: two corners on the chest, one on the forearm
        let long = [vert(&mut m, [0.2, 1.45, 0.03], 2), vert(&mut m, [0.2, 1.35, -0.03], 2), vert(&mut m, [0.42, 1.4, 0.04], 11)];
        // by the shoulder joint, all on the chest
        let near = [vert(&mut m, [0.12, 1.45, 0.05], 2), vert(&mut m, [0.12, 1.3, 0.05], 2), vert(&mut m, [0.16, 1.38, 0.06], 2)];
        // on the forearm
        let fore = [vert(&mut m, [0.45, 1.42, 0.03], 11), vert(&mut m, [0.55, 1.42, 0.03], 11), vert(&mut m, [0.5, 1.37, -0.03], 11)];
        // out along the arm, on the chest, but reaching no elbow
        let short = [vert(&mut m, [0.25, 1.45, 0.05], 2), vert(&mut m, [0.3, 1.35, 0.05], 2), vert(&mut m, [0.28, 1.4, -0.05], 2)];
        // the other arm's mirror image of the first (its forearm is node 6)
        let mirror = long.map(|v| {
            let (p, b) = (m.pos[v], m.bones[v][0]);
            vert(&mut m, [-p[0], p[1], p[2]], if b == 2 { 2 } else { 6 })
        });
        assert_eq!(rig.part[10], Part::Limb(Limb::Arm(1), 0));
        assert_eq!(weight_group(&m, &rig, long), Some(Group::Body), "by its weights, the torso's");
        assert_eq!(group_of(&m, &rig, long), Some(Group::Limb(Limb::Arm(1))));
        assert_eq!(group_of(&m, &rig, near), Some(Group::Body));
        assert_eq!(group_of(&m, &rig, short), Some(Group::Body), "a shoulder's, or a shell's beside the arm");
        assert_eq!(group_of(&m, &rig, fore), Some(Group::Limb(Limb::Arm(1))));
        assert_eq!(group_of(&m, &rig, mirror), Some(Group::Limb(Limb::Arm(0))));
    }

    /// The triangles of a kept entry keep the body's look, but one drawn in
    /// the skin's colours (nearer the mean of the skin's triangles on that
    /// texture than of the part's); a kept entry on a texture of its own
    /// keeps all of it, and with nothing kept every opaque triangle wears
    /// (the controls).
    #[test]
    fn kept_parts_but_where_they_show_the_skin() {
        let mut m = esf::Model::default();
        for p in [[0.0, 0.0], [0.4, 0.0], [0.0, 0.9], [0.6, 0.1], [0.9, 0.1], [0.6, 0.9], [0.1, 0.1], [0.3, 0.1], [0.1, 0.3]] {
            m.pos.push([p[0], p[1], 0.0]);
            m.uv.push(p);
            m.normal.push([0.0, 0.0, 1.0]);
            m.color.push([1.0; 4]);
            m.bones.push([0, 255, 255, 255]);
            m.weights.push([127, 0, 0, 0]);
        }
        let layer = |texture: i32| esf::Layer { flags: 0, texture, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
        // texture 7: brown bark on its left half, blue on its right; entry 1
        // the skin on the bark; entry 0 a part in the blue, with one triangle
        // in the bark; entry 2 a part on texture 9, of its own
        m.batches.push(esf::Batch { material: 1, layers: vec![layer(7)], indices: vec![0, 1, 2], lod: None });
        m.batches.push(esf::Batch { material: 0, layers: vec![layer(7)], indices: vec![3, 4, 5, 3, 5, 4, 6, 7, 8], lod: None });
        m.batches.push(esf::Batch { material: 2, layers: vec![layer(9)], indices: vec![6, 7, 8], lod: None });
        let colours = |t: i32| (t == 7 || t == 9).then(|| Rc::new(Texels { w: 2, h: 1, rgb: vec![[120, 90, 80], [40, 90, 200]] }));
        assert_eq!(wearing(&m, &[0, 2], colours), vec![vec![true], vec![false, false, true], vec![false]]);
        assert_eq!(wearing(&m, &[], colours), vec![vec![true], vec![true; 3], vec![true]]);
        // a translucent entry wears nothing either way
        m.batches[0].layers[0].blend = 3;
        assert_eq!(wearing(&m, &[], colours)[0], vec![false]);
    }

    /// The fungusman in Fun's look (plate on all six, the live game's white):
    /// its bag and strap (entry 2) and its fungus (entry 0, but the one strip
    /// of its right upper arm the fungus material draws in the skin's bark)
    /// wear nothing; its skin wears every slot but the gloves (its hands are
    /// the fungus's claws, bar a sliver at the wrist), and its upper arms
    /// take the human's sleeve, on the arm. The controls: with nothing kept,
    /// the bag and the fungus wear armour; read by its weights alone, the
    /// upper arm is the torso's.
    #[test]
    fn the_fungusman_keeps_its_bag_and_fungus() {
        let Some(lib) = disc() else { return };
        let id = 0x4A4D88FBu32 as i32;
        let r = lib.lookup(id).unwrap();
        let model = lib.build_model(r).unwrap();
        let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        let t = Tables::retail();
        let has = |id: u32| lib.surface(id as i32).is_some();
        let look = Look { armor: [4; 6], armor_color: Some([[255; 4]; 6]), robe: -1, ..Default::default() };
        let art = worn(&vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, &look, false, has, &t)).map(|(t, _)| t);
        let dressed = fit_of(&lib, id, &model, &cs, art, false).expect("a fit");
        let mut entry = vec![-1; model.pos.len()];
        for b in &model.batches {
            for &v in &b.indices {
                entry[v as usize] = b.material;
            }
        }
        let of = |fit: &Fit, e: i32| fit.tris.iter().map(|ts| ts.iter().filter(|t| t.v.iter().all(|&v| entry[v as usize] == e)).count()).collect::<Vec<_>>();
        assert_eq!(of(&dressed, 2), [0; SLOTS], "the bag");
        let fungus: usize = of(&dressed, 0).iter().sum();
        assert_eq!(fungus, 1, "the fungus but the arm's strip of bark");
        let skin = of(&dressed, 1);
        assert!(skin[0] > 50 && skin[1] > 5 && skin[3] > 50 && skin[4] > 20, "{skin:?}");
        // the control: nothing kept
        let r0 = REFS.with(|r| r.borrow().get(&(lib.view(), false)).cloned()).flatten().expect("the human");
        let all = Carried::of(&r0, &model, &cs, &wearing(&model, &[], |t| colours_of(&lib, t))).unwrap();
        let alpha: [Option<Rc<Alpha>>; SLOTS] = std::array::from_fn(|i| alpha_of(&lib, art[i].map_or(r0.own[i], |t| t as i32)));
        let bare = fit(&r0, &all, &alpha, false);
        assert!(of(&bare, 2).iter().sum::<usize>() > 40 && of(&bare, 0).iter().sum::<usize>() > 100);
        // the upper arms: their skin's triangles out along the arm, by their
        // weights the torso's, are the arms'
        let mut rig = Rig::of_sprite(&cs).unwrap();
        rig.neck();
        let groups = debug_groups(&lib, id, &model, &cs).unwrap();
        let (mut along, mut on_arm) = (0, 0);
        for (b, g) in model.batches.iter().zip(&groups).filter(|(b, _)| b.material == 1) {
            for (t, &code) in b.indices.chunks_exact(3).zip(g) {
                let t = [t[0] as usize, t[1] as usize, t[2] as usize];
                if rig.arm_along(t.map(|v| pos(&model, v))).is_some() && weight_group(&model, &rig, t) == Some(Group::Body) {
                    along += 1;
                    on_arm += (code == 1 || code == 2) as usize;
                }
            }
        }
        assert!(along >= 10 && on_arm == along, "{on_arm} of {along}");
    }

    /// The skeleton wears a human's armour, not its own bones painted: in
    /// plate, each slot's batch is HumanMale's slot triangles as they are
    /// (place, bones, weights), each with its inside (turned round, its
    /// normals reversed: the shell is hollow), and its own triangles under
    /// them are taken out; in a chest alone, only the chest's, its legs,
    /// hands and feet bare. The controls: the kappa, on a skeleton of its
    /// own, is painted as before, one face a triangle; bare, the skeleton is
    /// as it was.
    #[test]
    fn the_skeleton_wears_a_humans_armour() {
        let Some(lib) = disc() else { return };
        let t = Tables::retail();
        let load = |id: i32| {
            let r = lib.lookup(id).unwrap();
            let model = lib.build_model(r).unwrap();
            let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
            (model, cs)
        };
        let id = 0x6AF98A81u32 as i32;
        let (model, cs) = load(id);
        let human = Human::load(&lib, false).unwrap();
        let plate = Look { armor: [4; 6], robe: -1, ..Default::default() };
        let mut dressed = model.clone();
        crate::play::dress_look(&lib, id, &mut dressed, &cs, &plate, false, &t, true);
        let n = model.batches.len();
        let made = &dressed.batches[n..];
        assert_eq!(made.iter().map(|b| b.material).collect::<Vec<_>>(), (0..SLOTS as i32).map(|i| ENTRY + i).collect::<Vec<_>>());
        for (i, b) in made.iter().enumerate() {
            let entry = human.cs.text_slots[i].0;
            let theirs: Vec<[f32; 3]> = near_triangles(&human.model).filter(|(hb, _)| hb.material == entry).flat_map(|(_, t)| t.map(|v| human.model.pos[v])).collect();
            let ours: Vec<[f32; 3]> = b.indices.iter().map(|&v| dressed.pos[v as usize]).collect();
            assert!(!ours.is_empty() && ours.len() <= 2 * theirs.len());
            assert!(ours.iter().all(|p| theirs.contains(p)), "slot {i}: the human's own places");
            // each triangle twice: its inside turned round, its normals reversed
            for t in b.indices.chunks_exact(6) {
                let (out, inside) = (&t[..3], &t[3..]);
                let at = |v: u32| (dressed.pos[v as usize], dressed.uv[v as usize], dressed.bones[v as usize], dressed.weights[v as usize]);
                assert_eq!([at(inside[0]), at(inside[1]), at(inside[2])], [at(out[0]), at(out[2]), at(out[1])], "slot {i}: the inside turned round");
                assert!([(inside[0], out[0]), (inside[1], out[2]), (inside[2], out[1])].iter().all(|&(a, b)| dressed.normal[a as usize] == dressed.normal[b as usize].map(|n| -n)), "slot {i}: the inside's normals reversed");
            }
            let bones: std::collections::HashSet<u8> = b.indices.iter().flat_map(|&v| dressed.bones[v as usize]).filter(|&k| k != 255).collect();
            assert!(!bones.contains(&5), "slot {i}: nothing on the skull's top, the one node the skeletons differ in");
        }
        let own = |m: &esf::Model| m.batches[..n].iter().map(|b| b.indices.len() / 3).sum::<usize>();
        let hidden = own(&model) - own(&dressed);
        assert!(hidden > 400, "{hidden} of its own triangles hidden in plate");
        // a chest alone: the chest's batch, and fewer of its own hidden
        let chest = Look { armor: [4, 0, 0, 0, 0, 0], robe: -1, ..Default::default() };
        let mut top = model.clone();
        crate::play::dress_look(&lib, id, &mut top, &cs, &chest, false, &t, true);
        assert_eq!(top.batches[n..].iter().map(|b| b.material).collect::<Vec<_>>(), vec![ENTRY]);
        assert!(own(&model) - own(&top) < hidden && own(&model) > own(&top));
        // the controls
        let mut bare = model.clone();
        crate::play::dress_look(&lib, id, &mut bare, &cs, &Look { robe: -1, ..Default::default() }, false, &t, true);
        assert!(same(&bare, &model));
        let kappa = NINE[0] as i32;
        let (km, kcs) = load(kappa);
        assert!(shell_of(&lib, kappa, &kcs, [Some(0x40B0DDA7), None, None, None, None]).is_none(), "the kappa's skeleton is its own");
        let mut k = km.clone();
        crate::play::dress_look(&lib, kappa, &mut k, &kcs, &plate, false, &t, true);
        assert_eq!(k.pos[..km.pos.len()], km.pos[..], "painted on its own vertices");
        assert!(k.batches[km.batches.len()..].iter().flat_map(|b| &b.indices).all(|&v| (v as usize) >= km.pos.len()));
        assert!(k.batches[..km.batches.len()].iter().zip(&km.batches).all(|(a, b)| a.indices == b.indices), "none of its own hidden");
        let turned = |b: &esf::Batch| b.indices.chunks_exact(6).filter(|t| [t[3], t[4], t[5]].map(|v| k.pos[v as usize]) == [t[0], t[2], t[1]].map(|v| k.pos[v as usize])).count();
        assert_eq!(k.batches[km.batches.len()..].iter().map(turned).sum::<usize>(), 0, "one face a triangle on the kappa");
    }

    /// The Jal'Raeth's bodies name no helm node (slot 8): a player's helm goes on
    /// their node of the human's helm node's name, a child of their head, on each of
    /// the three; an NPC's keeps none, as a PS2 draws it, and the kappa (named in no
    /// list) keeps its own word (the controls).
    #[test]
    fn a_jalraeths_helm_goes_on_its_head() {
        let Some(lib) = disc() else { return };
        for id in HEAD_HELMS {
            let r = lib.lookup(*id as i32).unwrap();
            let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
            assert_eq!(cs.node_ids[8], -1, "{id:#010x} names no helm node");
            assert!(cs.node_ids[7] >= 0);
            let named = named_helm_node(&lib, *id, false).expect("the human's helm node by name");
            assert_eq!(helm_node(&lib, *id, &cs, true), named, "{id:#010x}: a player's on its node of the human's helm node's name");
            assert_eq!(cs.h.nodes[named as usize].parent, cs.node_ids[7], "a child of the head");
            assert_eq!(helm_node(&lib, *id, &cs, false), -1, "{id:#010x}: an NPC's none");
        }
        // the control: a body in no list (the stone golem, which no race's options
        // offer) keeps its own word
        let golem = 0x87CE_5F73u32;
        let r = lib.lookup(golem as i32).unwrap();
        let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        assert_eq!(helm_node(&lib, golem, &cs, true), cs.node_ids[8], "the control: the golem as before");
    }

    /// The Jal'Raeth in plate wear the human's chest, bracer and gloves carried onto
    /// their skeleton by bone names: three made-up batches (no legs or boots: theirs
    /// float crossed), every corner on one of the body's own nodes; bare, nothing
    /// (the control), and the fit by limbs had found no hands on them.
    #[test]
    fn a_jalraeth_wears_a_humans_upper_armour() {
        let Some(lib) = disc() else { return };
        let t = Tables::retail();
        for id in NAMED_SHELLS.iter().map(|&b| b as i32) {
            let r = lib.lookup(id).unwrap();
            let model = lib.build_model(r).unwrap();
            let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
            assert!(is_slotless(&cs));
            let plate = Look { armor: [4; 6], robe: -1, ..Default::default() };
            let mut dressed = model.clone();
            crate::play::dress_look(&lib, id, &mut dressed, &cs, &plate, false, &t, true);
            let n = model.batches.len();
            let made: Vec<i32> = dressed.batches[n..].iter().map(|b| b.material).collect();
            assert_eq!(made, vec![ENTRY, ENTRY + 1, ENTRY + 2], "{id:#010x}: chest, bracer and gloves");
            let nodes = cs.h.nodes.len() as u8;
            for b in &dressed.batches[n..] {
                assert!(b.indices.iter().all(|&v| dressed.bones[v as usize].iter().zip(dressed.weights[v as usize]).all(|(&j, w)| w == 0 || j < nodes)), "{id:#010x}: on its own nodes");
            }
            let mut bare = model.clone();
            crate::play::dress_look(&lib, id, &mut bare, &cs, &Look { robe: -1, ..Default::default() }, false, &t, true);
            assert!(same(&bare, &model), "{id:#010x}: bare, as it was");
        }
    }

    /// A great skeleton (the Skeleton race's larger bodies) wears the troll's
    /// armour as the skeleton wears the human's: in plate, each slot's batch is
    /// the troll's slot triangles as they are, each with its inside, and its
    /// own triangles under them are taken out. Its skeleton is the troll's (the
    /// same parents and bind poses where armour is skinned), not the human's
    /// (the control: the human's shell fits it nowhere, so before the troll was
    /// a shell's source it was painted on its own bones).
    #[test]
    fn a_great_skeleton_wears_the_trolls_armour() {
        let Some(lib) = disc() else { return };
        let t = Tables::retail();
        let id = 0x6AF98A82u32 as i32;
        let r = lib.lookup(id).unwrap();
        let model = lib.build_model(r).unwrap();
        let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        assert!(is_slotless(&cs));
        let human = Human::load(&lib, false).unwrap();
        assert!(!same_skeleton(&human.cs, &cs, std::iter::empty()), "the control: not the human's skeleton");
        let troll = Human::load_id(&lib, SHELL_BODIES[0]).unwrap();
        assert!(same_skeleton(&troll.cs, &cs, std::iter::empty()));
        let plate = Look { armor: [4; 6], robe: -1, ..Default::default() };
        let mut dressed = model.clone();
        crate::play::dress_look(&lib, id, &mut dressed, &cs, &plate, false, &t, true);
        let n = model.batches.len();
        let made = &dressed.batches[n..];
        assert_eq!(made.iter().map(|b| b.material).collect::<Vec<_>>(), (0..SLOTS as i32).map(|i| ENTRY + i).collect::<Vec<_>>());
        for (i, b) in made.iter().enumerate() {
            let entry = troll.cs.text_slots[i].0;
            let theirs: Vec<[f32; 3]> = near_triangles(&troll.model).filter(|(tb, _)| tb.material == entry).flat_map(|(_, t)| t.map(|v| troll.model.pos[v])).collect();
            let ours: Vec<[f32; 3]> = b.indices.iter().map(|&v| dressed.pos[v as usize]).collect();
            assert!(!ours.is_empty() && ours.len() <= 2 * theirs.len(), "slot {i}");
            assert!(ours.iter().all(|p| theirs.contains(p)), "slot {i}: the troll's own places");
            assert_eq!(b.indices.len() % 6, 0, "slot {i}: each triangle with its inside");
        }
        let own = |m: &esf::Model| m.batches[..n].iter().map(|b| b.indices.len() / 3).sum::<usize>();
        assert!(own(&model) - own(&dressed) > 300, "{} of its own triangles hidden in plate", own(&model) - own(&dressed));
        let mut bare = model.clone();
        crate::play::dress_look(&lib, id, &mut bare, &cs, &Look { robe: -1, ..Default::default() }, false, &t, true);
        assert!(same(&bare, &model), "bare: as it was");
    }

    /// The 2002 disc's copies of the character files standing in for
    /// Frontiers' (`disc`), as the world's library has them in that art
    /// (`era::install`): `$EQOA_2002_ISO`, else the USA disc under
    /// `../decomp-builds/discs/eqoa-usa/`. None (skipped, loudly) without
    /// either disc.
    fn disc_2002() -> Option<Library> {
        let mut lib = disc()?;
        let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
            let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../decomp-builds/discs/eqoa-usa");
            std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        });
        let Some(mut d) = y2002.and_then(|p| iso::Iso::open(p).ok()) else {
            eprintln!("SKIP: no 2002 disc (set EQOA_2002_ISO)");
            return None;
        };
        for f in 0..lib.files.len() {
            let name = lib.files[f].0.clone();
            if let Ok(bytes) = d.read(&name) {
                lib.stand_in(f, esf::Container::open(bytes).expect(&name));
            }
        }
        Some(lib)
    }

    /// Where a player's skeleton wears its helm (`helm_node`): Frontiers'
    /// skeleton names the human's helm node (5) itself, for a player and an
    /// NPC alike; the 2002 disc's names none, and a player's wears it on the
    /// 2002 HumanMale's (6), the same node of the same skeleton, the skull,
    /// standing where Frontiers' skeleton's does (Frontiers' 3 cm the
    /// higher), while an NPC's keeps none, as a PS2 draws it. The controls:
    /// the 2002 brownie, on the same skeleton and naming none, is not dressed
    /// as a human and keeps none; a human keeps its own in both arts. Over
    /// every CSprite on either disc in either art, a player's or not, the
    /// only bodies whose helm goes elsewhere than their own word are the
    /// skeletons a player may wear (`SHELLED`: the Skeleton race's own and
    /// the three its options offer), each in the 2002 disc's art alone and
    /// each onto the 2002 human's node 6: on both discs the four are one
    /// skeleton, Frontiers' each naming node 5 and the 2002 disc's none; and
    /// the Jal'Raeth's three (`HEAD_HELMS`), a player's in either art, which
    /// name no helm node: onto their node 6, named as the human's helm node.
    /// Needs both discs (`disc_2002`; without them it passes having checked
    /// nothing, and says so only under `--nocapture`).
    #[test]
    fn a_skeletons_helm_goes_on_the_humans_head() {
        let Some(lib) = disc_2002() else { return };
        let sprite = |id: u32| {
            let r = lib.lookup(id as i32).unwrap();
            vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap()
        };
        let (skeleton, brownie, human) = (0x6AF9_8A81u32, 0x4376_EB52u32, vi::hash::hash_resource_id("HumanMale"));
        let mut heights = Vec::new();
        for (era, want) in [(Era::Frontiers, 5), (Era::Y2002, 6)] {
            lib.set_view(era);
            let (cs, h) = (sprite(skeleton), sprite(human));
            assert_eq!(h.node_ids[8], want, "{era:?}: the human's helm node");
            assert_eq!(helm_node(&lib, human, &h, false), want, "{era:?}: a human's own");
            assert_eq!(cs.node_ids[8], if era == Era::Frontiers { 5 } else { -1 }, "{era:?}: the skeleton's own word");
            assert_eq!(helm_node(&lib, skeleton, &cs, true), want, "{era:?}: a player's skeleton");
            assert_eq!(helm_node(&lib, skeleton, &cs, false), cs.node_ids[8], "{era:?}: an NPC's keeps its own word");
            // the brownie, which names none either, hangs a player's helm on its
            // seat's node (`HEAD_SEATS`), not the human's
            let b = sprite(brownie);
            assert_eq!((b.node_ids[8], helm_node(&lib, brownie, &b, true)), (-1, named_helm_node(&lib, brownie, false).unwrap()), "{era:?}: the brownie");
            assert_eq!(helm_node(&lib, brownie, &b, false), -1, "{era:?}: an NPC brownie's none");
            heights.push(-cs.h.nodes[want as usize].inv_bind_world.to_cols()[3][1]);
        }
        lib.set_view(Era::Y2002);
        assert!(same_skeleton(&sprite(human), &sprite(brownie), std::iter::empty()), "the 2002 brownie is on the human's skeleton too");
        assert!((heights[0] - 1.83).abs() < 0.005 && (heights[1] - 1.80).abs() < 0.005, "the skull, at {heights:?}");
        // every CSprite on either disc, a player's or not, in either art
        // (built in the art it is drawn in, as the world builds it): its own
        // word, but a player's skeletons in the 2002 disc's art
        let mut ids: Vec<u32> = lib.files.iter().flat_map(|(_, c)| c.dict.iter().filter(|&(_, &n)| c.node(n).ty == 0x2700).map(|(&id, _)| id as u32)).collect();
        ids.sort();
        ids.dedup();
        let mut checked = 0;
        let (mut moved, mut seated) = (Vec::new(), 0);
        for era in [Era::Frontiers, Era::Y2002] {
            lib.set_view(era);
            for &id in &ids {
                lib.with_view(lib.view_for(id as i32), || {
                    let Some(cs) = lib.lookup(id as i32).and_then(|r| vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1))) else { return };
                    for player in [false, true] {
                        let node = helm_node(&lib, id, &cs, player);
                        // a player's in a body of a server's race that names no
                        // helm node (`HEAD_SEATS`): on its seat's node, in either art
                        if player && cs.node_ids[8] < 0 && let Some(s) = head_seat(id) {
                            assert_eq!(Some(node), seat_node(&lib, id, &cs, s), "{era:?} {id:#010x}: on its seat's node");
                            seated += 1;
                        } else if node != cs.node_ids[8] {
                            moved.push((era, id, player, cs.node_ids[8], node));
                        }
                        checked += 1;
                    }
                });
            }
        }
        assert!(seated >= 240, "{seated} seated");
        assert!(checked > 1000, "{checked}");
        let mut skeletons = SHELLED.to_vec();
        skeletons.sort();
        assert!(skeletons.contains(&skeleton), "the Skeleton race's own body among them");
        // and the Jal'Raeth's three (`HEAD_HELMS`), a player's in either art, which
        // name none: onto their node of the human helm node's name, 6
        let want: Vec<_> = [Era::Frontiers, Era::Y2002]
            .into_iter()
            .flat_map(|era| ids.iter().filter(move |&&id| HEAD_HELMS.contains(&id) || (era == Era::Y2002 && SHELLED.contains(&id))).map(move |&id| (era, id, true, -1, 6)))
            .collect();
        assert_eq!(moved, want);
    }

    /// A fit made anew comes out the same every time: its ties (a corner as
    /// near two segments, a triangle as heavy on two limbs) go by a fixed
    /// order, never a hash map's, which differs from map to map (the fairy's
    /// came out two ways, run to run).
    #[test]
    fn a_fit_is_the_same_every_time() {
        let Some(lib) = disc() else { return };
        let t = Tables::retail();
        let has = |id: u32| lib.surface(id as i32).is_some();
        for (id, look) in [(NINE[3], Look { armor: [4; 6], robe: -1, ..Default::default() }), (NINE[3], Look { armor: [3; 6], robe: 1, ..Default::default() }), (NINE[0], Look { armor: [2; 6], robe: -1, ..Default::default() })] {
            let id = id as i32;
            let r = lib.lookup(id).unwrap();
            let model = lib.build_model(r).unwrap();
            let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
            let changes = vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, &look, false, has, &t);
            let art = worn(&changes).map(|(t, _)| t);
            let robed = look.robe >= 0;
            let anew = || {
                REFS.with(|c| c.borrow_mut().clear());
                CARRIED.with(|c| c.borrow_mut().clear());
                FITS.with(|c| c.borrow_mut().clear());
                fit_of(&lib, id, &model, &cs, art, robed).expect("a fit")
            };
            let first = anew();
            for _ in 0..12 {
                assert_eq!(anew().tris, first.tris, "{id:#010x} in {look:?}");
            }
        }
    }

    trait ConcatSets {
        fn concat_sets(self) -> std::collections::HashSet<u32>;
    }

    impl<const N: usize> ConcatSets for [std::collections::HashSet<u32>; N] {
        fn concat_sets(self) -> std::collections::HashSet<u32> {
            self.into_iter().flatten().collect()
        }
    }

    /// The made-up slots dress five entries of their own, nothing else, and
    /// a slot without art adds no batch (the control: with art it does).
    #[test]
    fn made_up_slots() {
        let s = slots();
        assert_eq!(&s[..5], &[(ENTRY, 0), (ENTRY + 1, 0), (ENTRY + 2, 0), (ENTRY + 3, 0), (ENTRY + 4, 0)]);
        assert!(s[5..].iter().all(|&e| e == (-1, -1)));
        let mut m = esf::Model::default();
        for k in 0..3 {
            m.pos.push([k as f32, 0.0, 0.0]);
            m.uv.push([0.0; 2]);
            m.normal.push([0.0, 1.0, 0.0]);
            m.color.push([0.0, 0.0, 0.0, 1.0]);
            m.bones.push([0, 255, 255, 255]);
            m.weights.push([127, 0, 0, 0]);
        }
        let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
        let mut tris: [Vec<Tri>; SLOTS] = Default::default();
        tris[3].push(Tri { v: [0, 1, 2], uv: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], lod: None });
        let fit = Fit { tris, layers: std::array::from_fn(|_| layer.clone()) };
        let none = [LayerChange { entry: ENTRY + 3, layer: 0, texture: None, color: Some([1, 2, 3, 255]) }];
        assert!(!apply(&mut m, &fit, &none));
        assert!(m.batches.is_empty() && m.pos.len() == 3);
        let art = [LayerChange { entry: ENTRY + 3, layer: 0, texture: Some(0xABCD), color: Some([1, 2, 3, 255]) }];
        assert!(apply(&mut m, &fit, &art));
        assert_eq!(m.batches.len(), 1);
        let b = &m.batches[0];
        assert_eq!((b.material, b.layers[0].texture, b.layers[0].color, b.indices.clone()), (ENTRY + 3, 0xABCD, [1, 2, 3, 255], vec![3, 4, 5]));
        assert_eq!(m.uv[3..], [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(m.pos[4], m.pos[1]);
    }

    /// Where a ray from `o` along the unit `d` meets the triangles: the
    /// nearest and the farthest distance, and how many it meets.
    fn meets(tris: &[[Vec3; 3]], o: Vec3, d: Vec3) -> (f32, f32, usize) {
        let (mut near, mut far, mut n) = (f32::MAX, f32::MIN, 0);
        for t in tris {
            let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
            let p = d.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-12 {
                continue;
            }
            let s = o - t[0];
            let u = s.dot(p) / det;
            let q = s.cross(e1);
            let v = d.dot(q) / det;
            if !(0.0..=1.0).contains(&u) || v < 0.0 || u + v > 1.0 {
                continue;
            }
            let at = e2.dot(q) / det;
            if at > 1e-6 {
                near = near.min(at);
                far = far.max(at);
                n += 1;
            }
        }
        (near, far, n)
    }

    /// Body `id`'s sprite as the world loads it.
    fn sprite_of(lib: &Library, id: u32) -> CSprite<Ee> {
        let r = lib.lookup(id as i32).unwrap();
        vi::character::load_csprite(lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap()
    }

    /// The helms of armour sets 1 to 8 as a player's character in body `id`
    /// wears them (`helm_on`: on its node, `fit` seated as `helm_fit` says or
    /// as the node alone seats it), each set's triangles in its node's frame.
    fn helms_on(lib: &Library, id: u32, cs: &CSprite<Ee>, fit: bool) -> Vec<(u8, Vec<[Vec3; 3]>)> {
        let t = Tables::retail();
        (1..9u8)
            .filter_map(|set| {
                let (_, m) = helm_on(lib, id, cs, true, set, [255; 4], &t, fit)?;
                Some((set, m.batches.iter().flat_map(|b| b.indices.chunks_exact(3).map(|t| [0, 1, 2].map(|k| Vec3::from(m.pos[t[k] as usize])))).collect()))
            })
            .collect()
    }

    /// The skull of body `id` as its helm sees it: the vertices of its head
    /// (those weighted most to the node the body names for its head, 7, or
    /// one under it), at `at` ms of its action `action`, in the frame of the
    /// node a player's helm hangs on (`helm_node`).
    fn skull(lib: &Library, id: u32, action: i32, at: i64) -> Vec<Vec3> {
        let r = lib.lookup(id as i32).unwrap();
        let model = lib.build_model(r).unwrap();
        let mut anims = vi::hsprite::AnimScene::<Ee>::default();
        let mut rand = Rand(1);
        let mut cs = vi::character::load_csprite(lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut rand).unwrap();
        let node = helm_node(lib, id, &cs, true);
        assert!(node >= 0, "{id:#010x}: a helm node");
        let mut t = 0;
        while t <= at {
            anims.time = t;
            if t == 0 {
                cs.set_action(action, &anims, &mut rand);
            }
            cs.h.process(0, &anims);
            t += 16;
        }
        cs.h.set_blend_matrices();
        let mat = |m: &vi::geom::Matrix44<Ee>| glam::Mat4::from_cols_array_2d(&m.to_cols());
        let to_node = mat(&cs.h.nodes[node as usize].world).inverse();
        let head = cs.node_ids[7];
        let under = |mut n: i32| {
            while n >= 0 {
                if n == head {
                    return true;
                }
                n = cs.h.nodes[n as usize].parent;
            }
            false
        };
        (0..model.pos.len())
            .filter(|&v| {
                let (b, w) = (model.bones[v], model.weights[v]);
                let k = (0..4).max_by_key(|&k| w[k]).unwrap();
                b[k] != 255 && w[k] > 0 && under(b[k] as i32)
            })
            .map(|v| {
                let (b, w) = (model.bones[v], model.weights[v]);
                let (mut q, mut sum) = (Vec3::ZERO, 0.0);
                for k in (0..4).filter(|&k| b[k] != 255 && w[k] > 0) {
                    q += mat(&cs.h.blend[b[k] as usize]).transform_point3(Vec3::from(model.pos[v])) * w[k] as f32;
                    sum += w[k] as f32;
                }
                to_node.transform_point3(q / sum)
            })
            .collect()
    }

    /// The middle of a skull's bounds, where `worst_out`'s rays start.
    fn middle(skull: &[Vec3]) -> Vec3 {
        let lo = skull.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(*p));
        let hi = skull.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(*p));
        (lo + hi) * 0.5
    }

    /// How far the vertices `pts` of a skull whose middle is `mid` are out of
    /// the helms at worst, and in which helm and where: a vertex past every
    /// surface of a helm that a ray from the middle through it meets is out
    /// by how far past the farthest (positive); one short of every surface is
    /// inside by how far short of the nearest (negative); one between two of
    /// a helm's layers is in the helm's thickness, and one no ray meets a
    /// helm through (the face, below the rim) is where a helm covers nothing,
    /// as a human's face. The helms as given are seated by `fit` (the skull
    /// is taken back through it instead: the same thing).
    fn worst_out(helms: &[(u8, Vec<[Vec3; 3]>)], pts: &[Vec3], mid: Vec3, fit: Option<&HelmFit>) -> (f32, u8, Vec3) {
        let back = |p: Vec3| fit.map_or(p, |f| (p - f.offset) / f.scale);
        let mid = back(mid);
        let mut worst = (f32::MIN, 0, Vec3::ZERO);
        for (set, tris) in helms {
            for &p in pts {
                let q = back(p) - mid;
                let (d, at) = (q.normalize(), q.length());
                let (near, far, n) = meets(tris, mid, d);
                let along = fit.map_or(1.0, |f| (f.scale * d).length());
                let out = match n {
                    0 => continue,
                    _ if far < at => (at - far) * along,
                    _ if near > at => -(near - at) * along,
                    _ => continue,
                };
                if out > worst.0 {
                    worst = (out, *set, p);
                }
            }
        }
        worst
    }

    /// The report (the owner, 2026-11-09: "the skeleton helm has its head
    /// clipping out the back slightly"), measured on the eight skeletons a
    /// player may wear (`SHELLED`: the Skeleton race's own and the three its
    /// options offer, on the human's skeleton; the four great skeletons, on the
    /// troll's in Frontiers and the 2002 HumanMale's in the 2002 art), in both
    /// arts, each body built whole in the art it is drawn in, as the world
    /// builds it (`view_for`), against the eight helms as a player's character
    /// wears them: seated by `helm_fit`, every vertex of the skull is at least
    /// `SKULL_INSIDE` inside every helm. The skull rides the helm's node
    /// rigidly in every action (sampled: walking, running, dying, the battle
    /// stance, attacks, character select's flourish, casting and the wave), so
    /// one pose holds for all. The controls, seated as the node alone seats
    /// them (what this client drew until now): a body has a fit exactly where
    /// its skull came out of a helm so. In the 2002 art each of the four on the
    /// human's skeleton came out of the back of every helm by 4 mm or more (the
    /// 2002 skeletons name no helm node, and the 2002 HumanMale's they hang on
    /// is a human head's); in Frontiers' the skeleton's own stays inside every
    /// helm, on the node SOE stretched for it (FixScale 1.1 across and front to
    /// back), and has no fit; the rest came out at the cheeks: the three
    /// options, their skulls a tenth larger on the same node, the great
    /// skeletons in Frontiers on their own node (SOE's FixScale 1.4, 1.2, 1.2)
    /// and in the 2002 art on that human's. Needs both discs (`disc_2002`;
    /// without them it passes having checked nothing, and says so only under
    /// `--nocapture`).
    #[test]
    fn a_skeletons_skull_stays_inside_its_helm() {
        let (Some(front), Some(y2002)) = (disc(), disc_2002()) else { return };
        y2002.set_view(Era::Y2002);
        let skeleton = 0x6AF9_8A81u32;
        let great = [0x6AF9_8A82u32, 0x197F_51C7, 0x90F1_6A6F, 0x7D9F_41B2];
        assert!(great.iter().all(|g| SHELLED.contains(g)), "the great skeletons are a player's");
        let mut checked = 0;
        for (lib, art) in [(&front, Era::Frontiers), (&y2002, Era::Y2002)] {
            for &id in SHELLED {
                lib.with_view(lib.view_for(id as i32), || {
                    let era = lib.era_of(lib.lookup(id as i32).unwrap().file);
                    assert_eq!(era, art, "{id:#010x}: on both discs");
                    let cs = sprite_of(lib, id);
                    let head = skull(lib, id, 0, 1500);
                    assert!(head.len() > 150, "{era:?} {id:#010x}: {} vertices on the head", head.len());
                    for action in [1, 2, 14, 17, 22, 23, 37, 56] {
                        for at in [200, 900] {
                            let posed = skull(lib, id, action, at);
                            let moved = head.iter().zip(&posed).map(|(a, b)| a.distance(*b)).fold(0.0, f32::max);
                            assert!(moved < 1e-4, "{era:?} {id:#010x}: the skull moved {moved} on its helm's node in action {action}");
                        }
                    }
                    let mid = middle(&head);
                    let (out, set, at) = worst_out(&helms_on(lib, id, &cs, true), &head, mid, None);
                    assert!(out <= -SKULL_INSIDE, "{era:?} {id:#010x}: out by {out:.4} through helm {set} at {at:.3}");
                    // the controls: the helms on the node alone
                    let bare = helms_on(lib, id, &cs, false);
                    assert_eq!(bare.len(), 8);
                    let (out, _, at) = worst_out(&bare, &head, mid, None);
                    assert_eq!(helm_fit(lib, id, true).is_some(), out > -SKULL_INSIDE, "{era:?} {id:#010x}: a fit where the node alone lets the skull out ({out:.4} at {at:.3})");
                    let own_fix = |want: [f32; 3]| {
                        let fix = cs.h.nodes[cs.node_ids[8] as usize].fix_scale.to_array();
                        assert!(fix.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{id:#010x}: SOE's stretch on its own helm node: {fix:?}");
                    };
                    match era {
                        Era::Y2002 if !great.contains(&id) => {
                            for helm in &bare {
                                let (out, _, at) = worst_out(std::slice::from_ref(helm), &head, mid, None);
                                assert!(out > 0.004 && at.z < -0.1, "{id:#010x}: the back of the skull out of helm {} by {out:.4} at {at:.3}", helm.0);
                            }
                        }
                        Era::Frontiers if id == skeleton => {
                            own_fix([1.1, 1.0, 1.1]);
                            assert!(helm_fit(lib, id, true).is_none(), "nothing past it");
                        }
                        _ => {
                            if era == Era::Frontiers && great.contains(&id) {
                                own_fix([1.4, 1.2, 1.2]);
                            }
                            assert!(out > 0.0 && at.z > 0.03 && at.x.abs() > 0.04, "{era:?} {id:#010x}: its cheek out by {out:.4} at {at:.3}");
                        }
                    }
                    checked += 1;
                });
            }
        }
        assert_eq!(checked, 2 * SHELLED.len());
    }

    /// How far a fit moves the helms' vertices at most (`helms`, each set's
    /// triangles in its node's frame).
    fn moved_by(helms: &[(u8, Vec<[Vec3; 3]>)], fit: &HelmFit) -> f32 {
        helms.iter().flat_map(|(_, t)| t.iter().flatten()).map(|&p| fit.point(p).distance(p)).fold(0.0, f32::max)
    }

    /// Whether fit `a` is a better seat than `b` (`measure_helm_fits`'
    /// order): it moves the helms' vertices less, else stretches them less,
    /// else moves them less far front to back.
    fn better(helms: &[(u8, Vec<[Vec3; 3]>)], a: &HelmFit, b: &HelmFit) -> bool {
        let key = |f: &HelmFit| ((moved_by(helms, f) * 1e5).round() as i64, ((f.scale.x * f.scale.z) * 1e4).round() as i64, (f.offset.z.abs() * 1e5).round() as i64);
        key(a) < key(b)
    }

    /// The fits `measure_helm_fits` searches: a stretch across and front to
    /// back of 1.00 to 1.15 and 1.00 to 1.20 by hundredths, a move front to
    /// back of up to 10 mm either way by half millimetres.
    fn fits() -> impl Iterator<Item = HelmFit> {
        (0..=15).flat_map(|x| (0..=20).flat_map(move |z| (-20..=20).map(move |dz| HelmFit { scale: Vec3::new(1.0 + x as f32 * 0.01, 1.0, 1.0 + z as f32 * 0.01), offset: Vec3::new(0.0, 0.0, dz as f32 * 0.0005) })))
    }

    /// Each fit in `SEATS` is measured, not chosen by eye: it holds the skull
    /// inside every helm (`SKULL_INSIDE`), and none of its neighbours in the
    /// search (a hundredth less stretch either way, half a millimetre either
    /// way front to back) that `measure_helm_fits` would put before it does.
    /// The controls: every row is a skeleton a player may wear, each in an
    /// art at most once, and none moves a helm's vertices 2 cm. Needs both
    /// discs (`disc_2002`).
    #[test]
    fn each_helm_fit_is_the_least_that_holds() {
        let (Some(front), Some(y2002)) = (disc(), disc_2002()) else { return };
        y2002.set_view(Era::Y2002);
        let mut seen = std::collections::HashSet::new();
        for &(id, era, [x, z], dz) in SEATS {
            assert!(SHELLED.contains(&id) && seen.insert((id, era)), "{id:#010x} {era:?}");
            let lib = if era == Era::Y2002 { &y2002 } else { &front };
            let was = lib.view();
            lib.set_view(lib.view_for(id as i32));
            assert_eq!(lib.era_of(lib.lookup(id as i32).unwrap().file), era, "{id:#010x}: drawn in its row's art");
            let cs = sprite_of(lib, id);
            let bare = helms_on(lib, id, &cs, false);
            let head = skull(lib, id, 0, 1500);
            let fit = HelmFit { scale: Vec3::new(x, 1.0, z), offset: Vec3::new(0.0, 0.0, dz) };
            assert_eq!(helm_fit(lib, id, true), Some(fit));
            let mid = middle(&head);
            assert!(worst_out(&bare, &head, mid, Some(&fit)).0 <= -SKULL_INSIDE, "{id:#010x} {era:?}: holds");
            assert!(moved_by(&bare, &fit) < 0.02, "{id:#010x} {era:?}");
            let steps = [(-0.01, 0.0, 0.0), (0.01, 0.0, 0.0), (0.0, -0.01, 0.0), (0.0, 0.01, 0.0), (0.0, 0.0, -0.0005), (0.0, 0.0, 0.0005)];
            for (sx, sz, sdz) in steps {
                let n = HelmFit { scale: Vec3::new(x + sx, 1.0, z + sz), offset: Vec3::new(0.0, 0.0, dz + sdz) };
                if n.scale.x < 1.0 - 1e-4 || n.scale.z < 1.0 - 1e-4 || !better(&bare, &n, &fit) {
                    continue;
                }
                assert!(worst_out(&bare, &head, mid, Some(&n)).0 > -SKULL_INSIDE, "{id:#010x} {era:?}: {n:?} holds too, and is better");
            }
            lib.set_view(was);
        }
    }

    /// Only a player's skeleton's helm is seated past its node: over every
    /// CSprite on both discs in both arts, a player's or not, `helm_fit` is
    /// none but for `SHELLED`'s bodies as a player's, and on them as `SEATS`
    /// says (Frontiers' skeleton has none: its own node holds its skull). The
    /// helm `helm_on` builds for a player's human or playable race is the one
    /// built before it (the node `helm_node` names, the set's sprite, every
    /// layer in the tint) to the bit, in every set and both arts: those
    /// bodies' helms are drawn as they were. The control: a skeleton's fitted
    /// helm is not.
    #[test]
    fn only_a_players_skeleton_has_its_helm_fitted() {
        let (Some(front), Some(y2002)) = (disc(), disc_2002()) else { return };
        let t = Tables::retail();
        let tint = [12, 34, 56, 255];
        // the playable races' bodies (CHAR.ESF), and the human's by name
        let mut players: Vec<u32> = vec![0x0D53_7335, 0x61CA_3EDF, 0xC44A_D516, 0xACC0_0B4F, 0x8480_7490, 0xD8D5_6FF8, 0xA3DB_3FE2, 0xA99C_6B15];
        players.extend(["HumanMale", "HumanFemale"].map(vi::hash::hash_resource_id));
        let (mut checked, mut compared) = (0, 0);
        for (lib, era) in [(&front, Era::Frontiers), (&y2002, Era::Y2002)] {
            lib.set_view(era);
            let mut ids: Vec<u32> = lib.files.iter().flat_map(|(_, c)| c.dict.iter().filter(|&(_, &n)| c.node(n).ty == 0x2700).map(|(&id, _)| id as u32)).collect();
            ids.sort();
            ids.dedup();
            for &id in &ids {
                lib.with_view(lib.view_for(id as i32), || {
                    let Some(r) = lib.lookup(id as i32) else { return };
                    let body_era = lib.era_of(r.file);
                    for player in [false, true] {
                        let want = player && SHELLED.contains(&id) && SEATS.iter().any(|s| s.0 == id && s.1 == body_era);
                        assert_eq!(helm_fit(lib, id, player).is_some(), want, "{era:?} {id:#010x} player {player}");
                        checked += 1;
                    }
                    if !players.contains(&id) && !SHELLED.contains(&id) {
                        return;
                    }
                    let cs = sprite_of(lib, id);
                    for set in 1..9u8 {
                        let got = helm_on(lib, id, &cs, true, set, tint, &t, true);
                        // the helm as attached before `helm_on`
                        let node = helm_node(lib, id, &cs, true);
                        let old = (node >= 0).then(|| vi::cust::helm_with(set, &t)).flatten().and_then(|h| lib.lookup(h as i32)).and_then(|r| lib.build_model(r)).map(|mut m| {
                            m.batches.iter_mut().flat_map(|b| b.layers.iter_mut()).for_each(|l| l.color = tint);
                            (node, m)
                        });
                        let fitted = helm_fit(lib, id, true).is_some();
                        match (got, old) {
                            (Some((gn, gm)), Some((on, om))) => {
                                assert_eq!(gn, on, "{era:?} {id:#010x} set {set}: the node");
                                assert_eq!(same(&gm, &om), !fitted, "{era:?} {id:#010x} set {set}: fitted {fitted}");
                                compared += 1;
                            }
                            (None, None) => {}
                            (g, o) => panic!("{era:?} {id:#010x} set {set}: a helm {} before, {} now", o.is_some(), g.is_some()),
                        }
                    }
                });
            }
        }
        assert!(checked > 2000, "{checked}");
        // the ten player bodies and the four skeletons in both arts, eight helms each
        assert!(compared >= 2 * 14 * 8 - 16, "{compared}");
    }

    /// The search `SEATS` is the result of (run with `--ignored --nocapture`;
    /// needs both discs): for each skeleton a player may wear in each art (the
    /// body, its skull and its helms built in the art it is drawn in),
    /// among `fits()` that hold its skull at least `SKULL_INSIDE` inside each
    /// of the eight helms on its node, the best by `better`; none for a
    /// skeleton whose node alone holds it. Prints the table's rows.
    #[test]
    #[ignore]
    fn measure_helm_fits() {
        let (Some(front), Some(y2002)) = (disc(), disc_2002()) else { return };
        y2002.set_view(Era::Y2002);
        for (lib, art) in [(&front, Era::Frontiers), (&y2002, Era::Y2002)] {
            for &id in SHELLED {
                // the body, its skull and its helms in the art it is drawn in
                lib.with_view(lib.view_for(id as i32), || {
                let era = lib.era_of(lib.lookup(id as i32).unwrap().file);
                let cs = sprite_of(lib, id);
                let bare = helms_on(lib, id, &cs, false);
                let head = skull(lib, id, 0, 1500);
                let mid = middle(&head);
                let node = helm_node(lib, id, &cs, true);
                let fix = cs.h.nodes[node as usize].fix_scale.to_array();
                eprintln!("    // {art:?} art {id:#010x}, drawn in {era:?}'s: {} nodes, helm node {node} (its own word {}), FixScale {fix:.2?}; per helm {:?}", cs.h.nodes.len(), cs.node_ids[8], bare.iter().map(|h| { let (o, _, at) = worst_out(std::slice::from_ref(h), &head, mid, None); format!("{}: {:+.1} mm at {at:.3}", h.0, o * 1000.0) }).collect::<Vec<_>>());
                let (out, set, at) = worst_out(&bare, &head, mid, None);
                if out <= -SKULL_INSIDE {
                    eprintln!("    // {era:?} {id:#010x}: inside every helm on its node alone ({:.1} mm at the closest)", -out * 1000.0);
                    return;
                }
                // the vertices that can come out: within 2.5 cm of a helm, the
                // farthest out first (a fit fails at the first)
                let mut near: Vec<(f32, Vec3)> = head.iter().map(|&p| (worst_out(&bare, &[p], mid, None).0, p)).filter(|(o, _)| *o > -0.025).collect();
                near.sort_by(|a, b| b.0.total_cmp(&a.0));
                let near: Vec<Vec3> = near.into_iter().map(|(_, p)| p).collect();
                let holds = |f: &HelmFit| {
                    // each vertex alone, the farthest out first
                    near.iter().all(|&p| worst_out(&bare, &[p], mid, Some(f)).0 <= -SKULL_INSIDE)
                };
                let mut best: Option<HelmFit> = None;
                for f in fits() {
                    if best.as_ref().is_some_and(|b| !better(&bare, &f, b)) || !holds(&f) {
                        continue;
                    }
                    best = Some(f);
                }
                let b = best.expect("a fit in the search");
                eprintln!("    // {era:?} {id:#010x}: out by {:.1} mm through helm {set} at {at:.3} on its node alone; {:.1} mm inside fitted, the helms moved {:.1} mm at most", out * 1000.0, -worst_out(&bare, &head, mid, Some(&b)).0 * 1000.0, moved_by(&bare, &b) * 1000.0);
                eprintln!("    ({id:#010X}, Era::{era:?}, [{:.2}, {:.2}], {:.4}),", b.scale.x, b.scale.z, b.offset.z);
                });
            }
        }
    }

    /// Every body go-eqoa-server's races may wear (its race's own and every one its
    /// options offer, `entity/raceoptions.go`, with the hill giant's armour bodies),
    /// each a label for the survey.
    const RACE_BODIES: &[(u32, &str)] = &[
        (0x141BFE0C, "froglok"), (0x429A04AB, "froglok robed"), (0x063C5D97, "froglok spiked"), (0x0F239012, "ghoul"), (0x6F17325E, "ghoul robed"), (0x1663EC1C, "ghoul spiked"),
        (0xFFB1EA2C, "hill giant"), (0x8117C627, "ice giant"), (0xE617303B, "sand giant"), (0x72A8FDBB, "fire giant"), (0x7F2604DF, "fire giant vest"), (0x05F59B65, "cyclops"), (0x2D686972, "fire giant reaver"), (0xBF6ADFDB, "fire giant king"),
        (0xF34501CB, "hill giant F"), (0x286C6E9E, "ice giant F"), (0x8E2A6552, "sand giant F"), (0xE30827CC, "cyclops F"),
        (0x22176258, "kerran tiger"), (0x4D9C53EF, "kerran tiger robed"), (0x77A7E7B8, "kerran white tiger"), (0x78FA8F20, "kerran lion"), (0x0DDD4087, "kerran lion clothed"), (0x5D40C8AB, "kerran maned lion"),
        (0xB749BF57, "kerran tiger F"), (0xB62092E4, "kerran robed F"), (0xE700DE53, "kerran red tiger F"), (0x39FEAA5F, "kerran lioness F"), (0x7654AC3C, "kerran lioness robed F"), (0xD8E0F980, "kerran lioness warrior F"),
        (0x6C1308E0, "goblin grey"), (0x581B6A80, "goblin grey robed"), (0x96371804, "goblin green"), (0xBCDF0C04, "goblin green robed"), (0xE96179D4, "goblin ice"), (0xA5F92F43, "goblin ice robed"), (0x6C7C07ED, "goblin fire"), (0xBE927B85, "goblin fire robed"), (0xCEF5874A, "goblin fire shaman"),
        (0xEDB4E0AD, "goblin gladiator"), (0x687E7645, "goblin purple robe"), (0x2ECBB95A, "goblin overseer"), (0xB656DFEA, "goblin skull mask"), (0xEC0885EB, "goblin bandit"), (0x1F8E11D3, "goblin high priest"), (0x3AB96672, "goblin warlord"), (0xF54C2142, "goblin warmaster"),
        (0x1245E56B, "goblin soothsayer"), (0x27A4E554, "goblin wizard"), (0x5937EA22, "goblin mystic"), (0x507C4A52, "goblin chieftain"), (0xEE1DDFBA, "goblin poacher"), (0x1CCDF0EF, "goblin inquisitor"), (0x2F34B867, "goblin king"),
        (0x3151DF13, "aviak hawk"), (0x8EFD12EA, "aviak hawk F"), (0x262C572A, "aviak hawk robed"), (0xDC641055, "aviak raven"), (0x41EFF500, "aviak raven robed"), (0x3C3DE6CA, "aviak vulture"), (0xC04CEC07, "aviak vulture robed"), (0xB1EEB204, "aviak owl"), (0x8B332785, "aviak owl robed"),
        (0x246B5FED, "aviak raven gown"), (0x5585143C, "aviak raven black"), (0xF83BD167, "aviak hawk green robe"),
        (0x911220E2, "lizardman strapped"), (0xD45B9F1F, "lizardman pauldrons"), (0x2C1D8724, "lizardman bare"), (0x7468EAE7, "lizardman robed"), (0x16BB9E24, "lizardman hierophant"), (0xEEBF6DA5, "lizardman grey"),
        (0x31A15F17, "kappa brown"), (0xC3EC4F58, "kappa marked"), (0xF6747024, "kappa banded"), (0xF0FAB933, "kappa straw hat"),
        (0x4376EB52, "brownie"), (0x49468331, "brownie elder"), (0x94BA77CD, "pixie"), (0x7A85646C, "fairy"),
        (0xCF415561, "gnoll black"), (0x223FD6BC, "gnoll captain"), (0xF5F3A450, "gnoll blue-pawed"), (0x097BFC70, "gnoll black robed"), (0xB7B21DFC, "gnoll elder"), (0x4A9BAB9E, "gnoll black and tan"), (0x17DA0FEB, "gnoll scout"), (0x7492678E, "gnoll bt robed"),
        (0x585E5ED9, "gnoll teal robe"), (0x16BA0BD3, "gnoll green robe"), (0x3C6D3677, "gnoll packlord"), (0xC2D7DD2F, "gnoll bt armour"), (0x71F163CA, "gnoll grey"), (0xC3B94FA5, "gnoll grey armour"), (0x147AC3EE, "gnoll red scarf"), (0x09E0AE7D, "gnoll grey robed"),
        (0xB15628DA, "gnoll grey gown"), (0xCE06735E, "gnoll grey red robe"), (0x2EDA50DC, "gnoll reanimated"), (0x16E320F7, "gnoll reanimated robed"),
        (0x8214A32D, "orc"), (0x84F47767, "orc long hair"), (0x656DD4C5, "orc grey mail"), (0x37FD13F9, "orc deathfist"), (0x3198F5B1, "orc deathfist kilt"), (0xB2FBD7EF, "orc hatebone elder"), (0xB086ADA7, "orc fire weaver"),
        (0x0A03EB92, "orc F"), (0xDA515962, "orc grey mail F"), (0x0205D208, "orc healer F"), (0xC76F1FA8, "orc slayer F"), (0xE144942F, "orc chieftain F"),
        (0xCFC478EE, "kobold"), (0x6B9B7016, "kobold augur"), (0x597325B5, "kobold patched"), (0x9CDF3C2D, "kobold pauldrons"),
        (0xC5CBF4CF, "minotaur black"), (0x0AE71D02, "minotaur brown"),
        (0xBADC25C9, "ratman"), (0x0ECC836A, "ratman hooded"), (0x133A79CA, "ratman hooded pauldrons"), (0x46BA26A9, "ratman trickster"),
        (0xBE10226F, "satyr"), (0x097ADB18, "faun F"),
        (0x70EFCE05, "dryad"), (0x21F311BB, "erych guardian"), (0xACE71E26, "haliai"), (0xB90ADDD2, "erych arbiter"),
        (0x6AF98A81, "skeleton"), (0x45A8B064, "skeleton old"), (0x198BAA64, "skeleton mossy"), (0x648DFC11, "skeleton frost"), (0x6AF98A82, "great bone"), (0x197F51C7, "great old bone"), (0x90F16A6F, "great mossy"), (0x7D9F41B2, "great frost"),
        (0x4A4D88FB, "fungus blue"), (0x5874ACFC, "fungus red"), (0xE8B0ADC8, "fungus pale"),
        (0xB4D2D132, "gargoyle granite"), (0x4E2A945C, "gargoyle moss"), (0xAEC5D9F6, "gargoyle ice"), (0x001355C8, "gargoyle obsidian"), (0x6D959FD9, "gargoyle shadow"), (0x9304BAB5, "gargoyle fire"),
        (0xF51BAB09, "jalraeth robes"), (0x4B24FF7E, "jalraeth channeler"), (0x491F6347, "jalraeth knight"),
    ];

    /// What each body a server's race may wear draws of a full set, as a player's
    /// character (run with `--ignored --nocapture`): its text slots, the made-up
    /// batches in plate and in a robe, the helm's node and the items' nodes.
    #[test]
    #[ignore]
    fn survey_race_bodies() {
        let Some(lib) = disc() else { return };
        let t = Tables::retail();
        let hm = crate::raceemote::Rig::load(&lib, vi::hash::hash_resource_id("HumanMale")).unwrap();
        let human_refs: std::collections::HashSet<i32> = hm.refs.iter().flatten().copied().collect();
        let races = net::creation::Creation { all_classes: true, races: RACE_BODIES.iter().map(|&(b, _)| net::creation::Race { id: 11, like: 0, city: 0, bonus: [0; 7], class_mask: 0, body: [b, b], name: String::new(), short: String::new(), desc: String::new() }).collect() };
        for &(id, label) in RACE_BODIES {
            let Some(r) = lib.lookup(id as i32) else {
                eprintln!("{id:08X} {label}: NOT ON THE DISC");
                continue;
            };
            let model = lib.build_model(r).unwrap();
            let mut cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
            let hdr = cs.holds_items;
            crate::racebody::grant(&mut cs, id, true, Some(&races));
            let tris = |m: &esf::Model, e: i32, l: i32| m.batches.iter().filter(|b| b.material == e && (l as usize) < b.layers.len()).map(|b| b.indices.len() / 3).sum::<usize>();
            let own: Vec<String> = (0..5).map(|i| { let (e, l) = cs.text_slots[i]; if e == -1 { "-".into() } else { format!("{e}/{l}:{}", tris(&model, e, l)) } }).collect();
            let made = |look: &Look| {
                let mut m = model.clone();
                crate::play::dress_look(&lib, id as i32, &mut m, &cs, look, false, &t, true);
                (0..5).map(|i| tris(&m, ENTRY + i as i32, 0)).collect::<Vec<_>>()
            };
            let plate = made(&Look { armor: [4; 6], robe: -1, ..Default::default() });
            let robe = made(&Look { armor: [4; 6], robe: 1, ..Default::default() });
            let helm = helm_on(&lib, id, &cs, true, 4, [255; 4], &t, true).map(|(n, _)| n);
            let rig = Rig::of_sprite(&cs).is_some();
            let br = crate::raceemote::Rig::load(&lib, id).unwrap();
            let shared = br.refs.iter().flatten().filter(|r| human_refs.contains(r)).count();
            eprintln!(
                "{id:08X} {label:28} nodes {:2} shared {:2} sex {} hdr {} | own {:?} | slotless {} rig {} plate {:?} robe {:?} | n7 {} n8 {} helm {:?} named {:?} | items {:?} | head {}",
                cs.h.nodes.len(), shared, cs.sex, hdr as u8, own, is_slotless(&cs) as u8, rig as u8, plate, robe, cs.node_ids[7], cs.node_ids[8], helm,
                named_helm_node(&lib, id, cs.sex != 0),
                [cs.item_node(0), cs.item_node(1), cs.item_node(2)],
                head_info(&model, &cs)
            );
        }
        for name in ["HumanMale", "HumanFemale"] {
            let id = vi::hash::hash_resource_id(name);
            let r = lib.lookup(id as i32).unwrap();
            let model = lib.build_model(r).unwrap();
            let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
            let h = cs.node_ids[8] as usize;
            let w = glam::Mat4::from_cols_array_2d(&cs.h.nodes[h].inv_bind_world.to_cols()).inverse();
            let helm = helm_on(&lib, id, &cs, true, 4, [255; 4], &t, true).unwrap().1;
            let (lo, hi) = helm.pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| { let q = w.transform_point3(Vec3::from(*p)); (lo.min(q), hi.max(q)) });
            eprintln!("{name}: n7 {} n8 {} fix {:?} | head {} | helm 4 in the model {lo:.3} .. {hi:.3}", cs.node_ids[7], cs.node_ids[8], cs.h.nodes[h].fix_scale.to_array(), head_info(&model, &cs));
        }
    }

    /// What a player's character in body `id` draws of a full set (plate, helm 4,
    /// a weapon in each hand and a shield; and robe 1 over the plate), in the art
    /// `lib` draws it in now: per worn slot whether plate is drawn there (by its
    /// own slot or a made-up batch), whether the robe's chest and legs are, whether
    /// a helm hangs on it, and which item slots hold their item.
    struct Drawn {
        armour: [bool; SLOTS],
        robe: [bool; 2],
        helm: bool,
        items: [bool; 3],
    }

    fn drawn(lib: &Library, id: u32, races: &net::creation::Creation) -> Option<Drawn> {
        let t = Tables::retail();
        let has = |id: u32| lib.surface(id as i32).is_some();
        let r = lib.lookup(id as i32)?;
        let model = lib.build_model(r)?;
        let mut cs = vi::character::load_csprite(lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1))?;
        crate::racebody::grant(&mut cs, id, true, Some(races));
        let lacks = lacking(&model, &cs);
        let helm = helm_on(lib, id, &cs, true, 4, [255; 4], &t, true).is_some();
        let made = |m: &esf::Model, i: usize| m.batches.iter().filter(|b| b.material == ENTRY + i as i32 && !b.indices.is_empty()).map(|b| b.layers[0].texture).collect::<Vec<i32>>();
        let mut plate = model.clone();
        crate::play::dress_look(lib, id as i32, &mut plate, &cs, &Look { armor: [4; 6], robe: -1, ..Default::default() }, helm, &t, true);
        let armour = std::array::from_fn(|i| !lacks[i] || !made(&plate, i).is_empty());
        let robe_look = Look { armor: [4; 6], robe: 1, ..Default::default() };
        let art = worn(&vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, &robe_look, helm, has, &t)).map(|(t, _)| t);
        let mut robed = model.clone();
        crate::play::dress_look(lib, id as i32, &mut robed, &cs, &robe_look, helm, &t, true);
        let robe = [0, 3].map(|i| {
            let Some(want) = art[i] else { return false };
            if lacks[i] {
                return made(&robed, i).contains(&(want as i32));
            }
            let (entry, layer) = cs.text_slots[i];
            robed.batches.iter().any(|b| b.material == entry && b.layers.get(layer as usize).is_some_and(|l| l.texture == want as i32))
        });
        Some(Drawn { armour, robe, helm, items: [0, 1, 2].map(|k| cs.item_node(k).is_some()) })
    }

    /// The pieces no limb of a body carries, by body: the Jal'Raeth's legs and
    /// boots (they float with their legs crossed under their robes: chest,
    /// bracers and gloves alone, `NAMED_SHELLS`); the robed kerrans' bracers
    /// (0x4D9C53EF and 0xB62092E4 name no bracer slot, and their forearms are
    /// their robe's sleeves, their chest slot's entry, which the chest's art
    /// covers as on a PS2); and the fungusmen's helm (`HEADLESS`).
    fn bare(id: u32) -> (Vec<usize>, bool) {
        let slots = match id {
            _ if NAMED_SHELLS.contains(&id) => vec![3, 4],
            0x4D9C_53EF | 0xB620_92E4 => vec![1],
            _ => Vec::new(),
        };
        (slots, HEADLESS.contains(&id))
    }

    /// Every body a server's races may wear (`RACE_BODIES`: each race's own and
    /// every one its options offer), as a player's character, draws a full set:
    /// plate on all five worn slots (its own slots or made-up batches), a robe's
    /// chest and legs over it, a helm, a weapon in each hand and a shield, in
    /// Frontiers' art and in the 2002 disc's (`disc_2002`, when there; F9 draws a
    /// body the 2002 disc has from it); but what no limb of it carries (`bare`).
    /// Before it, 79 of these 148 bodies (every creature's with slots of its
    /// own) drew no plate on a worn slot or more (gloves and boots most; the
    /// hooded ratman's bracers alone), and 136 hung no helm.
    /// The controls: an NPC in the same body hangs no helm where the body names
    /// no helm node of its own, and with the dressing off a body with some slots
    /// draws only those.
    #[test]
    fn every_race_body_wears_a_full_set() {
        let Some(front) = disc() else { return };
        let y2002 = disc_2002();
        let races = net::creation::Creation { all_classes: true, races: RACE_BODIES.iter().map(|&(b, _)| net::creation::Race { id: 11, like: 0, city: 0, bonus: [0; 7], class_mask: 0, body: [b, b], name: String::new(), short: String::new(), desc: String::new() }).collect() };
        let mut missing = Vec::new();
        let mut checked = 0;
        for (lib, art) in [Some((&front, Era::Frontiers)), y2002.as_ref().map(|l| (l, Era::Y2002))].into_iter().flatten() {
            lib.set_view(art);
            for &(id, label) in RACE_BODIES {
                lib.with_view(lib.view_for(id as i32), || {
                    let era = lib.lookup(id as i32).map(|r| lib.era_of(r.file));
                    let Some(d) = drawn(lib, id, &races) else {
                        missing.push(format!("{art:?} {id:08X} {label}: not built"));
                        return;
                    };
                    let (legs, headless) = bare(id);
                    for i in 0..SLOTS {
                        if d.armour[i] == legs.contains(&i) {
                            missing.push(format!("{art:?} ({era:?}) {id:08X} {label}: slot {i} {}", if d.armour[i] { "drawn, bare expected" } else { "bare" }));
                        }
                    }
                    for (k, i) in [0usize, 3].into_iter().enumerate() {
                        if d.robe[k] == legs.contains(&i) {
                            missing.push(format!("{art:?} ({era:?}) {id:08X} {label}: the robe's slot {i} {}", if d.robe[k] { "drawn, bare expected" } else { "bare" }));
                        }
                    }
                    if d.helm == headless {
                        missing.push(format!("{art:?} ({era:?}) {id:08X} {label}: helm {}", d.helm));
                    }
                    if d.items != [true; 3] {
                        missing.push(format!("{art:?} ({era:?}) {id:08X} {label}: items {:?}", d.items));
                    }
                    checked += 1;
                    // the control: an NPC's helm where the body names none
                    let cs = sprite_of(lib, id);
                    if cs.node_ids[8] < 0 {
                        assert!(helm_on(lib, id, &cs, false, 4, [255; 4], &Tables::retail(), true).is_none(), "{art:?} {id:08X}: an NPC's helm");
                    }
                });
            }
        }
        assert!(missing.is_empty(), "{} of {checked}:\n{}", missing.len(), missing.join("\n"));
        assert!(checked >= RACE_BODIES.len(), "{checked}");
        // the control: with the dressing off, the froglok draws its own three slots
        // alone (no gloves, no boots) and no helm
        set_enabled(false);
        front.set_view(Era::Frontiers);
        let d = drawn(&front, 0x141B_FE0C, &races).unwrap();
        set_enabled(true);
        assert_eq!((d.armour, d.helm), ([true, true, false, true, false], false));
    }

    /// The share of body `id`'s head (the vertices skinned most to its head node
    /// or one under it, on its nearest level) inside a player's seated helm's
    /// bounds, in the bind pose; or with `node` and `fit` given, the helm hung so
    /// instead. None without a helm.
    fn head_inside(lib: &Library, id: u32, alt: Option<(i32, Option<HelmFit>)>) -> Option<f32> {
        let r = lib.lookup(id as i32)?;
        let model = lib.build_model(r)?;
        let cs = sprite_of(lib, id);
        let t = Tables::retail();
        let (node, m) = match alt {
            None => helm_on(lib, id, &cs, true, 4, [255; 4], &t, true)?,
            Some((node, fit)) => {
                let mut m = vi::cust::helm_with(4, &t).and_then(|h| lib.lookup(h as i32)).and_then(|r| lib.build_model(r))?;
                if let Some(f) = fit {
                    f.apply(&mut m);
                }
                (node, m)
            }
        };
        let w = glam::Mat4::from_cols_array_2d(&cs.h.nodes[node as usize].inv_bind_world.to_cols()).inverse();
        let (lo, hi) = m.pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
            let q = w.transform_point3(Vec3::from(*p));
            (lo.min(q), hi.max(q))
        });
        let head = cs.node_ids[7];
        let under = |mut n: i32| {
            while n >= 0 && (n as usize) < cs.h.nodes.len() {
                if n == head {
                    return true;
                }
                n = cs.h.nodes[n as usize].parent;
            }
            false
        };
        let pts: Vec<Vec3> = near_triangles(&model).flat_map(|(_, t)| t).filter(|&v| bone_of(&model, v).is_some_and(|b| under(b as i32))).map(|v| pos(&model, v)).collect();
        (!pts.is_empty()).then(|| pts.iter().filter(|p| p.cmpge(lo).all() && p.cmple(hi).all()).count() as f32 / pts.len() as f32)
    }

    /// A seated helm sits on its wearer's head (`HEAD_SEATS`): on every family's
    /// bodies in Frontiers' art at least a quarter of the head lies inside the
    /// helm's bounds as it hangs in the bind pose (the rest a snout, a beak, a
    /// muzzle, ears, horns, a hood or a hat, out through the face and the top;
    /// the least, a quarter, a hooded ratman's). The
    /// controls: hung on the node of the human's helm node's name with no seat,
    /// as the human's helm hangs, the lizardmen's, gnolls', minotaurs' and
    /// brownies' helm sits at their jaws and necks (that node is low on their
    /// heads), with half as much of the head inside it as seated or less; and
    /// every seat's node is the head's or one under it, for a player only.
    #[test]
    fn a_seated_helm_sits_on_the_head() {
        let Some(lib) = disc() else { return };
        let mut worst = (1.0f32, 0u32);
        for seat in HEAD_SEATS {
            for &id in seat.bodies {
                let cs = sprite_of(&lib, id);
                let node = seat_node(&lib, id, &cs, seat).unwrap_or_else(|| panic!("{id:08X}: no node for its seat"));
                let mut n = node;
                while n >= 0 && n != cs.node_ids[7] {
                    n = cs.h.nodes[n as usize].parent;
                }
                assert_eq!(n, cs.node_ids[7], "{id:08X}: the seat's node is its head's or under it");
                assert_eq!(helm_node(&lib, id, &cs, true), node, "{id:08X}: a player's");
                assert_eq!(helm_node(&lib, id, &cs, false), -1, "{id:08X}: an NPC's none");
                assert!(head_fit(id, &cs, true).is_some() && head_fit(id, &cs, false).is_none(), "{id:08X}");
                let inside = head_inside(&lib, id, None).unwrap();
                eprintln!("{id:08X}: {:.0}% of its head inside its helm", inside * 100.0);
                if inside < worst.0 {
                    worst = (inside, id);
                }
            }
        }
        assert!(worst.0 >= 0.25, "{:08X}: {:.0}% of its head inside its helm", worst.1, worst.0 * 100.0);
        // the controls: on the named node, with no seat, half as much or less
        for id in [0x9112_20E2u32, 0xCF41_5561, 0xC5CB_F4CF, 0x4376_EB52] {
            let cs = sprite_of(&lib, id);
            let named = named_helm_node(&lib, id, cs.sex != 0).unwrap();
            let (alone, seated) = (head_inside(&lib, id, Some((named, None))).unwrap(), head_inside(&lib, id, None).unwrap());
            eprintln!("{id:08X}: {:.0}% inside hung on its named node alone, {:.0}% seated", alone * 100.0, seated * 100.0);
            assert!(alone <= seated * 0.5, "{id:08X}: {:.0}% inside, hung on its named node alone, against {:.0}% seated", alone * 100.0, seated * 100.0);
        }
    }

    /// A NodeIDList that names one foot for both footstep slots (the ratman
    /// trickster's, 0x46BA26A9: its left foot twice) is read with the other
    /// foot the node mirrored across the root; one with no node there finds no
    /// legs (the control), and the same skeleton naming both feet reads alike.
    #[test]
    fn both_footsteps_on_one_foot() {
        let (parent, at, ids) = skeleton();
        let both = Rig::of(&parent, &at, &ids).expect("a rig");
        let mut one = ids;
        one[6] = one[5];
        let rig = Rig::of(&parent, &at, &one).expect("the other foot found");
        assert_eq!(rig.part, both.part);
        let mut moved = at.clone();
        moved[ids[6] as usize].x += 0.05;
        assert!(Rig::of(&parent, &moved, &one).is_none(), "the control: no node mirrors it");
        // on the disc: the trickster is fitted, and gains gloves and boots
        let Some(lib) = disc() else { return };
        let cs = sprite_of(&lib, 0x46BA_26A9);
        assert_eq!(cs.node_ids[5], cs.node_ids[6], "it names one foot twice");
        assert!(Rig::of_sprite(&cs).is_some());
    }

    /// A body's head as the survey prints it: the head node's place in the bind
    /// pose, the vertices on it or under it and their bounds (model space).
    fn head_info(model: &esf::Model, cs: &CSprite<Ee>) -> String {
        let head = cs.node_ids[7];
        if head < 0 {
            return "none".into();
        }
        let under = |mut n: i32| {
            while n >= 0 && (n as usize) < cs.h.nodes.len() {
                if n == head {
                    return true;
                }
                n = cs.h.nodes[n as usize].parent;
            }
            false
        };
        let at = glam::Mat4::from_cols_array_2d(&cs.h.nodes[head as usize].inv_bind_world.to_cols()).inverse().transform_point3(Vec3::ZERO);
        let pts: Vec<Vec3> = (0..model.pos.len()).filter(|&v| bone_of(model, v).is_some_and(|b| under(b as i32))).map(|v| Vec3::from(model.pos[v])).collect();
        let (lo, hi) = pts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let top = cs.h.nodes.iter().map(|n| glam::Mat4::from_cols_array_2d(&n.inv_bind_world.to_cols()).inverse().transform_point3(Vec3::ZERO).y).fold(f32::MIN, f32::max);
        format!("node at {at:.3}, {} verts {lo:.3} .. {hi:.3}, skeleton top {top:.3}", pts.len())
    }
}
