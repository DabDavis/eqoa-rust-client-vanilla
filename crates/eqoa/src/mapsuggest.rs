//! A community marker suggested from the map (`map::community`): the map's L3
//! (N on a keyboard, its Suggest button) asks for a spot (`Map::take_suggest`),
//! the map goes down and the game's edit box asks the marker's name, then a
//! note on it (an empty Enter leaves it out); the suggestion goes to the
//! community's list for a GM's approval and the map comes back where it was,
//! saying what the list answered. Escape at either step sends nothing; a name
//! of fewer than two letters is refused here, as the list would refuse it.

use super::Play;
use crate::map::community::{self, Draft};

/// The edit box's titles for the two steps (short: the title's tab is the
/// box's width, and a longer one ran off it).
const NAME_TITLE: &str = "Name a marker for everyone's map";
const NOTE_TITLE: &str = "A note on it, or none: Enter sends it";
/// What the map says of a name too short to send.
const TOO_SHORT: &str = "A marker needs a name of 2 to 40 letters: nothing was sent.";

impl Play {
    /// The map's spot to suggest, if it asked for one: the map put away, the
    /// edit box up for the marker's name; the spot the target's (F3
    /// 20261026-231048: a merchant, a quest giver), its name already typed
    /// there, as the game's own boxes take the target's name (`open_edit`),
    /// so the player sees whose spot goes.
    pub(super) fn take_map_suggest(&mut self) {
        let Some(spot) = self.map.take_suggest() else { return };
        let target = self.map.take_suggest_target();
        let follow = self.map.following();
        self.map.close();
        self.map.community.draft = Some(Draft { spot, name: None, follow });
        self.open_edit(community::NAME_PURPOSE, community::NAME_MAX, -1, target);
        self.edit.title = crate::ui::wide(NAME_TITLE);
    }

    /// The edit box went down for a suggestion's name or note (`done`: Done
    /// with its text, else cancelled): true when it was one of theirs.
    pub(super) fn map_edit_done(&mut self, purpose: u32, done: bool) -> bool {
        if purpose != community::NAME_PURPOSE && purpose != community::NOTE_PURPOSE {
            return false;
        }
        let text = String::from_utf16_lossy(&std::mem::take(&mut self.edit.text));
        self.edit.title.clear();
        self.mode = 0;
        let Some(mut draft) = self.map.community.draft.take() else { return true };
        let follow = draft.follow;
        if !done {
            self.map.resume(follow);
            return true;
        }
        if purpose == community::NAME_PURPOSE {
            let name = community::clip(&text, community::NAME_MAX);
            if name.chars().count() < community::NAME_MIN {
                self.map.community.notice(TOO_SHORT);
                self.map.resume(follow);
                return true;
            }
            draft.name = Some(name);
            self.map.community.draft = Some(draft);
            self.open_edit(community::NOTE_PURPOSE, community::NOTE_MAX, -1, false);
            self.edit.title = crate::ui::wide(NOTE_TITLE);
            return true;
        }
        if let Some(name) = draft.name.take() {
            self.map.community.send(draft.spot, &name, &text);
        }
        self.map.resume(follow);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::community::testing::{Request, serve, until};
    use crate::pointer::key;
    use crate::pointer::testing::{NullRaster, vui};
    use net::session::{Player, Session};

    /// Play on a session nothing answers, its map up over Tunaria's layout
    /// (`map::testing::up`) and its community list at `base`.
    fn play(base: &str) -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        let mut p = Play::new(s, Player::default(), Box::default());
        p.map = crate::map::testing::up((1280, 720));
        p.map.community.set_base(Some(base.to_string()));
        // a cache of its own, empty (none of the machine's)
        p.map.community.cache = Some(Box::new(crate::map::store::Dir::new(std::env::temp_dir().join(format!("eqoa-mapsuggest-{}-{}", std::process::id(), base.len())))));
        p
    }

    /// The edit box's keys as the game hands them (`edit_key`, `run_edit`):
    /// the text typed, then Enter (13) or Escape (27).
    fn type_in(p: &mut Play, text: &str, enter: bool) {
        p.edit.set_text(&crate::ui::wide(text));
        p.edit_key(if enter { 13 } else { 27 }, true);
        p.run_edit(&vui(), &NullRaster);
    }

    fn suggestions(asked: &std::sync::Mutex<Vec<Request>>) -> Vec<Request> {
        asked.lock().unwrap().iter().filter(|r| r.method == "POST").cloned().collect()
    }

    /// N on the map (L3) with a waypoint set: the map goes down for the
    /// edit box's name, then its note; Enter sends both with the waypoint's
    /// spot and this build to the list, and the map comes back where it was,
    /// saying the list took it. The control: before Enter on the note nothing
    /// is sent.
    #[test]
    fn a_marker_is_suggested_through_the_edit_box() {
        crate::settings::testing::reset(false);
        let (base, asked) = serve(|r| match r.path.as_str() {
            "/mapmarks/suggest" => (200, Vec::new(), r#"{"ok":true,"id":12}"#.into()),
            _ => (200, Vec::new(), r#"{"version":"1","marks":[]}"#.into()),
        });
        let mut p = play(&base);
        // a waypoint on an empty corner of the map
        assert!(p.map.click([40.0, 680.0], false));
        let spot = p.map.waypoint(0).expect("the waypoint");
        p.map.follow_for_test(false);
        assert!(p.map.key(key::L3, true));
        p.take_map_suggest();
        assert_eq!((p.mode, p.edit.purpose, p.map.open), (2, community::NAME_PURPOSE, false), "the edit box up for the name, the map down");
        assert_eq!(String::from_utf16_lossy(&p.edit.title), NAME_TITLE);
        type_in(&mut p, "  Hidden cave ", true);
        assert_eq!((p.mode, p.edit.purpose), (2, community::NOTE_PURPOSE), "then the note");
        assert!(suggestions(&asked).is_empty(), "the control: nothing sent yet");
        type_in(&mut p, "Behind the falls", true);
        assert_eq!(p.mode, 0);
        assert!(p.map.open && !p.map.following(), "the map back where it was");
        assert!(until(5.0, || {
            p.map.community.frame(true);
            p.map.community.notice_now() == Some(community::SENT)
        }));
        let sent = suggestions(&asked);
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].path.as_str(), sent[0].header("content-type")), ("/mapmarks/suggest", Some("application/json")));
        let v: serde_json::Value = serde_json::from_str(&sent[0].body).unwrap();
        assert_eq!((v["world"].as_i64(), v["name"].as_str(), v["note"].as_str()), (Some(0), Some("Hidden cave"), Some("Behind the falls")));
        assert!((v["x"].as_f64().unwrap() as f32 - spot[0]).abs() < 0.01 && (v["z"].as_f64().unwrap() as f32 - spot[1]).abs() < 0.01, "the waypoint's spot: {v}");
        assert_eq!(v["client"].as_str(), Some(community::client().as_str()));
    }

    /// N with a merchant targeted and the map following the player (F3
    /// 20261026-231048, Liav's): the merchant's spot is suggested over a
    /// waypoint from before (the map opened since), its name already typed in
    /// the box, and Enter sends it as it stands. The control: the map panned,
    /// the pad's cross is the spot and the box starts empty.
    #[test]
    fn the_target_is_suggested_with_its_name() {
        crate::settings::testing::reset(false);
        let (base, asked) = serve(|r| match r.path.as_str() {
            "/mapmarks/suggest" => (200, Vec::new(), r#"{"ok":true,"id":14}"#.into()),
            _ => (200, Vec::new(), r#"{"version":"1","marks":[]}"#.into()),
        });
        let mut p = play(&base);
        // a waypoint from before: put, the map put away and opened again
        assert!(p.map.click([40.0, 680.0], false));
        p.map.close();
        p.map.show();
        let merchant = [25475.9, 15704.6];
        let creature = net::entity::Creature { name: "Merchant Ravnous".into(), ..Default::default() };
        p.entities.slots[0] = Some(net::entity::Entity { id: 0xEA01, creature: Some(creature), ..Default::default() });
        p.target = 0xEA01;
        p.map.target_for_test(Some(merchant));
        assert!(p.map.key(key::L3, true));
        p.take_map_suggest();
        assert_eq!(p.map.community.draft.as_ref().map(|d| d.spot.at), Some(merchant), "the merchant's spot, not the old waypoint's");
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "Merchant Ravnous", "its name typed");
        p.edit_key(13, true);
        p.run_edit(&vui(), &NullRaster);
        type_in(&mut p, "", true);
        assert!(until(5.0, || {
            p.map.community.frame(true);
            p.map.community.notice_now() == Some(community::SENT)
        }));
        let sent = suggestions(&asked);
        let v: serde_json::Value = serde_json::from_str(&sent[0].body).unwrap();
        assert_eq!(v["name"].as_str(), Some("Merchant Ravnous"));
        assert!((v["x"].as_f64().unwrap() as f32 - merchant[0]).abs() < 0.01 && (v["z"].as_f64().unwrap() as f32 - merchant[1]).abs() < 0.01, "{v}");
        // the control: panned, the pad's cross; nothing typed
        p.map.follow_for_test(false);
        assert!(p.map.key(key::L3, true));
        p.take_map_suggest();
        let spot = p.map.community.draft.as_ref().map(|d| d.spot.at).unwrap();
        assert!((spot[0] - merchant[0]).abs() > 1.0 || (spot[1] - merchant[1]).abs() > 1.0, "the cross, not the merchant: {spot:?}");
        assert!(p.edit.text.is_empty(), "nothing typed");
    }

    /// Escape at the name, or at the note, sends nothing and brings the map
    /// back; a name of one letter is refused with the map saying so; the
    /// list's refusal is shown in its own words. The control: an empty note
    /// still sends.
    #[test]
    fn escape_sends_nothing_and_a_refusal_is_shown() {
        crate::settings::testing::reset(false);
        let (base, asked) = serve(|r| match (r.path.as_str(), r.body.contains("Taken")) {
            ("/mapmarks/suggest", true) => (400, Vec::new(), r#"{"ok":false,"error":"That spot has a marker already."}"#.into()),
            ("/mapmarks/suggest", false) => (200, Vec::new(), r#"{"ok":true,"id":13}"#.into()),
            _ => (200, Vec::new(), r#"{"version":"1","marks":[]}"#.into()),
        });
        let mut p = play(&base);
        let start = |p: &mut Play| {
            assert!(p.map.key(key::L3, true));
            p.take_map_suggest();
            assert_eq!(p.mode, 2);
        };
        start(&mut p);
        type_in(&mut p, "Hidden cave", false);
        assert!((p.mode == 0 && p.map.open && p.map.community.draft.is_none()), "Escape at the name: the map back");
        start(&mut p);
        type_in(&mut p, "Hidden cave", true);
        type_in(&mut p, "a note", false);
        assert!(p.mode == 0 && p.map.open, "Escape at the note: the map back");
        start(&mut p);
        type_in(&mut p, "A", true);
        assert_eq!((p.mode, p.map.community.notice_now()), (0, Some(TOO_SHORT)), "one letter: refused here");
        std::thread::sleep(std::time::Duration::from_millis(200));
        p.map.community.frame(true);
        assert!(suggestions(&asked).is_empty(), "nothing sent of any");
        // the list's refusal in its words
        start(&mut p);
        type_in(&mut p, "Taken", true);
        type_in(&mut p, "", true);
        assert!(until(5.0, || {
            p.map.community.frame(true);
            p.map.community.notice_now() == Some("That spot has a marker already.")
        }));
        // the control: an empty note sends
        start(&mut p);
        type_in(&mut p, "Quiet glade", true);
        type_in(&mut p, "", true);
        assert!(until(5.0, || {
            p.map.community.frame(true);
            p.map.community.notice_now() == Some(community::SENT)
        }));
        let sent = suggestions(&asked);
        assert_eq!(sent.len(), 2, "the refused one and the last");
        assert!(sent[1].body.contains("\"note\":\"\""), "{}", sent[1].body);
    }
}
