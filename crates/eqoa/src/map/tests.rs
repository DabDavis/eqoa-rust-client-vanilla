//! The map's keys, pointer and view, without a GPU: a map over Tunaria's
//! layout (a zone a 2,000 square, x 2,000..28,000, z 0..34,000).

use super::*;

fn tunaria() -> Vec<[f32; 4]> {
    (0..17).flat_map(|r| (1..14).map(move |c| [c as f32 * CELL, r as f32 * CELL, (c + 1) as f32 * CELL, (r + 1) as f32 * CELL])).collect()
}

/// A map up over Tunaria in a `size` window, its view made on the player at
/// Freeport (25,300, 15,200).
fn up(size: (u32, u32)) -> Map {
    let mut m = Map {
        world: Some(World { name: "TUNARIA.ESF".into(), era: esf::Era::Frontiers, index: 0, builder: Builder::new(WORLD_FILE, &tunaria(), None, None), overview: None, tiles: BTreeMap::new(), made: Instant::now(), view: None }),
        size,
        ..Default::default()
    };
    m.show();
    m.steer(size, 0.0, [0.0; 4], Some(FREEPORT));
    m
}

const FREEPORT: [f32; 2] = [25300.0, 15200.0];

/// Opened on the player at the opening zoom; △ (and Start) close it; a key
/// while it is down is not the map's. The control: a map that is not up
/// takes nothing.
#[test]
fn it_opens_on_the_player_and_triangle_closes_it() {
    let size = (1917, 973);
    let mut m = up(size);
    assert_eq!(m.view.unwrap().center, FREEPORT);
    assert_eq!(m.view.unwrap().scale, view::opening_scale(size));
    assert!(m.key(key::TRIANGLE, true));
    assert!(!m.open);
    assert!(!m.key(key::CROSS, true), "the control: not up, not the map's");
    m.show();
    m.key(key::START, true);
    assert!(!m.open);
}

/// R1 zooms in a step and L1 out, from the middle; the steps stop at the
/// limits: in to a world unit a pixel, out to the whole world. The
/// control: a step's factor.
#[test]
fn the_shoulders_zoom_a_step_within_the_limits() {
    let size = (1280, 720);
    let mut m = up(size);
    let s0 = m.view.unwrap().scale;
    m.key(key::R1, true);
    m.steer(size, 0.0, [0.0; 4], None);
    assert!((m.view.unwrap().scale - s0 / ZOOM_STEP).abs() < 1e-4);
    m.key(key::L1, true);
    m.key(key::L1, true);
    m.steer(size, 0.0, [0.0; 4], None);
    assert!((m.view.unwrap().scale - s0 * ZOOM_STEP).abs() < 1e-3);
    assert_ne!(m.view.unwrap().scale, s0, "the control");
    let (lo, hi) = view::limits(m.world.as_ref().unwrap().builder.cells.extent(), size);
    for _ in 0..60 {
        m.key(key::R1, true);
    }
    m.steer(size, 0.0, [0.0; 4], None);
    assert_eq!(m.view.unwrap().scale, lo);
    for _ in 0..60 {
        m.key(key::L1, true);
    }
    m.steer(size, 0.0, [0.0; 4], None);
    assert_eq!(m.view.unwrap().scale, hi);
}

/// A held d-pad arm pans by the time it is held, the same in a second at 30
/// frames and at 165 (the clock's, not the frames'); it stops following the
/// player; ✕ follows again. The control: a pan by the frame, not the time,
/// goes 5.5 times as far at 165.
#[test]
fn a_held_pan_goes_by_the_clock() {
    let size = (1564, 857);
    let held_for_a_second = |hz: u32| {
        let mut m = up(size);
        m.key(key::RIGHT, true);
        for _ in 0..hz {
            m.steer(size, 1.0 / hz as f32, [0.0; 4], Some(FREEPORT));
        }
        m.key(key::RIGHT, false);
        m.steer(size, 1.0 / hz as f32, [0.0; 4], Some(FREEPORT));
        (m.view.unwrap().center[0] - FREEPORT[0], m.follow)
    };
    let (at30, follow) = held_for_a_second(30);
    let (at165, _) = held_for_a_second(165);
    let expect = PAN_RATE * size.1 as f32 * view::opening_scale(size);
    assert!((at30 - expect).abs() / expect < 1e-3, "{at30} vs {expect}");
    assert!((at165 - at30).abs() / at30 < 1e-3, "{at165} vs {at30}");
    assert!(!follow, "panned: no longer following");
    let by_frame = |hz: u32| hz as f32 * PAN_RATE * size.1 as f32 / 30.0;
    assert!(by_frame(165) / by_frame(30) > 5.0, "the control");
    let mut m = up(size);
    m.key(key::DOWN, true);
    m.steer(size, 0.5, [0.0; 4], Some(FREEPORT));
    assert!(m.view.unwrap().center[1] > FREEPORT[1], "down is south, +z");
    m.key(key::DOWN, false);
    m.key(key::CROSS, true);
    m.steer(size, 0.016, [0.0; 4], Some([20000.0, 9000.0]));
    assert_eq!(m.view.unwrap().center, [20000.0, 9000.0], "✕: back to the player, following");
}

/// The left stick pans (pushed up, north), the right stick zooms (pushed
/// up, in), each by the time. The control: the right stick does not pan.
#[test]
fn the_sticks_pan_and_zoom() {
    let size = (1280, 720);
    let mut m = up(size);
    let s0 = m.view.unwrap().scale;
    m.steer(size, 0.1, [0.0, 0.0, 0.0, -1.0], Some(FREEPORT));
    assert!(m.view.unwrap().center[1] < FREEPORT[1], "up is north");
    let mut z = up(size);
    z.steer(size, 0.1, [0.0, -1.0, 0.0, 0.0], Some(FREEPORT));
    let v = z.view.unwrap();
    assert!((v.scale - s0 * ZOOM_RATE.powf(-0.1)).abs() < 1e-3, "{} vs {}", v.scale, s0);
    assert_eq!(v.center, FREEPORT, "the control: the right stick does not pan");
}

/// A mouse drag moves the map with the pointer and is no click; a press let
/// go where it was is one (a right one △'s); the wheel zooms about the
/// pointer. The control: a move under the slop is no drag.
#[test]
fn the_mouse_drags_clicks_and_wheels() {
    let size = (1280, 720);
    let mut m = up(size);
    let v0 = m.view.unwrap();
    assert_eq!(m.mouse_button(false, true, [600.0, 300.0]), None);
    m.mouse_moved([603.0, 300.0]);
    assert_eq!(m.view.unwrap(), v0, "the control: under the slop");
    m.mouse_moved([700.0, 350.0]);
    let grabbed = v0.to_world(size, [600.0, 300.0]);
    let now = m.view.unwrap().to_world(size, [700.0, 350.0]);
    assert!((now[0] - grabbed[0]).abs() < 1e-2 && (now[1] - grabbed[1]).abs() < 1e-2);
    assert_eq!(m.mouse_button(false, false, [700.0, 350.0]), None, "a drag is no click");
    assert!(!m.follow);
    assert_eq!(m.mouse_button(false, true, [10.0, 10.0]), None);
    assert_eq!(m.mouse_button(false, false, [10.0, 10.0]), Some(crate::mouse::Click::Target(10.0, 10.0)));
    m.mouse_button(true, true, [20.0, 20.0]);
    assert_eq!(m.mouse_button(true, false, [20.0, 20.0]), Some(crate::mouse::Click::Right(20.0, 20.0)));
    let before = m.view.unwrap();
    let under = before.to_world(size, [100.0, 600.0]);
    m.wheel(1.0, Some([100.0, 600.0]));
    let after = m.view.unwrap();
    assert!((after.scale - before.scale / ZOOM_STEP).abs() < 1e-3, "a notch up: a step in");
    let still = after.to_world(size, [100.0, 600.0]);
    assert!((still[0] - under[0]).abs() < 1e-2 && (still[1] - under[1]).abs() < 1e-2);
}

/// The wheel zooms about the pointer, and a frame later the point under it
/// is still there: the map no longer follows the player. On the player (the
/// middle), it zooms about the player and goes on following. The control:
/// a map still following goes back to the player and the point moves.
#[test]
fn the_wheel_away_from_the_player_stops_following() {
    let size = (1280, 720);
    let mut m = up(size);
    let under = m.view.unwrap().to_world(size, [300.0, 200.0]);
    m.wheel(2.0, Some([300.0, 200.0]));
    m.steer(size, 0.016, [0.0; 4], Some(FREEPORT));
    let still = m.view.unwrap().to_world(size, [300.0, 200.0]);
    assert!((still[0] - under[0]).abs() < 1e-2 && (still[1] - under[1]).abs() < 1e-2, "{still:?} vs {under:?}");
    assert!(!m.follow);
    let mut c = up(size);
    c.wheel(2.0, None);
    c.steer(size, 0.016, [0.0; 4], Some(FREEPORT));
    assert!(c.follow && c.view.unwrap().center == FREEPORT, "on the player: still following");
    c.zoom(ZOOM_STEP.powf(-2.0), [300.0, 200.0]);
    c.steer(size, 0.016, [0.0; 4], Some(FREEPORT));
    let moved = c.view.unwrap().to_world(size, [300.0, 200.0]);
    assert!((moved[0] - under[0]).abs() > 1.0, "the control: following, the zoom's point is lost");
}

/// Two fingers apart zoom in as far as they went apart (`touch.rs`'s pinch
/// in lines, a tenth a line), one drags. The control: the wheel's step for
/// the same lines.
#[test]
fn a_pinch_follows_the_fingers() {
    let size = (2400, 1080);
    let mut m = up(size);
    let s0 = m.view.unwrap().scale;
    // fingers 300 then 450 apart: touch.rs's lines
    let lines = (300.0f32 / 450.0).ln() / 0.9f32.ln();
    m.touch(0.0, 0.0, lines);
    let s1 = m.view.unwrap().scale;
    assert!((s1 - s0 * 300.0 / 450.0).abs() < 1e-3, "{s1} vs {}", s0 * 300.0 / 450.0);
    assert!((s0 * ZOOM_STEP.powf(-lines) - s1).abs() > 1e-2, "the control");
    let c = m.view.unwrap().center;
    m.touch(30.0, 0.0, 0.0);
    assert!(m.view.unwrap().center[0] < c[0], "a finger dragged east shows the west");
}

/// Tiles are wanted only while an overview texel is over two pixels, for the
/// cells shown, nearest the middle first, at most `MAX_TILES`. The control:
/// at the furthest zoom none.
#[test]
fn the_view_wants_the_tiles_it_shows() {
    let size = (1920, 1080);
    let cells = cells::Cells::new(&tunaria());
    let v = View { center: FREEPORT, scale: 2.0 };
    let want = wanted_tiles(&cells, v, size);
    let here = cells.zone_at[cells.at(FREEPORT[0], FREEPORT[1]).unwrap()].unwrap();
    assert_eq!(want.first(), Some(&here));
    // 3,840 by 2,160 units round Freeport: 3 or 4 columns, 2 or 3 rows
    assert!((4..=12).contains(&want.len()), "{}", want.len());
    let far = View { center: FREEPORT, scale: TILE_SCALE * 1.01 };
    assert!(wanted_tiles(&cells, far, size).is_empty(), "the control: too far out");
    let wide = View { center: [15000.0, 17000.0], scale: TILE_SCALE * 0.99 };
    assert_eq!(wanted_tiles(&cells, wide, (8000, 8000)).len(), MAX_TILES);
}

/// The compass's rose is where a click opens the map: its middle and its
/// rim, not the frame's gauges beside it (which target the player). The
/// control: a point in the player's frame past the rose.
#[test]
fn the_compass_opens_the_map() {
    assert!(on_compass([39.0, 54.0]));
    assert!(on_compass([39.0 + 21.0, 54.0]));
    assert!(on_compass([39.0, 54.0 - 21.0]));
    assert!(!on_compass([39.0 + 16.0, 54.0 + 16.0]), "past the rim, between it and the corner");
    assert!(!on_compass([100.0, 40.0]), "the control: the frame's gauges");
}

/// On the keyboard: M, Escape and Backspace close it (a held M's repeats do
/// not close it again as it opens); + and - zoom a step each press and
/// repeat; the arrows and WASD pan while held. The control: a key the map
/// does not use.
#[test]
fn the_keyboard_works_the_map() {
    let size = (1280, 720);
    let mut m = up(size);
    m.keyboard_key("m", true, true);
    assert!(m.open, "a repeat of the M that opened it");
    let s0 = m.view.unwrap().scale;
    m.keyboard_key("+", true, false);
    m.keyboard_key("+", true, true);
    m.steer(size, 0.0, [0.0; 4], Some(FREEPORT));
    assert!((m.view.unwrap().scale - s0 / ZOOM_STEP / ZOOM_STEP).abs() < 1e-3, "a press and its repeat: two steps");
    m.keyboard_key("d", true, false);
    m.steer(size, 0.1, [0.0; 4], Some(FREEPORT));
    m.keyboard_key("d", false, false);
    let x = m.view.unwrap().center[0];
    assert!(x > FREEPORT[0], "D pans east");
    m.steer(size, 0.1, [0.0; 4], Some(FREEPORT));
    assert_eq!(m.view.unwrap().center[0], x, "let go: still");
    m.keyboard_key("q", true, false);
    assert!(m.open, "the control: Q is not the map's");
    assert_eq!(keyboard("q"), None);
    m.keyboard_key("escape", true, false);
    assert!(!m.open);
}

/// Closed, the map lets its tiles go (they come back from the cache when it
/// opens again); the overview stays. The control: a map closed by a window
/// opening (the same close) and one only hidden keep nothing either way, and
/// before the close the tiles are there.
#[test]
fn closing_lets_the_tiles_go() {
    let mut m = up((1280, 720));
    m.world.as_mut().unwrap().tiles.insert(84, (3, 1));
    m.world.as_mut().unwrap().tiles.insert(85, (4, 1));
    assert_eq!(m.world.as_ref().unwrap().tiles.len(), 2, "the control: there before");
    m.key(key::TRIANGLE, true);
    assert!(!m.open);
    assert!(m.world.as_ref().unwrap().tiles.is_empty());
}

/// The build's time a frame is its most at 60 frames a second and 30, and a
/// share of the frame at 165, so a fast display keeps most of its rate.
/// The control: the most alone at 165 takes the whole 6 ms frame.
#[test]
fn the_build_takes_a_share_of_a_fast_frame() {
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    assert_eq!(ms(budget(OPEN_BUDGET, 1.0 / 60.0)).round(), 6.0);
    assert_eq!(ms(budget(OPEN_BUDGET, 1.0 / 30.0)), 6.0);
    assert_eq!(ms(budget(PLAY_BUDGET, 1.0 / 30.0)), 2.0);
    let fast = ms(budget(OPEN_BUDGET, 1.0 / 165.0));
    assert!((2.0..2.2).contains(&fast), "{fast}");
    assert!(ms(budget(PLAY_BUDGET, 1.0 / 165.0)) < 0.8);
    assert!(ms(OPEN_BUDGET.0) >= 1000.0 / 165.0 * 0.99, "the control: the most alone is the whole frame");
}

/// What a frame makes again: nothing while the art and the world stay; the
/// chrome and the world's pictures when the art swaps (F9), the view kept;
/// the world's map alone, its view kept, when the same world comes off the
/// other disc (Tunaria off the 2002 disc); a new world's map, its own view,
/// for another world. The control: the old rule, by the world's name
/// alone, missed the other disc's Tunaria.
#[test]
fn the_map_is_made_again_when_the_art_or_the_worlds_disc_changes() {
    use esf::Era::{Frontiers as F, Y2002 as Y};
    let none = Renew { layers: false, world: false, keep_view: false };
    assert_eq!(renew(Some(F), F, Some(("TUNARIA.ESF", F)), "TUNARIA.ESF", F), none);
    assert_eq!(renew(Some(Y), Y, Some(("TUNARIA.ESF", Y)), "TUNARIA.ESF", Y), none);
    // the art swapped: the chrome again, the world's pictures with it, where it looked kept
    assert_eq!(renew(Some(F), Y, Some(("TUNARIA.ESF", F)), "TUNARIA.ESF", F), Renew { layers: true, world: true, keep_view: true });
    // in a world the 2002 disc lacks, the same
    assert_eq!(renew(Some(F), Y, Some(("ODUS.ESF", F)), "ODUS.ESF", F), Renew { layers: true, world: true, keep_view: true });
    // the world off the other disc a frame later (a page's reads)
    assert_eq!(renew(Some(Y), Y, Some(("TUNARIA.ESF", F)), "TUNARIA.ESF", Y), Renew { layers: false, world: true, keep_view: true });
    // another world: its own map, its own view
    assert_eq!(renew(Some(Y), Y, Some(("TUNARIA.ESF", Y)), "ODUS.ESF", F), Renew { layers: false, world: true, keep_view: false });
    // the first frame
    assert_eq!(renew(None, F, None, "TUNARIA.ESF", F), Renew { layers: false, world: true, keep_view: false });
    // the control: by name alone the other disc's Tunaria is no new map
    let by_name = |mapped: Option<(&str, esf::Era)>, name: &str| mapped.is_none_or(|(n, _)| n != name);
    assert!(!by_name(Some(("TUNARIA.ESF", F)), "TUNARIA.ESF"));
    assert!(renew(Some(Y), Y, Some(("TUNARIA.ESF", F)), "TUNARIA.ESF", Y).world);
}
