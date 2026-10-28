//! The GM's channel (CLIENT `viadmin.cpp`, `VIClientPart382E8` at
//! VIClient+0x382E8): commands are ids 0xA1xx0000 (`Show(id, x)`, and
//! `DoText(id, text)` for those that ask for a text first) and 0xA2xx0000
//! (`Do2`); each one is one 0x0701 message to the server, a sub-op byte and
//! its fields (0x0702 for `Do2`). The server turns the channel on with
//! 0x0700 sub-op 35 (`RecvEnabled`, a zigzag int: the start menu's TEST
//! entry while it is not 0, the commands while it is above 0) and asks a
//! question with sub-op 50 (`RecvPrompt`, a name: Yes is sub-op 48, No 49).
//! The three simulation fields feed `connection_set_simulated` after every
//! 0x40 (`ApplySimulation`).
//!
//! This is the channel's logic, with no window: what each command sends and
//! asks the client to do (`Out`), held to the decomp's writes by the tests.
//! `adminplay` ties it to `Play`. The auction query's stream operators at
//! the top of `viadmin.cpp` are `auction.rs`'s.

/// The guaranteed channel's message every command is.
pub const OP_ADMIN: u16 = 0x0701;
/// `Do2`'s message.
pub const OP_ADMIN2: u16 = 0x0702;
/// The server's message to the channel.
pub const OP_ADMIN_IN: u16 = 0x0700;

/// `IsCommand`: a 0xA1xx0000 id.
pub fn is_command(id: u32) -> bool {
    id & 0xFF00_0000 == 0xA100_0000
}

/// `IsCommand2`: a 0xA2xx0000 id.
pub fn is_command2(id: u32) -> bool {
    id & 0xFF00_0000 == 0xA200_0000
}

/// `VIClientPart382E8`'s fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Part {
    /// `m_loss`: the simulated packet loss, % (an `unsigned char`)
    pub loss: u8,
    /// `m_latency`
    pub latency: u16,
    /// `m_jitter`: nothing sets it (the VARIANCE entries set `m_loss`, as
    /// shipped)
    pub jitter: u16,
    /// `m_p0C`: `Toggle19` flips it, `Reset` clears it; while set the
    /// client draws `DrawNetStats`
    pub p0c: i32,
    /// `m_p10`
    pub p10: i32,
    /// `m_enabled`: `RecvEnabled`
    pub enabled: i32,
}

/// What a command asks of the client, in its order.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// a message for the server's guaranteed channel (251)
    Send(Vec<u8>),
    /// `OpenPanel27`: the Buff PC picker (`VIWndGame::OpenBuffs`, mode 27)
    Buffs,
    /// `ShowNameClear(id, word)`: the edit box titled with `word`, 256 long,
    /// empty, whose text comes back as `id`
    Edit(u32, i32),
    /// `ShowTargetPlayerName(id, word)`: the same, 24 long, holding the
    /// target's name when it is a player
    EditTarget(u32, i32),
    /// `ShowMsg(word)`: a word in the chat box
    Word(i32),
}

/// What 0x0700 brought (`Recv0700`).
#[derive(Clone, Debug, PartialEq)]
pub enum Recv {
    /// sub-op 35, `RecvEnabled`: `m_enabled`
    Enabled(i32),
    /// sub-op 50, `RecvPrompt`: `PromptWithName(name)`, a Yes/No question
    /// (word 1189, a space, the name) whose answer is 48 or 49
    Prompt(Vec<u16>),
}

/// A message's head: 0x0701 and the sub-op.
fn head(sub: u8) -> Vec<u8> {
    let mut m = OP_ADMIN.to_le_bytes().to_vec();
    m.push(sub);
    m
}

/// `SEND_NONE`: the sub-op alone.
pub fn send_none(sub: u8) -> Vec<u8> {
    head(sub)
}

/// `SEND_INT`: the sub-op and a raw word (`WriteU32`).
pub fn send_int(sub: u8, a: i32) -> Vec<u8> {
    let mut m = head(sub);
    m.extend((a as u32).to_le_bytes());
    m
}

/// `SEND_INT2`: two raw words.
pub fn send_int2(sub: u8, a: i32, b: i32) -> Vec<u8> {
    let mut m = send_int(sub, a);
    m.extend((b as u32).to_le_bytes());
    m
}

/// `SEND_STRING`: the 8-bit `WriteString`, a u32 length and the bytes.
pub fn send_string(sub: u8, s: &[u8]) -> Vec<u8> {
    let mut m = head(sub);
    m.extend((s.len() as u32).to_le_bytes());
    m.extend(s);
    m
}

/// `SendText13`: the wide `WriteString`, a u32 count of characters and
/// them, UTF-16LE.
pub fn send_text13(text: &[u16]) -> Vec<u8> {
    let mut m = head(13);
    m.extend((text.len() as u32).to_le_bytes());
    for c in text {
        m.extend(c.to_le_bytes());
    }
    m
}

/// `Send33(a, s)`: a raw word, then the string.
pub fn send33(a: i32, s: &[u8]) -> Vec<u8> {
    let mut m = send_int(33, a);
    m.extend((s.len() as u32).to_le_bytes());
    m.extend(s);
    m
}

/// `Send16` / `Send17`: nothing for the id -1.
pub fn send_id(sub: u8, id: u32) -> Option<Vec<u8>> {
    (id != u32::MAX).then(|| send_int(sub, id as i32))
}

/// `Do2(0xA2010000, x)`: 0x0702, 100 and the low byte of `x`.
pub fn do2(id: u32, x: i32) -> Option<Vec<u8>> {
    if id != 0xA201_0000 {
        return None;
    }
    let mut m = OP_ADMIN2.to_le_bytes().to_vec();
    m.push(100);
    m.push(x as u8);
    Some(m)
}

/// `VIString::char16to8`: a character past 255 is '?' (CLIENT's 1252 table
/// is never counted in, its count a static nothing sets).
pub fn char16to8(c: u16) -> u8 {
    if c < 256 { c as u8 } else { b'?' }
}

/// `VIString::str16to8` over the whole text (`DoText` sizes the buffer to
/// it), stopped at a NUL.
pub fn str16to8(text: &[u16]) -> Vec<u8> {
    text.iter().take_while(|&&c| c != 0).map(|&c| char16to8(c)).collect()
}

impl Part {
    /// `SendSimulation`: sub-op 18, the loss (a byte), the latency and the
    /// jitter (two bytes each).
    pub fn simulation(&self) -> Vec<u8> {
        let mut m = head(18);
        m.push(self.loss);
        m.extend(self.latency.to_le_bytes());
        m.extend(self.jitter.to_le_bytes());
        m
    }

    /// `IsEnabled() > 0`: the commands' test (the slash commands', the
    /// auction's minutes).
    pub fn commands(&self) -> bool {
        self.enabled > 0
    }

    /// `OpenStartMenu`'s: the TEST entry stays while `IsEnabled()` is not 0.
    pub fn test_entry(&self) -> bool {
        self.enabled != 0
    }

    /// `Reset` (from `ResetWorld`): `m_p0C` cleared. Nothing calls it here:
    /// a new `Play` a session starts clear.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.p0c = 0;
    }

    /// `Show(id, x)`: command `id` with the entry's number `x`, against the
    /// target and the player's own id.
    pub fn show(&mut self, id: u32, x: i32, target: u32, self_id: u32) -> Vec<Out> {
        let clamp = |hi: i32| x.max(0).min(hi);
        let t = target as i32;
        let send = |m: Vec<u8>| vec![Out::Send(m)];
        match id {
            0xA10B_0000 => {
                self.loss = clamp(50) as u8;
                send(self.simulation())
            }
            0xA10C_0000 => {
                self.latency = clamp(8192) as u16;
                send(self.simulation())
            }
            // VARIANCE: m_loss, not m_jitter, as shipped (and a byte)
            0xA10D_0000 => {
                self.loss = clamp(8192) as u8;
                send(self.simulation())
            }
            0xA101_0000 => send_id(16, target).map(Out::Send).into_iter().collect(),
            0xA102_0000 => send_id(16, self_id).map(Out::Send).into_iter().collect(),
            0xA103_0000 => send_id(17, target).map(Out::Send).into_iter().collect(),
            0xA104_0000 => send_id(17, self_id).map(Out::Send).into_iter().collect(),
            0xA105_0000 => send(send_none(14)),
            0xA10A_0000 => {
                self.p0c ^= 1;
                send(send_none(19))
            }
            0xA109_0000 => send(send_none(15)),
            0xA10E_0000 => send(send_none(1)),
            0xA10F_0000 => vec![Out::Buffs],
            0xA110_0000 => send(send_int2(2, t, x)),
            0xA111_0000 | 0xA125_0000 => vec![Out::Edit(id, 656)],
            0xA122_0000 => vec![Out::Edit(id, 741)],
            0xA112_0000 => vec![Out::Edit(id, 657)],
            0xA113_0000 => send(send_none(5)),
            0xA114_0000 => vec![Out::Edit(id, 660)],
            0xA115_0000 | 0xA116_0000 | 0xA123_0000 | 0xA124_0000 => vec![Out::Edit(id, 661)],
            0xA117_0000 => vec![Out::EditTarget(id, 689)],
            0xA118_0000 => vec![Out::EditTarget(id, 690)],
            0xA134_0000 => vec![Out::EditTarget(id, 1468)],
            0xA119_0000 => vec![Out::EditTarget(id, 691)],
            0xA11A_0000 => vec![Out::EditTarget(id, 692)],
            0xA120_0000 => vec![Out::EditTarget(id, 729)],
            0xA121_0000 => vec![Out::EditTarget(id, 728)],
            0xA108_0000 => vec![Out::Edit(id, 155)],
            0xA126_0000 => send(send_none(32)),
            0xA127_0000 => vec![Out::Edit(id, 1030)],
            0xA128_0000 => send(send_int(34, t)),
            0xA129_0000 => send(send_none(37)),
            0xA130_0000 => send(send_int(40, t)),
            0xA131_0000 => send(send_int(38, t)),
            0xA132_0000 => send(send_int(39, t)),
            0xA133_0000 => vec![Out::EditTarget(id, 1038)],
            0xA135_0000 => send(send_int(52, t)),
            _ => Vec::new(),
        }
    }

    /// `DoText(id, text)`: a command's text (the edit box's, a slash
    /// command's rest) to its message; a number that `atoi` reads as 0 is
    /// refused with a word.
    pub fn do_text(&self, id: u32, text: &[u16], target: u32) -> Vec<Out> {
        let buf = str16to8(text);
        let n = || crate::auction::atoi(&String::from_utf8_lossy(&buf));
        let t = target as i32;
        let num = |f: &dyn Fn(i32) -> Vec<u8>, refuse: i32| {
            let v = n();
            vec![if v != 0 { Out::Send(f(v)) } else { Out::Word(refuse) }]
        };
        let send = |m: Vec<u8>| vec![Out::Send(m)];
        match id {
            0xA108_0000 => send(send_text13(&text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())])),
            0xA117_0000 => send(send_string(9, &buf)),
            0xA118_0000 => send(send_string(10, &buf)),
            0xA134_0000 => send(send_string(51, &buf)),
            0xA119_0000 => send(send_string(11, &buf)),
            0xA11A_0000 => send(send_string(12, &buf)),
            0xA120_0000 => send(send_string(20, &buf)),
            0xA121_0000 => send(send_string(21, &buf)),
            0xA111_0000 => num(&|v| send_int(3, v), 816),
            0xA111_1000 => send(send_string(53, &buf)),
            0xA136_0000 => send(send_string(54, &buf)),
            0xA137_0000 => send(send_string(55, &buf)),
            0xA125_0000 => num(&|v| send_int2(25, t, v), 816),
            0xA122_0000 => num(&|v| send_int(22, v), 817),
            0xA112_0000 => send(send_string(4, &buf)),
            0xA114_0000 => send(send_string(6, &buf)),
            0xA115_0000 => num(&|v| send_int(7, v), 818),
            0xA116_0000 => num(&|v| send_int(8, v), 818),
            0xA123_0000 => num(&|v| send_int(23, v), 818),
            0xA124_0000 => num(&|v| send_int(24, v), 818),
            0xA127_0000 => {
                if buf.len() > 12 {
                    vec![Out::Word(1031)]
                } else {
                    send(send33(t, &buf))
                }
            }
            0xA133_0000 => send(send_string(41, &buf)),
            _ => Vec::new(),
        }
    }

    /// `Recv0700`: the sub-op, then 35's zigzag int (`m_enabled`) or 50's
    /// name (`ReadString(name, 64)`: 31 characters at most, a longer one
    /// skipped and read as empty). Another sub-op is read as nothing.
    pub fn recv(&mut self, data: &[u8]) -> Option<Recv> {
        let mut r = net::stream::Reader::new(data);
        match r.u8() {
            35 => {
                self.enabled = r.signed() as i32;
                Some(Recv::Enabled(self.enabled))
            }
            50 => Some(Recv::Prompt(r.wide(32).encode_utf16().collect())),
            _ => None,
        }
    }

    /// `Send0701a` (Yes to the prompt) and `Send0701b` (No): 48, 49.
    pub fn answer(yes: bool) -> Vec<u8> {
        send_none(if yes { 48 } else { 49 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The decomp's senders (`viadmin.cpp`'s macro table and the hand-written
    /// ones), transcribed: each id the `Show` / `DoText` switch sends from,
    /// its sub-op and what it writes after the sub-op.
    #[derive(Clone, Copy, Debug)]
    enum Field {
        None,
        Int(u32),
        Int2(u32, u32),
        Str(&'static [u8]),
    }

    /// The bytes as `VIMemStream` writes them, built here byte by byte: u16
    /// 0x0701 little-endian, the sub-op, then the fields (`WriteU32` raw,
    /// `WriteString` a u32 length and the bytes).
    fn wire(sub: u8, f: Field) -> Vec<u8> {
        let mut v = vec![0x01, 0x07, sub];
        let w = |v: &mut Vec<u8>, x: u32| v.extend([x as u8, (x >> 8) as u8, (x >> 16) as u8, (x >> 24) as u8]);
        match f {
            Field::None => {}
            Field::Int(a) => w(&mut v, a),
            Field::Int2(a, b) => {
                w(&mut v, a);
                w(&mut v, b);
            }
            Field::Str(s) => {
                w(&mut v, s.len() as u32);
                v.extend(s);
            }
        }
        v
    }

    const TARGET: u32 = 0x1234_5678;
    const ME: u32 = 0x00AB_CDEF;

    /// Every `Show` command that sends at once: the sub-op and the fields
    /// the decomp writes (`Send16(m_target)`, `Send02(m_target, x)`, ...).
    #[test]
    fn every_show_command_writes_what_the_decomp_writes() {
        let x = 37;
        let table: [(u32, u8, Field); 16] = [
            (0xA101_0000, 16, Field::Int(TARGET)),
            (0xA102_0000, 16, Field::Int(ME)),
            (0xA103_0000, 17, Field::Int(TARGET)),
            (0xA104_0000, 17, Field::Int(ME)),
            (0xA105_0000, 14, Field::None),
            (0xA10A_0000, 19, Field::None),
            (0xA109_0000, 15, Field::None),
            (0xA10E_0000, 1, Field::None),
            (0xA110_0000, 2, Field::Int2(TARGET, 37)),
            (0xA113_0000, 5, Field::None),
            (0xA126_0000, 32, Field::None),
            (0xA128_0000, 34, Field::Int(TARGET)),
            (0xA129_0000, 37, Field::None),
            (0xA130_0000, 40, Field::Int(TARGET)),
            (0xA131_0000, 38, Field::Int(TARGET)),
            (0xA132_0000, 39, Field::Int(TARGET)),
        ];
        for (id, sub, f) in table {
            let mut p = Part::default();
            assert_eq!(p.show(id, x, TARGET, ME), vec![Out::Send(wire(sub, f))], "{id:#x}");
        }
        let mut p = Part::default();
        assert_eq!(p.show(0xA135_0000, x, TARGET, ME), vec![Out::Send(wire(52, Field::Int(TARGET)))]);
        // the negative control: a sub-op one off, or the player's id where
        // the target's goes, is not what is written
        assert_ne!(p.show(0xA110_0000, x, TARGET, ME), vec![Out::Send(wire(3, Field::Int2(TARGET, 37)))]);
        assert_ne!(p.show(0xA101_0000, x, TARGET, ME), vec![Out::Send(wire(16, Field::Int(ME)))]);
        // Send16 / Send17 send nothing for the id -1; the others send it
        assert!(p.show(0xA101_0000, x, u32::MAX, ME).is_empty());
        assert!(p.show(0xA104_0000, x, TARGET, u32::MAX).is_empty());
        assert_eq!(p.show(0xA128_0000, x, u32::MAX, ME), vec![Out::Send(wire(34, Field::Int(u32::MAX)))]);
        // an id the switch has no case for does nothing
        assert!(p.show(0xA106_0000, x, TARGET, ME).is_empty());
        assert!(p.show(0xA1FF_0000, x, TARGET, ME).is_empty());
    }

    /// `Show`'s commands that ask for a text first: the edit box's title
    /// word, empty (`ShowNameClear`) or with the target player's name
    /// (`ShowTargetPlayerName`); and Buff PC's picker.
    #[test]
    fn the_commands_that_ask_first() {
        let mut p = Part::default();
        let mut ask = |id: u32| p.show(id, 0, TARGET, ME);
        for (id, word) in [(0xA111_0000, 656), (0xA125_0000, 656), (0xA122_0000, 741), (0xA112_0000, 657), (0xA114_0000, 660), (0xA115_0000, 661), (0xA116_0000, 661), (0xA123_0000, 661), (0xA124_0000, 661), (0xA108_0000, 155), (0xA127_0000, 1030)] {
            assert_eq!(ask(id), vec![Out::Edit(id, word)], "{id:#x}");
        }
        for (id, word) in [(0xA117_0000, 689), (0xA118_0000, 690), (0xA134_0000, 1468), (0xA119_0000, 691), (0xA11A_0000, 692), (0xA120_0000, 729), (0xA121_0000, 728), (0xA133_0000, 1038)] {
            assert_eq!(ask(id), vec![Out::EditTarget(id, word)], "{id:#x}");
        }
        assert_eq!(ask(0xA10F_0000), vec![Out::Buffs]);
        // the control: ShowNameClear and ShowTargetPlayerName are not one
        assert_ne!(ask(0xA117_0000), vec![Out::Edit(0xA117_0000, 689)]);
    }

    /// `DoText`: the text as `str16to8` leaves it, under each sub-op; the
    /// numbers through `atoi`, 0 refused with its word; Change Name's limit.
    #[test]
    fn every_text_command_writes_what_the_decomp_writes() {
        let p = Part::default();
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let name = w("Ltcomdata");
        for (id, sub) in [(0xA117_0000, 9), (0xA118_0000, 10), (0xA134_0000, 51), (0xA119_0000, 11), (0xA11A_0000, 12), (0xA120_0000, 20), (0xA121_0000, 21), (0xA111_1000, 53), (0xA136_0000, 54), (0xA137_0000, 55), (0xA112_0000, 4), (0xA114_0000, 6), (0xA133_0000, 41)] {
            assert_eq!(p.do_text(id, &name, TARGET), vec![Out::Send(wire(sub, Field::Str(b"Ltcomdata")))], "{id:#x}");
        }
        let n = w(" 42 apples");
        for (id, sub, f) in [(0xA111_0000u32, 3u8, Field::Int(42)), (0xA125_0000, 25, Field::Int2(TARGET, 42)), (0xA122_0000, 22, Field::Int(42)), (0xA115_0000, 7, Field::Int(42)), (0xA116_0000, 8, Field::Int(42)), (0xA123_0000, 23, Field::Int(42)), (0xA124_0000, 24, Field::Int(42))] {
            assert_eq!(p.do_text(id, &n, TARGET), vec![Out::Send(wire(sub, f))], "{id:#x}");
        }
        for (id, word) in [(0xA111_0000, 816), (0xA125_0000, 816), (0xA122_0000, 817), (0xA115_0000, 818), (0xA116_0000, 818), (0xA123_0000, 818), (0xA124_0000, 818)] {
            assert_eq!(p.do_text(id, &w("none"), TARGET), vec![Out::Word(word)], "{id:#x}: atoi 0");
        }
        // -7 is not 0: it is sent
        assert_eq!(p.do_text(0xA111_0000, &w("-7"), TARGET), vec![Out::Send(wire(3, Field::Int(-7i32 as u32)))]);
        // Change Name: twelve characters go, thirteen are refused (1031)
        assert_eq!(p.do_text(0xA127_0000, &w("Twelve_chars"), TARGET), vec![Out::Send({
            let mut v = wire(33, Field::Int(TARGET));
            v.extend([12, 0, 0, 0]);
            v.extend(b"Twelve_chars");
            v
        })]);
        assert_eq!(p.do_text(0xA127_0000, &w("Thirteen_char"), TARGET), vec![Out::Word(1031)]);
        // BROADCAST: the wide text, its characters counted
        assert_eq!(p.do_text(0xA108_0000, &w("Hi\u{263A}"), TARGET), vec![Out::Send(vec![1, 7, 13, 3, 0, 0, 0, b'H', 0, b'i', 0, 0x3A, 0x26])]);
        // a wide character past 255 is '?' in an 8-bit one
        assert_eq!(p.do_text(0xA117_0000, &w("a\u{263A}b"), TARGET), vec![Out::Send(wire(9, Field::Str(b"a?b")))]);
        // the control: an id DoText has no case for sends nothing
        assert!(p.do_text(0xA113_0000, &name, TARGET).is_empty());
        assert_ne!(p.do_text(0xA117_0000, &name, TARGET), vec![Out::Send(wire(10, Field::Str(b"Ltcomdata")))]);
    }

    /// The simulation's three entries: loss clamped to 0..50, latency to
    /// 0..8192, VARIANCE into `m_loss` (as shipped) through its byte; each
    /// sends sub-op 18 with the three fields.
    #[test]
    fn the_network_simulation() {
        let mut p = Part::default();
        assert_eq!(p.show(0xA10B_0000, 25, TARGET, ME), vec![Out::Send(vec![1, 7, 18, 25, 0, 0, 0, 0])]);
        assert_eq!(p.show(0xA10B_0000, 99, TARGET, ME), vec![Out::Send(vec![1, 7, 18, 50, 0, 0, 0, 0])], "50 at most");
        assert_eq!(p.show(0xA10B_0000, -3, TARGET, ME), vec![Out::Send(vec![1, 7, 18, 0, 0, 0, 0, 0])], "0 at least");
        assert_eq!(p.show(0xA10C_0000, 1000, TARGET, ME), vec![Out::Send(vec![1, 7, 18, 0, 0xE8, 0x03, 0, 0])]);
        assert_eq!(p.show(0xA10C_0000, 9000, TARGET, ME), vec![Out::Send(vec![1, 7, 18, 0, 0x00, 0x20, 0, 0])], "8192 at most");
        assert_eq!((p.loss, p.latency, p.jitter), (0, 8192, 0));
        // VARIANCE 1000: m_loss = 1000 as a byte (232), the jitter untouched
        assert_eq!(p.show(0xA10D_0000, 1000, TARGET, ME), vec![Out::Send(vec![1, 7, 18, 232, 0x00, 0x20, 0, 0])]);
        assert_eq!((p.loss, p.jitter), (232, 0));
        assert_ne!(p.simulation(), vec![1, 7, 18, 0, 0x00, 0x20, 0xE8, 0x03], "the control: not into the jitter");
    }

    /// `Toggle19` flips `m_p0C` (the client's `DrawNetStats`), `Reset`
    /// clears it.
    #[test]
    fn server_load_toggles_and_reset_clears() {
        let mut p = Part::default();
        p.show(0xA10A_0000, 0, TARGET, ME);
        assert_eq!(p.p0c, 1);
        p.show(0xA10A_0000, 0, TARGET, ME);
        assert_eq!(p.p0c, 0);
        p.show(0xA10A_0000, 0, TARGET, ME);
        p.reset();
        assert_eq!(p.p0c, 0);
    }

    /// `Do2`: TRIGGER's eight entries, 0x0702 sub-op 100 and the byte.
    #[test]
    fn trigger_is_0x0702() {
        assert_eq!(do2(0xA201_0000, 7), Some(vec![0x02, 0x07, 100, 7]));
        assert_eq!(do2(0xA201_0000, 0x1FF), Some(vec![0x02, 0x07, 100, 0xFF]), "the low byte");
        assert_eq!(do2(0xA202_0000, 7), None, "only 0xA2010000");
        assert!(is_command2(0xA201_0000) && !is_command(0xA201_0000));
        assert!(is_command(0xA111_1000) && !is_command(0xB051_0000) && !is_command2(0xA111_0000));
    }

    /// `Recv0700`: 35's zigzag int, 50's name of at most 31 characters, any
    /// other sub-op nothing; the menu's test and the commands' differ on a
    /// negative value.
    #[test]
    fn recv_0700() {
        let mut p = Part::default();
        assert_eq!(p.recv(&[35, 2]), Some(Recv::Enabled(1)), "zigzag 2 is 1");
        assert!(p.commands() && p.test_entry());
        assert_eq!(p.recv(&[35, 1]), Some(Recv::Enabled(-1)), "zigzag 1 is -1");
        assert!(!p.commands() && p.test_entry(), "TEST shown, the commands refused");
        assert_eq!(p.recv(&[35, 0]), Some(Recv::Enabled(0)));
        assert!(!p.commands() && !p.test_entry());
        let mut m = vec![50, 3, 0, 0, 0];
        m.extend("Zak".encode_utf16().flat_map(|c| c.to_le_bytes()));
        assert_eq!(p.recv(&m), Some(Recv::Prompt("Zak".encode_utf16().collect())));
        let long: String = "x".repeat(32);
        let mut m = vec![50, 32, 0, 0, 0];
        m.extend(long.encode_utf16().flat_map(|c| c.to_le_bytes()));
        assert_eq!(p.recv(&m), Some(Recv::Prompt(Vec::new())), "32 characters do not fit name[32]");
        assert_eq!(p.recv(&[36, 2]), None);
        assert_eq!(p.enabled, 0, "an unknown sub-op leaves the flag");
        assert_eq!(Part::answer(true), vec![1, 7, 48]);
        assert_eq!(Part::answer(false), vec![1, 7, 49]);
    }
}
