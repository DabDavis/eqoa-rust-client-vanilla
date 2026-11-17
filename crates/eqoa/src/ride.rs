//! go-eqoa-server's own, no retail message: the player rides an NPC's vehicle
//! (the server owner, 2026-11-17: Santa Claus's sleigh tour of Freeport, "the
//! sled with the pegasus and santa will altogether fly together"). The server
//! flies the vehicle, its team and its host along the tour's route and holds
//! each rider's place on its deck; a client that rides (one that answered the
//! decorations' offer with layout 2: `net::decor::LAYOUT`) is told
//! [`OP_RIDE`] on, keeps its walker on the deck as it draws the vehicle every
//! frame, with no fall and no walk of its own (the server sends a run speed of
//! 0 as well), until told off, with the place the server set it down at. A
//! PS2 is moved by teleports instead.

/// go-eqoa-server's `OpcodeRide`: a u8, 1 on, 0 off. On: the vehicle NPC's
/// name (u8 byte count, keyed as the tints key one) and the rider's place on
/// it, f32 x, y, z in its frame (x right, y up, z ahead, as a light string's).
/// Off: f32 x, y, z and heading, where the server set the rider down.
pub const OP_RIDE: u16 = 0x7E83;

/// A ride: the vehicle (its name's key) and the rider's place on its deck.
#[derive(Clone, Debug, PartialEq)]
pub struct Ride {
    pub vehicle: String,
    pub at: [f32; 3],
}

/// What [`OP_RIDE`] says.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    On(Ride),
    Off([f32; 4]),
}

/// [`OP_RIDE`]'s body (after the opcode); none if cut short or not finite.
pub fn read(data: &[u8]) -> Option<Said> {
    let mut r = net::stream::Reader::new(data);
    let said = match r.u8() {
        1 => {
            let len = r.u8() as usize;
            let vehicle = net::npctint::key(&String::from_utf8_lossy(r.bytes(len)));
            let at = [r.f32(), r.f32(), r.f32()];
            if !at.iter().all(|v| v.is_finite()) {
                return None;
            }
            Said::On(Ride { vehicle, at })
        }
        _ => {
            let at = [r.f32(), r.f32(), r.f32(), r.f32()];
            if !at.iter().all(|v| v.is_finite()) {
                return None;
            }
            Said::Off(at)
        }
    };
    (!r.error).then_some(said)
}

/// Where a rider stands: the vehicle drawn at `at` facing `heading`, the
/// rider's place `q` in its frame (as a light string's: `decorlights`).
pub fn place(at: [f32; 3], heading: f32, q: [f32; 3]) -> [f32; 3] {
    let (sin, cos) = heading.sin_cos();
    [at[0] + q[2] * sin + q[0] * cos, at[1] + q[1], at[2] + q[2] * cos - q[0] * sin]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_on_and_off() {
        let mut b = vec![1, 14];
        b.extend(b"Santa's Sleigh");
        for v in [-0.9f32, 0.72, 0.65] {
            b.extend(v.to_le_bytes());
        }
        assert_eq!(read(&b), Some(Said::On(Ride { vehicle: "santa's sleigh".into(), at: [-0.9, 0.72, 0.65] })));
        assert_eq!(read(&b[..b.len() - 1]), None, "cut short");
        let mut o = vec![0];
        for v in [25281.0f32, 54.1, 15761.0, 4.2] {
            o.extend(v.to_le_bytes());
        }
        assert_eq!(read(&o), Some(Said::Off([25281.0, 54.1, 15761.0, 4.2])));
    }

    /// The server's frame (entity/npctour.go's riderAt, a light string's): a
    /// place ahead of a vehicle facing east is east of it, one along its x
    /// north of it.
    #[test]
    fn places_in_the_vehicles_frame() {
        let east = std::f32::consts::FRAC_PI_2;
        let p = place([100.0, 50.0, 200.0], east, [0.0, 1.0, 2.0]);
        assert!((p[0] - 102.0).abs() < 1e-4 && (p[1] - 51.0).abs() < 1e-6 && (p[2] - 200.0).abs() < 1e-4, "{p:?}");
        let p = place([100.0, 50.0, 200.0], east, [1.0, 0.0, 0.0]);
        assert!((p[0] - 100.0).abs() < 1e-4 && (p[2] - 199.0).abs() < 1e-4, "{p:?}");
    }
}
