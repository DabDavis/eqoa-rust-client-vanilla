//! SOE's live client's parts in play (`live`): its server feature word
//! (0x07F5). A child of `play`, for `Play`'s fields.

use super::Play;

impl Play {
    /// The live client's messages: 0x07F5, the feature word (`Msg07F5`, live
    /// 0x012C9548: the u32 into VIClient+0x813C), kept for every window
    /// that reads it (`live::features`).
    pub(super) fn live_opcode(&mut self, op: u16, data: &[u8]) {
        if op == 0x07F5
            && let Some(w) = crate::live::Features::parse(data)
        {
            if crate::live::features() != crate::live::Features(Some(w)) {
                eprintln!("live: the server's feature word {w:#x} (0x07F5)");
            }
            crate::live::set_features(crate::live::Features(Some(w)));
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::live::{Features, features};

    /// A session that sends nowhere and records what it would (as
    /// `adminplay`'s tests).
    pub(in crate::play) fn play() -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = net::session::Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        s.record = Some(Vec::new());
        Play::new(s, Default::default(), Box::default())
    }

    /// A new session has no word (the disc's rules), 0x07F5 sets it, a
    /// short one is nothing, and the next session starts without.
    #[test]
    fn msg_07f5_keeps_the_word_for_the_session() {
        let mut p = play();
        assert_eq!(features(), Features(None));
        p.live_opcode(0x07F5, &0x1Bu32.to_le_bytes());
        assert_eq!(features(), Features(Some(0x1B)));
        p.live_opcode(0x07F5, &[8, 0]);
        assert_eq!(features(), Features(Some(0x1B)), "a short 0x07F5 is nothing");
        p.live_opcode(0x07F5, &0x10u32.to_le_bytes());
        assert_eq!(features(), Features(Some(0x10)));
        let _next = play();
        assert_eq!(features(), Features(None), "a new session is the disc's until a word");
    }
}
