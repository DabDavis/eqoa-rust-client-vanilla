//! `SetupTimeOfDayLights`' personal light: the radius grows 2 a second at
//! night and in rain outdoors under overcast, shrinks by day, clamped to
//! [0, 20]; mode 1 forces it, mode 2 turns it off.

use vi::fpu::{Ee, F};
use vi::tod::PersonalLight;

#[test]
fn the_lantern_ramps_by_the_hour() {
    let f = F::<Ee>::new;
    let mut l = PersonalLight::<Ee>::default();
    assert!(l.step(0, f(22.0), F::ZERO, F::ONE, F::ZERO).is_none(), "radius 0 at first");
    let (_, r, c) = l.step(1000, f(22.0), F::ZERO, F::ONE, F::ZERO).unwrap();
    assert_eq!(r.0, 2.0, "2 a second at night");
    assert_eq!([c[0].0, c[1].0, c[2].0], [1.0, 0.69, 0.36]);
    let (_, r, _) = l.step(30_000, f(3.0), F::ZERO, F::ONE, F::ZERO).unwrap();
    assert_eq!(r.0, 20.0, "clamped at the maximum");
    let (_, r, _) = l.step(31_000, f(12.0), F::ZERO, F::ONE, F::ZERO).unwrap();
    assert_eq!(r.0, 18.0, "shrinks by day");
    // rain under a heavy overcast outdoors keeps it growing by day
    let (_, r, _) = l.step(32_000, f(12.0), f(0.3), F::ONE, f(0.8)).unwrap();
    assert_eq!(r.0, 20.0);
    // negative controls: indoors, or a light overcast, it shrinks
    let (_, r, _) = l.step(33_000, f(12.0), f(0.3), F::ZERO, f(0.8)).unwrap();
    assert_eq!(r.0, 18.0);
    let (_, r, _) = l.step(34_000, f(12.0), f(0.3), F::ONE, f(0.4)).unwrap();
    assert_eq!(r.0, 16.0);
    l.mode = 2;
    assert!(l.step(35_000, f(22.0), F::ZERO, F::ONE, F::ZERO).is_none(), "mode 2 is off");
    l.mode = 1;
    let (_, r, _) = l.step(36_000, f(12.0), F::ZERO, F::ONE, F::ZERO).unwrap();
    assert_eq!(r.0, 20.0, "mode 1 grows by day, dt spanning the 2 s it was off (16 + 4)");
}
