//! The map's keys, pointer and view, without a GPU: a map over Tunaria's
//! layout (a zone a 2,000 square, x 2,000..28,000, z 0..34,000).

use super::testing::{FREEPORT, tunaria, up};
use super::*;

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
    m.keyboard_key("y", true, false);
    assert!(m.open, "the control: Y is not the map's");
    assert_eq!(keyboard("y"), None);
    assert_eq!(keyboard("q"), Some((key::SQUARE, Hold::Held)), "Q is the keyboard's □: the waypoint, held the group's");
    assert_eq!(keyboard("g"), Some((KEY_GROUP, Hold::Once)), "G: the group's waypoint");
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

fn near(a: [f32; 2], b: [f32; 2], e: f32) -> bool {
    (a[0] - b[0]).abs() <= e && (a[1] - b[1]).abs() <= e
}

/// A click on the map puts the world's waypoint at the point under it; a
/// click on it (within twice its diamond) takes it away, a finger's tap
/// from a little farther; a click elsewhere moves it. The controls: a
/// click just outside the mouse's reach does not take it away; a map not
/// up takes no click; off the world's edge nothing is put.
#[test]
fn a_click_puts_the_waypoint_and_one_on_it_takes_it_away() {
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let v = m.view.unwrap();
    assert!(m.click([900.0, 300.0], false));
    let at = m.waypoint(0).expect("put");
    assert!(near(at, v.to_world(size, [900.0, 300.0]), 1e-3), "{at:?}");
    assert_eq!(m.waypoint(1), None, "one world's");
    let reach = view::waypoint_reach(size, false);
    let finger = view::waypoint_reach(size, true);
    assert!(finger > reach);
    // just outside the mouse's reach: the click moves it there instead
    assert!(m.click([900.0 + reach + 1.0, 300.0], false));
    let moved = m.waypoint(0).expect("moved, not taken away");
    assert!(!near(moved, at, 1e-3), "the control: just outside the reach moves it");
    // a finger from farther than the mouse's reach takes it away
    let p = v.to_window(size, moved);
    assert!(m.click([p[0] + (reach + finger) / 2.0, p[1]], true));
    assert_eq!(m.waypoint(0), None, "a tap a little off it takes it away");
    // on it with the mouse
    m.click([400.0, 500.0], false);
    assert!(m.click([401.0, 501.0], false));
    assert_eq!(m.waypoint(0), None);
    // off the world: nothing put
    let mut far = up(size);
    far.view = Some(View { center: [2000.0, 0.0], scale: 10.0 });
    assert!(!far.click([10.0, 10.0], false), "west and north of the world's edge");
    assert_eq!(far.waypoint(0), None);
    // the control: a map not up
    let mut shut = up(size);
    shut.close();
    assert!(!shut.click([900.0, 300.0], false));
    assert_eq!(shut.waypoint(0), None);
}

/// A press of □ and its release.
fn tap(m: &mut Map) {
    m.key(key::SQUARE, true);
    m.key(key::SQUARE, false);
}

/// □ (Q) puts the waypoint at the map's middle once the map is panned off
/// the player, takes it away at the middle, and takes it away wherever it
/// is while the map follows the player; it acts as it is let go (held, it is
/// the group's: `square_held_is_the_groups`). The control: following the
/// player with none, □ puts nothing (the middle is the player, already
/// there).
#[test]
fn square_puts_it_at_the_middle_once_panned() {
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    tap(&mut m);
    assert_eq!(m.waypoint(0), None, "the control: following, none put");
    m.mouse_button(false, true, [640.0, 360.0]);
    m.mouse_moved([540.0, 300.0]);
    m.mouse_button(false, false, [540.0, 300.0]);
    m.steer(size, 0.0, [0.0; 4], Some(FREEPORT));
    let middle = m.view.unwrap().center;
    assert_ne!(middle, FREEPORT, "panned");
    m.keyboard_key("q", true, false);
    assert_eq!(m.waypoint(0), None, "pressed: nothing yet");
    m.keyboard_key("q", false, false);
    assert_eq!(m.waypoint(0), Some(middle), "Q at the middle, as it is let go");
    tap(&mut m);
    assert_eq!(m.waypoint(0), None, "□ at the middle takes it away");
    tap(&mut m);
    m.key(key::CROSS, true);
    m.steer(size, 0.0, [0.0; 4], Some(FREEPORT));
    tap(&mut m);
    assert_eq!(m.waypoint(0), None, "following the player: taken away wherever it was");
}

/// F3 20261026-231048 (Liav's): while the map follows the player, □ puts
/// the waypoint on the target's spot and a second □ takes it away there;
/// once the map is panned, the middle is □'s spot, the target or not (what
/// the player chose on the map first); with the target out of view □ is as
/// it was. G (the group's at once) takes the same spots. The control: the
/// same □ with no target takes the waypoint away.
#[test]
fn the_target_is_squares_spot_while_following() {
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let merchant = [25475.9, 15704.6];
    m.target = Some(merchant);
    tap(&mut m);
    assert_eq!(m.waypoint(0), Some(merchant), "on the target");
    tap(&mut m);
    assert_eq!(m.waypoint(0), None, "on it already: taken away");
    tap(&mut m);
    m.key(KEY_GROUP, true);
    assert_eq!(m.take_group_ask(), Some(community::Spot { world: 0, at: merchant }), "G: the target's spot");
    // another world mapped than the player's: no target on it
    m.player_world = 2;
    assert_eq!(m.target_spot(), None);
    m.player_world = 0;
    // panned: the middle
    m.follow = false;
    let there = [21000.0, 12000.0];
    m.view = Some(View { center: there, ..m.view.unwrap() });
    tap(&mut m);
    assert_eq!(m.waypoint(0), Some(there), "panned: the middle, not the target");
    m.key(KEY_GROUP, true);
    assert_eq!(m.take_group_ask(), Some(community::Spot { world: 0, at: there }), "G panned: the middle");
    // following again, the target out of view: as it was
    m.follow = true;
    m.view = Some(View { center: FREEPORT, ..m.view.unwrap() });
    m.target = None;
    tap(&mut m);
    assert_eq!(m.waypoint(0), None, "the control: no target, taken away");
    m.key(KEY_GROUP, true);
    assert_eq!(m.take_group_ask(), Some(community::Spot { world: 0, at: FREEPORT }), "G, no target: the player's own place");
}

/// □ held `group_waypoint::HOLD` seconds of the clock asks the group to be
/// told of □'s spot, once, the same in seconds at 30 frames a second and at
/// 165 (by the clock, not the frames); a repeat while it is down is nothing,
/// and its release is nothing. The control: let go before the hold it is
/// □'s own waypoint, and the group is asked nothing.
#[test]
fn square_held_is_the_groups() {
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    for hz in [30u32, 165] {
        let mut m = up(size);
        let dt = 1.0 / hz as f32;
        m.key(key::SQUARE, true);
        let (mut asked, mut frames) = (None, 0);
        while asked.is_none() && frames < 1000 {
            m.key(key::SQUARE, true);
            m.hold(dt);
            frames += 1;
            asked = m.take_group_ask();
        }
        let secs = frames as f32 * dt;
        assert!(secs >= group_waypoint::HOLD - 1e-4 && secs < group_waypoint::HOLD + dt * 1.5, "{hz} Hz: {secs} s");
        assert_eq!(asked, Some(community::Spot { world: 0, at: FREEPORT }), "following, no target: the player's own place");
        m.hold(dt);
        m.key(key::SQUARE, false);
        assert_eq!((m.take_group_ask(), m.waypoint(0)), (None, None), "asked once; the release nothing");
    }
    // the control: panned, let go a little before the hold
    let mut m = up(size);
    m.follow = false;
    m.key(key::SQUARE, true);
    m.hold(group_waypoint::HOLD - 0.05);
    m.key(key::SQUARE, false);
    assert_eq!((m.take_group_ask(), m.waypoint(0)), (None, Some(FREEPORT)), "□'s own waypoint at the middle");
    // a map closed while □ is down forgets it
    m.key(key::SQUARE, true);
    m.close();
    m.show();
    m.hold(group_waypoint::HOLD * 2.0);
    assert_eq!(m.take_group_ask(), None, "closed: the hold forgotten");
}

/// A click with Shift held asks the group to be told of the spot under it
/// (a coach's own spot on its horseshoe), and leaves the player's waypoint
/// be; a finger held still asks the same (`long_press`), but not on the
/// chrome's buttons; off the world's edge nothing. The control: the same
/// click without Shift puts the player's own.
#[test]
fn shift_and_a_click_or_a_finger_held_tell_the_group() {
    use super::markers_data::COACHES;
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let v = m.view.unwrap();
    m.shift = true;
    assert!(m.click([900.0, 300.0], false));
    let asked = m.take_group_ask().expect("asked");
    assert!(near(asked.at, v.to_world(size, [900.0, 300.0]), 1e-3) && asked.world == 0, "{asked:?}");
    assert_eq!(m.waypoint(0), None, "the player's own left be");
    let ronks = COACHES.iter().position(|c| c.coachman == "Coachman Ronks").unwrap();
    let p = v.to_window(size, COACHES[ronks].at);
    assert!(m.click([p[0] + 2.0, p[1]], false));
    assert_eq!(m.take_group_ask().map(|s| s.at), Some(COACHES[ronks].at), "the coach's own spot");
    m.shift = false;
    assert!(m.click([900.0, 300.0], false));
    assert_eq!(m.take_group_ask(), None, "the control: no Shift");
    assert!(m.waypoint(0).is_some(), "the control: the player's own");
    // a finger held still, on a corner with no marker
    assert!(m.long_press([40.0, 680.0]));
    assert!(near(m.take_group_ask().unwrap().at, v.to_world(size, [40.0, 680.0]), 1e-3));
    m.buttons = vec![([600, 10, 640, 30], key::TRIANGLE)];
    assert!(!m.long_press(render::ui::window_from_frame(size, [620.0, 20.0])), "on a button");
    assert_eq!(m.take_group_ask(), None);
    let mut far = up(size);
    far.view = Some(View { center: [2000.0, 0.0], scale: 10.0 });
    far.shift = true;
    assert!(!far.click([10.0, 10.0], false), "past the world's edge");
    assert_eq!(far.take_group_ask(), None);
}

/// The group's waypoint goes once the player reaches it, but only after
/// being away from it (the sender's own, put where they stand, stays until
/// they leave and come back); a click on its diamond takes it away; □
/// following the player takes the player's own first, then the group's.
/// The controls: just outside `ARRIVE` it stays; in another world it stays.
#[test]
fn the_groups_waypoint_goes_when_reached_or_taken_away() {
    use group_waypoint::GroupMark;
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let at = [20000.0, 14000.0];
    let mark = || GroupMark { world: 0, at, from: "Bob".into() };
    let player = |x: f32| Some(([x, 50.0, at[1]], 0.0));
    m.set_group_waypoint(mark());
    m.arrive(player(at[0] + 3.0), 0);
    assert!(m.group_mark().is_some(), "put where the player stands: stays");
    m.arrive(player(at[0] + ARRIVE + 1.0), 0);
    assert!(m.group_mark().is_some(), "the control: just outside");
    m.arrive(player(at[0] + 3.0), 1);
    assert!(m.group_mark().is_some(), "the control: in another world");
    m.arrive(player(at[0] + 3.0), 0);
    assert_eq!(m.group_mark(), None, "away, then back: reached");
    // a click on it
    m.set_group_waypoint(mark());
    let p = m.view.unwrap().to_window(size, at);
    assert!(m.click([p[0] + 1.0, p[1]], false));
    assert_eq!((m.group_mark(), m.waypoint(0)), (None, None), "taken away, none put");
    // □ following: the player's own first, then the group's
    m.set_group_waypoint(mark());
    m.click([900.0, 300.0], false);
    tap(&mut m);
    assert!(m.waypoint(0).is_none() && m.group_mark().is_some(), "the player's own first");
    tap(&mut m);
    assert_eq!(m.group_mark(), None, "then the group's");
    assert!(!m.describe().contains("group's waypoint"), "{}", m.describe());
    m.set_group_waypoint(mark());
    assert!(m.describe().contains("the group's waypoint from Bob in world 0 at (20000, 14000)"), "{}", m.describe());
    // gone with its sender
    m.keep_group_waypoint(true, |n| n == "Bob");
    assert!(m.group_mark().is_some(), "the control: Bob in the group");
    m.keep_group_waypoint(true, |n| n == "Dairine");
    assert_eq!(m.group_mark(), None, "Bob out of it");
}

/// Suggest takes the spot the player chose last (`suggest`): a waypoint put
/// since the map opened over the pointer; the pointer over the target; the
/// target, while the map follows the player, over a waypoint from before,
/// saying it is the target's (`take_suggest_target`); a waypoint from before
/// over the middle. The control: no target, the waypoint from before.
#[test]
fn suggest_takes_the_spot_chosen_last() {
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let v = m.view.unwrap();
    let merchant = [25475.9, 15704.6];
    m.target = Some(merchant);
    assert!(m.click([40.0, 680.0], false));
    let w = m.waypoint(0).unwrap();
    m.mouse_moved([300.0, 200.0]);
    m.key(key::L3, true);
    assert_eq!((m.take_suggest().map(|s| s.at), m.take_suggest_target()), (Some(w), false), "this time's waypoint");
    // opened again: the waypoint is from before
    m.close();
    m.show();
    m.mouse_moved([300.0, 200.0]);
    m.key(key::L3, true);
    let s = m.take_suggest().unwrap();
    assert!(near(s.at, v.to_world(size, [300.0, 200.0]), 1e-3) && !m.take_suggest_target(), "the pointer over the target");
    m.let_go();
    m.key(key::L3, true);
    assert_eq!((m.take_suggest().map(|s| s.at), m.take_suggest_target()), (Some(merchant), true), "the target over the old waypoint");
    m.target = None;
    m.key(key::L3, true);
    assert_eq!((m.take_suggest().map(|s| s.at), m.take_suggest_target()), (Some(w), false), "the control: no target, the old waypoint");
    m.set_waypoint(0, None);
    m.key(key::L3, true);
    assert_eq!(m.take_suggest().map(|s| s.at), Some(v.center), "nothing chosen: the middle");
}

/// The group's line names worlds as the map's title does (`gWorldInfo`'s
/// word, else the mapped world's file) and reads them back in any case;
/// the area named when the map has one. The control: a world it cannot
/// name is none.
#[test]
fn the_groups_line_names_the_world() {
    let mut m = up((1280, 720));
    let words = vi::strtab::StringTable::parse(b"English\r\nTunaria\r\nOdus").unwrap();
    m.names.names[0] = Some(2);
    m.names.names[2] = Some(3);
    assert_eq!(m.world_title(2, Some(&words)).as_deref(), Some("Odus"));
    assert_eq!(m.world_named("odus", Some(&words)), Some(2));
    assert_eq!(m.world_named("TUNARIA", Some(&words)), Some(0));
    assert_eq!(m.world_named("Tunaria", None), Some(0), "no words: the mapped world's file");
    assert_eq!(m.world_named("Faydwer", Some(&words)), None, "the control");
    let line = m.group_line(community::Spot { world: 2, at: [1234.4, -5.0] }, Some(&words)).unwrap();
    assert_eq!(line, "Waypoint in Odus at x 1234, z -5");
    assert_eq!(m.group_line(community::Spot { world: 4, at: [1.0, 2.0] }, Some(&words)), None, "a world with no name");
}

/// The waypoints are kept as they change, a line a world, this window's own
/// (window 2's `waypoints-2.opt`), and read back by a map made later; a
/// line that does not read is left out. The control: a store that refuses
/// keeps nothing, and the map still has it.
#[test]
fn the_waypoints_are_kept_per_world_and_per_window() {
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    m.click([900.0, 300.0], false);
    let at = m.waypoint(0).unwrap();
    let kept = String::from_utf8(crate::settings::testing::kept("waypoints.opt").expect("kept")).unwrap();
    assert!(kept.starts_with("0 "), "{kept:?}");
    let mut later = Map::default();
    later.read_waypoints();
    assert_eq!(later.waypoint(0), Some(at), "read back");
    assert_eq!(later.waypoint(2), None);
    crate::settings::testing::put("waypoints.opt", b"0 1 2\nnot a line\n2 300.5 400\n1 nan 3\n");
    let mut read = Map::default();
    read.read_waypoints();
    assert_eq!((read.waypoint(0), read.waypoint(2), read.waypoint(1)), (Some([1.0, 2.0]), Some([300.5, 400.0]), None));
    // window 2 keeps its own
    crate::multibox::testing::set_place(Some(1));
    let mut two = up(size);
    two.click([300.0, 300.0], false);
    assert!(crate::settings::testing::kept("waypoints-2.opt").is_some(), "window 2's own file");
    crate::multibox::testing::set_place(None);
    // the control: a store that refuses
    crate::settings::testing::reset(true);
    let mut refused = up(size);
    refused.click([900.0, 300.0], false);
    assert!(refused.waypoint(0).is_some(), "the map has it");
    assert_eq!(crate::settings::testing::kept("waypoints.opt"), None, "nothing kept");
    crate::settings::testing::reset(false);
}

/// The player reaching the waypoint (within `ARRIVE`) takes it away; in
/// another world, or just outside, it stays. The control: 1 past the
/// radius keeps it.
#[test]
fn reaching_the_waypoint_takes_it_away() {
    crate::settings::testing::reset(false);
    let mut m = up((1280, 720));
    m.click([900.0, 300.0], false);
    let at = m.waypoint(0).unwrap();
    m.arrive(Some(([at[0] + ARRIVE + 1.0, 50.0, at[1]], 0.0)), 0);
    assert!(m.waypoint(0).is_some(), "the control: just outside");
    m.arrive(Some(([at[0], 50.0, at[1] + 1.0], 0.0)), 1);
    assert!(m.waypoint(0).is_some(), "in another world");
    m.arrive(Some(([at[0] + ARRIVE - 1.0, 50.0, at[1]], 0.0)), 0);
    assert_eq!(m.waypoint(0), None, "reached");
}

/// The markers answer the pointer: the mouse over Coachman Ronks's horseshoe
/// shows its card and off every marker none; a finger's tap shows the card and
/// puts no waypoint, a second tap puts the waypoint on the coach, a click on a
/// camp at once; the pad's middle cross over a marker shows its card once the
/// map is panned (with a keyboard in use, not). Hidden (○, X on a keyboard)
/// they answer nothing: the control, a click a little off the coach puts the
/// waypoint there, where shown it went on the coach.
#[test]
fn the_markers_answer_the_pointer() {
    use super::markers::Marker;
    use super::markers_data::{CAMPS, COACHES};
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let v = m.view.unwrap();
    let ronks = COACHES.iter().position(|c| c.coachman == "Coachman Ronks").unwrap();
    let at = COACHES[ronks].at;
    let p = v.to_window(size, at);
    m.mouse_moved(p);
    assert_eq!(m.card_marker(true), Some(Marker::Coach(ronks)), "the mouse over it");
    let empty = [40.0, 680.0];
    assert_eq!(markers::under(0, v, size, empty, true), None, "a corner with no marker");
    m.mouse_moved(empty);
    assert_eq!(m.card_marker(true), None, "off every marker");
    m.let_go();
    // a finger: the card, then the waypoint
    assert!(m.click([p[0] + 3.0, p[1]], true));
    assert_eq!((m.waypoint(0), m.card_marker(false)), (None, Some(Marker::Coach(ronks))), "a tap shows its card");
    assert!(m.click(p, true));
    assert_eq!(m.waypoint(0), Some(at), "tapped again: the waypoint on the coach");
    assert_eq!(m.card_marker(false), None, "and the card goes");
    // a click on a camp in view: the waypoint on it at once
    let nasehir = CAMPS.iter().position(|c| c.place == "Northern Ro's Nasehir, the first camp").unwrap();
    let q = v.to_window(size, CAMPS[nasehir].at);
    assert!((0.0..1280.0).contains(&q[0]) && (0.0..720.0).contains(&q[1]), "in the opening view: {q:?}");
    assert!(m.click([q[0] - 2.0, q[1] + 2.0], false));
    assert_eq!(m.waypoint(0), Some(CAMPS[nasehir].at));
    // the pad's cross, the map panned onto Ronks
    m.follow = false;
    m.view = Some(View { center: at, ..v });
    assert_eq!(m.card_marker(false), Some(Marker::Coach(ronks)), "the pad's cross on it");
    assert_eq!(m.card_marker(true), None, "with a keyboard in use, its mouse decides");
    m.view = Some(v);
    m.follow = true;
    // hidden: X is ○
    assert_eq!(keyboard("x"), Some((key::CIRCLE, Hold::Once)));
    assert!(m.key(key::CIRCLE, true));
    assert!(!crate::settings::map_markers(), "hidden, and kept");
    m.mouse_moved(p);
    assert_eq!(m.card_marker(true), None, "hidden: no card");
    m.click([p[0] + 5.0, p[1]], false);
    let put = m.waypoint(0).unwrap();
    assert!(near(put, v.to_world(size, [p[0] + 5.0, p[1]]), 1e-3) && !near(put, at, 1.0), "the control: put where clicked, {put:?}");
    m.key(key::CIRCLE, true);
    assert!(crate::settings::map_markers(), "shown again");
}

/// The horseshoe comes off the disc once: the builder keeps it out of a zone
/// it reads while it is wanted, and not when it is not (the control).
#[test]
fn the_builder_keeps_the_stable_signs_horseshoe() {
    let mut b = build::Builder::new(WORLD_FILE, &tunaria(), None, None);
    let zone = |with: bool| {
        // a zone's records with one surface: the sign's horseshoe, or another
        let id = if with { markers::ICON } else { 0x1234_5678 };
        let mut body = Vec::new();
        for v in [id, 2, 2, 1, 1, 1] {
            body.extend_from_slice(&v.to_le_bytes());
        }
        body.extend_from_slice(&[255, 255, 255, 255]);
        body.extend_from_slice(&2i32.to_le_bytes());
        body.extend_from_slice(&[0, 0, 0, 0]);
        let mut rec = Vec::new();
        rec.extend_from_slice(&0x1000u16.to_le_bytes());
        rec.extend_from_slice(&[0, 0]);
        rec.extend_from_slice(&(body.len() as u32).to_le_bytes());
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&body);
        esf::Container::parse_records(rec)
    };
    b.want_icon(false);
    b.look_for_icon(&zone(true));
    assert!(b.icon.is_none(), "the control: not wanted, not kept");
    b.want_icon(true);
    b.look_for_icon(&zone(false));
    assert!(b.icon.is_none(), "another surface");
    b.look_for_icon(&zone(true));
    let im = b.icon.take().expect("kept");
    assert_eq!((im.width, im.height), (2, 2));
}

/// T (Select on a pad) hides the towns' names and shows them again, kept,
/// apart from the markers; the map says so.
#[test]
fn t_turns_the_town_names() {
    crate::settings::testing::reset(false);
    let mut m = up((1280, 720));
    assert_eq!(keyboard("t"), Some((key::SELECT, Hold::Once)));
    assert!(crate::settings::map_towns(), "on at first");
    m.keyboard_key("t", true, false);
    assert!(!crate::settings::map_towns() && crate::settings::map_markers(), "hidden, the markers kept");
    assert!(m.describe().contains("town names hidden"), "{}", m.describe());
    m.keyboard_key("t", true, true);
    assert!(!crate::settings::map_towns(), "a repeat is not another press");
    assert!(m.key(key::SELECT, true));
    assert!(crate::settings::map_towns(), "Select: shown again");
}

/// The quest marks answer the pointer over the markers: the mouse over a
/// giver's "!" where a coach stands shows the mark's card, not the coach's
/// (the control: the mark gone, the coach's); a finger's tap shows the mark's
/// card with no waypoint, a second tap puts the waypoint on the giver; a coach
/// a finger tapped keeps its card under the mouse over a mark (the control:
/// let go, the mark's). J is R3: hidden, kept, the set let go and nothing
/// answers; R3 again shows them.
#[test]
fn the_quest_marks_answer_the_pointer_over_the_markers() {
    use super::markers::Marker;
    use super::markers_data::COACHES;
    use net::session::{MarkQuest, QuestMark};
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let v = m.view.unwrap();
    let ronks = COACHES.iter().position(|c| c.coachman == "Coachman Ronks").unwrap();
    let at = COACHES[ronks].at;
    let p = v.to_window(size, at);
    let giver = QuestMark { kind: 1, world: 0, x: at[0] + 1.0, z: at[1], npc: "Commander Nothard".into(), quests: vec![MarkQuest { name: "The Militia Bracer".into(), levels: (1, 60), line: "Every recruit wears our bracer.".into(), detail: "430 XP".into() }] };
    m.quests.set(std::slice::from_ref(&giver));
    m.mouse_moved(p);
    assert_eq!(m.cards(true), (Some(0), None), "the mark's card over the coach's");
    m.quests.set(&[]);
    assert_eq!(m.cards(true), (None, Some(Marker::Coach(ronks))), "the control: no mark, the coach's");
    m.quests.set(std::slice::from_ref(&giver));
    m.let_go();
    // a finger: the mark's card, then the waypoint on the giver
    assert!(m.click([p[0] + 3.0, p[1]], true));
    assert_eq!((m.cards(false), m.waypoint(0)), ((Some(0), None), None), "a tap shows its card");
    assert!(m.click(p, true));
    assert_eq!((m.waypoint(0), m.quests.pinned), (Some([giver.x, giver.z]), None), "tapped again: the waypoint on the giver");
    m.set_waypoint(0, None);
    // the mark off the coach: a coach a finger tapped keeps its card
    let off = v.to_world(size, [p[0] + 120.0, p[1]]);
    m.quests.set(&[QuestMark { x: off[0], z: off[1], ..giver.clone() }]);
    assert!(m.click(p, true));
    m.mouse_moved([p[0] + 120.0, p[1]]);
    assert_eq!(m.cards(true), (None, Some(Marker::Coach(ronks))), "the tapped coach's");
    m.pinned = None;
    assert_eq!(m.cards(true), (Some(0), None), "the control: let go, the mark's under the mouse");
    // J is R3: hidden and kept, the set let go, nothing answers
    assert_eq!(keyboard("j"), Some((key::R3, Hold::Once)));
    assert!(m.key(key::R3, true));
    assert!(!crate::settings::quest_markers(), "hidden, and kept");
    assert!(m.quests.marks.is_empty() && m.cards(true).0.is_none(), "nothing answers");
    assert!(m.key(key::R3, true));
    assert!(crate::settings::quest_markers(), "shown again");
}

/// The map names the world's place under the pointer (x and z, as the game
/// counts them), and under a pad's cross once the map is panned off the
/// player; the controls: following the player with no pointer it names none,
/// and the place moves with the pointer.
#[test]
fn the_map_names_the_place_under_the_pointer() {
    let size = (640, 448);
    let mut m = super::testing::up(size);
    assert_eq!(m.pointer_world(false), None, "the control: following the player, no pointer");
    m.mouse_moved([320.0, 224.0]);
    let mid = m.pointer_world(false).unwrap();
    assert!((mid[0] - super::testing::FREEPORT[0]).abs() < 1.0 && (mid[1] - super::testing::FREEPORT[1]).abs() < 1.0, "the middle is the player's place: {mid:?}");
    m.mouse_moved([420.0, 224.0]);
    let right = m.pointer_world(false).unwrap();
    assert!(right[0] > mid[0] + 1.0 && (right[1] - mid[1]).abs() < 1.0, "a pointer to the right is east of it: {right:?}");
    m.let_go();
    assert_eq!(m.pointer_world(false), None, "the pointer gone, none");
    m.follow = false;
    assert!(m.pointer_world(false).is_some(), "a pad's cross on a panned map");
    assert_eq!(m.pointer_world(true), None, "the control: a keyboard has no cross");
}

/// The community's markers answer the pointer as the camps and coaches do:
/// the mouse over a pin's head shows its card, never another world's pin at
/// the same place (the control: that one alone, no card); a finger's tap
/// shows the card and a second tap puts the waypoint on the spot; the pad's
/// cross over it once the map is panned; the map says how many it has.
/// Hidden with the markers (X) they answer nothing: the control, a click on
/// a pin then puts the waypoint where clicked.
#[test]
fn the_community_markers_answer_the_pointer() {
    use super::community::{Kind, Mark};
    crate::settings::testing::reset(false);
    let size = (1280, 720);
    let mut m = up(size);
    let v = m.view.unwrap();
    // a spot in an empty corner (no camp or coach near: `the_markers_answer_the_pointer`)
    let at = v.to_world(size, [60.0, 650.0]);
    let mark = |world: i32, id: i64| Mark { id, world, at, name: "Hidden cave".into(), note: "Behind the falls".into(), kind: Kind::Landmark };
    m.community.marks = vec![mark(1, 1), mark(0, 2)];
    let r = m.pin_size(v);
    let head = community::head(v.to_window(size, at), r);
    m.mouse_moved(head);
    assert_eq!(m.card_community(true), Some(1), "world 0's pin");
    assert!(m.describe().contains(", 2 community markers"), "{}", m.describe());
    m.mouse_moved([640.0, 100.0]);
    assert_eq!(m.card_community(true), None, "off it");
    m.community.marks = vec![mark(1, 1)];
    m.mouse_moved(head);
    assert_eq!(m.card_community(true), None, "the control: another world's alone");
    m.community.marks = vec![mark(1, 1), mark(0, 2)];
    m.let_go();
    // a finger: the card, then the waypoint on the spot
    assert!(m.click([head[0] + 3.0, head[1]], true));
    assert_eq!((m.waypoint(0), m.card_community(false)), (None, Some(1)), "a tap shows its card");
    assert!(m.click(head, true));
    assert_eq!((m.waypoint(0), m.community.pinned), (Some(at), None), "tapped again: the waypoint on it");
    m.set_waypoint(0, None);
    // the pad's cross on it, the map panned there
    m.follow = false;
    m.view = Some(View { center: at, ..v });
    assert_eq!(m.card_community(false), Some(1), "the pad's cross");
    assert_eq!(m.card_community(true), None, "with a keyboard in use, its mouse decides");
    m.view = Some(v);
    m.follow = true;
    // hidden with the markers
    assert!(m.key(key::CIRCLE, true));
    m.mouse_moved(head);
    assert_eq!(m.card_community(true), None, "hidden: no card");
    m.click(head, false);
    let put = m.waypoint(0).unwrap();
    assert!(near(put, v.to_world(size, head), 1e-3) && !near(put, at, 1.0), "the control: put where clicked, {put:?}");
    m.key(key::CIRCLE, true);
}

/// N (L3, the Suggest button) asks to suggest a spot of the map's world, once:
/// the map's middle with no pointer and the map following the player; the
/// place under the mouse; not a mouse on the chrome's buttons (Suggest's
/// own: the middle then); this world's waypoint over all; a pad's cross once
/// the map is panned. The control: a map with no view asks nothing.
#[test]
fn n_suggests_a_spot() {
    crate::settings::testing::reset(false);
    assert_eq!(keyboard("n"), Some((key::L3, Hold::Once)));
    let size = (1280, 720);
    let mut m = up(size);
    let v = m.view.unwrap();
    m.keyboard = true;
    assert!(m.key(key::L3, true));
    assert_eq!(m.take_suggest(), Some(community::Spot { world: 0, at: v.center }), "the middle");
    assert_eq!(m.take_suggest(), None, "once");
    m.mouse_moved([300.0, 200.0]);
    m.key(key::L3, true);
    let s = m.take_suggest().unwrap();
    assert!(near(s.at, v.to_world(size, [300.0, 200.0]), 1e-3), "under the mouse: {s:?}");
    m.buttons = vec![([600, 10, 640, 30], key::L3)];
    m.mouse_moved(render::ui::window_from_frame(size, [620.0, 20.0]));
    m.key(key::L3, true);
    assert_eq!(m.take_suggest().unwrap().at, v.center, "the button's own pointer: the middle");
    m.buttons.clear();
    // the waypoint over the mouse's place
    assert!(m.click([40.0, 680.0], false));
    let w = m.waypoint(0).unwrap();
    m.mouse_moved([300.0, 200.0]);
    m.key(key::L3, true);
    assert_eq!(m.take_suggest().unwrap().at, w, "the waypoint");
    m.set_waypoint(0, None);
    // a pad's cross, the map panned
    m.let_go();
    m.keyboard = false;
    m.follow = false;
    let there = [v.center[0] + 500.0, v.center[1] - 300.0];
    m.view = Some(View { center: there, ..v });
    m.key(key::L3, true);
    assert!(near(m.take_suggest().unwrap().at, there, 1e-3), "the pad's cross");
    let mut none = Map::default();
    none.show();
    none.key(key::L3, true);
    assert_eq!(none.take_suggest(), None, "the control: no view");
}
