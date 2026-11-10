//! Item links in play (`itemlink`; the GM Jibrish's petition 1463): making
//! one from the inventory into the chat line, the line kept with its links, the
//! walk through the chat box's links with its history held, the windows, and
//! the server told the Extended row Item Links. The chat line's own sending is
//! `Play::chat_line_linked`.

use super::Play;
use crate::editbox::TextLink;
use net::itemlink::{LINKS_MOST, Linked};

impl Play {
    /// Links can be made here: the server offered them and Item Links is on.
    pub(crate) fn links_made(&self) -> bool {
        crate::settings::item_links() && self.session.item_links_offer
    }

    /// Each frame: the server told what Item Links says (`agree_item_links`);
    /// off, the windows closed and the kept line's links let go; and the Chat
    /// page's note where the server has none.
    pub(super) fn agree_item_links(&mut self) {
        let on = crate::settings::item_links();
        self.session.agree_item_links(on);
        if !on {
            self.links.close_all();
            self.link_draft = None;
        }
        let note = (!self.session.item_links_offer).then_some("This server has no item links");
        if self.options_wnd.links_note.as_deref() != note {
            self.options_wnd.links_note = note.map(str::to_string);
        }
    }

    /// Link `id` opened, `name` its name as the line showed it: its window,
    /// the server asked for its item when it is not known (`itemlink::Links`).
    pub(super) fn open_link(&mut self, id: u32, name: &str) {
        if !crate::settings::item_links() {
            return;
        }
        if let Some(ask) = self.links.open(id, name) {
            self.session.ask_item_link(ask);
        }
        self.cues.push(crate::sound::Cue::Ui(13));
    }

    /// Escape or △ (the game's Exit; ○ in play is the HUD's Use): the newest
    /// link window closed; false with none open.
    pub fn close_link_window(&mut self) -> bool {
        let closed = self.links.close_newest();
        if closed {
            self.cues.push(crate::sound::Cue::Ui(8));
        }
        closed
    }

    /// A key while the chat box's history is held (mode 33): left (the
    /// keyboard's too) walks up through its links, right back down while one is
    /// lit, ✕ (Enter) opens the lit one, △ (Escape) closes the newest window.
    /// True when it was the walk's.
    pub(super) fn link_key(&mut self, key: i32) -> bool {
        if !crate::settings::item_links() {
            return false;
        }
        let Some(c) = self.wnd_chat.as_mut() else { return false };
        match key {
            136 | 17 => {
                c.walk_older();
                true
            }
            137 | 18 if c.walking() => {
                c.walk_newer();
                true
            }
            134 | 13 if c.walking() => {
                if let Some((id, name)) = c.walked() {
                    self.open_link(id, &name);
                }
                true
            }
            132 | 27 if self.links.is_open() => self.close_link_window(),
            _ => false,
        }
    }

    /// The history is held where the walk through the links can go (the
    /// keyboard's left is its there).
    pub fn links_walkable(&self) -> bool {
        self.mode == 33 && crate::settings::item_links()
    }

    /// A link is lit by the walk (the keyboard's right and Enter are its).
    pub fn link_walking(&self) -> bool {
        self.mode == 33 && self.wnd_chat.as_ref().is_some_and(|c| c.walking())
    }

    /// Where in a `size` window the `k`th newest item link the chat box drew is
    /// (its middle, in the window's pixels): `chat_point` undone. For a
    /// headless run's `linkclick`.
    pub fn link_on_screen(&self, size: (u32, u32), k: usize) -> Option<(f64, f64)> {
        let p = self.wnd_chat.as_ref()?.newest_link_at(k)?;
        let w = render::ui::window_from_frame(size, [p[0] + render::ui::side(size), p[1] + render::ui::top(size)]);
        Some((w[0] as f64, w[1] as f64))
    }

    /// The chat line as R3 and Enter open it (the channel's title), holding the
    /// line kept with its links where there is one.
    pub(super) fn open_chat_line(&mut self) {
        self.open_edit(0xB0F0000, 128, -1, false);
        self.edit.title = self.chat_title();
        if let Some((text, links)) = self.link_draft.clone() {
            self.edit.set_linked(&text, &links);
        }
    }

    /// The chat line given up: kept while it holds a link, until it is sent or
    /// emptied (the petition's "until they hit enter or clear out the text").
    pub(super) fn keep_link_draft(&mut self) {
        self.link_draft = (!self.edit.links.is_empty()).then(|| (self.edit.text.clone(), self.edit.links.clone()));
    }

    /// Item `i` of the player's list `list` (0 the pack, 1 the bank) linked: in
    /// the chat line at its caret where it is open, else the start menu closed
    /// and the chat line opened (with the line kept so far) and the item's name
    /// in it at its end; as a link either way. False, sound 9, where it does not
    /// go in (no room, four links already); nothing at all where links are not
    /// made.
    pub fn link_item(&mut self, list: u8, i: i32) -> bool {
        if !self.links_made() || i < 0 {
            return false;
        }
        let items = if list == 0 { &self.dump.inventory.items } else { &self.dump.bank.items.items };
        let Some(it) = items.get(i as usize) else { return false };
        // the name as a console's keyboard could type it (the server writes
        // its own in its place)
        let name: String = it.item.name.chars().filter(|&c| crate::editbox::typable(c)).collect();
        let (name, key) = (crate::ui::wide(name.trim()), it.inv_index);
        if self.mode != 2 || self.edit.purpose != 0x0B0F_0000 {
            // out of the bank's window as its Done takes it out, the chat box
            // let go of the bank's fixed place
            if self.mode == 11 {
                self.bank_close();
                if let Some(c) = self.wnd_chat.as_mut() {
                    c.mode = 0;
                }
            }
            self.set_mode_zero();
            self.open_chat_line();
        }
        let now = self.session.now() as i64;
        let ok = self.edit.insert_link(&name, list, key, now);
        self.cues.push(crate::sound::Cue::Ui(if ok { 13 } else { 9 }));
        ok
    }

    /// A typed line's links as they go (`net::itemlink::Linked`): those in it
    /// from `from` on, each one's place counted from there and moved to `at` in
    /// the line that goes; at most `LINKS_MOST`.
    pub(super) fn wire_links(wide: &[u16], links: &[TextLink], from: usize, at: usize) -> Vec<Linked> {
        links
            .iter()
            .filter(|l| l.start >= from && l.len > 0 && l.start + l.len <= wide.len())
            .take(LINKS_MOST)
            .map(|l| Linked { start: (l.start - from + at) as u16, len: l.len as u16, list: l.list, key: l.key })
            .collect()
    }

    /// The link windows, over the world in play and the chat box's history
    /// (none with the UI hidden): each its item's lines (`infotext`'s
    /// `link_item`) and icon.
    /// `side` is the frame pixels the window shows beyond each side (the
    /// windows' band widens by it).
    pub(super) fn run_links(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, side: i32) {
        if !self.links.is_open() || !matches!(self.mode, 0 | 33 | 38) || crate::hideui::hidden() {
            return;
        }
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let info = crate::infotext::Info {
            words: &vui.words,
            inv: &self.dump.inventory,
            hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
            base: &self.stats.base,
            class: pl.class,
            level: pl.level,
            attack_words: &attack_words,
        };
        let flags = net::session::EXPANSION_FLAGS;
        self.links.draw(vui, r, side, &|it, fits| info.link_item(it, fits), &|it| crate::inventory::item_icon(it, flags) as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::{Player, Session};

    /// Play on a session that offered item links (`linked`) or not, recording
    /// what it sends, its pack holding a sword and a gem and its bank a helm.
    fn play(linked: bool) -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        s.record = Some(Vec::new());
        s.item_links_offer = linked;
        let mut dump = Box::<net::dump::MemoryDump>::default();
        let item = |name: &str, key: u32| {
            let mut it = net::dump::NetItem { inv_index: key, ..Default::default() };
            it.item.name = name.to_string();
            it
        };
        dump.inventory.items = vec![item("Rusty Sword", 3), item("Star Rose Quartz", 9)];
        dump.bank.items.items = vec![item("Helm of `Iron", 5)];
        let mut p = Play::new(s, Player::default(), dump);
        p.agree_item_links();
        p
    }

    /// The messages sent since the last look (each its bytes, its opcode first).
    fn taken(p: &mut Play) -> Vec<Vec<u8>> {
        std::mem::take(p.session.record.as_mut().unwrap()).into_iter().map(|(_, m)| m).collect()
    }

    /// The links of a 0x7E50 message: (start, len, list, key) each, after its
    /// route, target and text.
    fn wire(m: &[u8]) -> (u8, String, Vec<(u16, u16, u8, u32)>) {
        let mut r = net::stream::Reader::new(&m[2..]);
        let route = r.u8();
        r.u32();
        let text = r.wide(1024);
        let n = r.u8() as usize;
        let links = (0..n).map(|_| (r.u16(), r.u16(), r.u8(), r.u32())).collect();
        assert!(!r.error);
        (route, text, links)
    }

    /// Link Item (or a Shift+click) puts the item's name in the chat line as a
    /// link (its list and key); a second one from the bank goes after it (its
    /// name as a console could type it); Enter sends the line as 0x7E50 in
    /// place of 0x000B, each link where its name is. The control: on a server
    /// that made no offer nothing is linked, and the same words go as 0x000B.
    #[test]
    fn a_linked_line_goes_in_place_of_the_retail_one() {
        let mut p = play(true);
        taken(&mut p);
        p.mode = 13;
        assert!(p.link_item(0, 0));
        assert_eq!(p.mode, 2, "the start menu closed, the chat line open");
        p.edit.paste(" and", 0);
        // given up (Cancel), the line is kept with its link; the bank's helm
        // linked from the inventory opens it again with it
        p.keep_link_draft();
        p.mode = 13;
        assert!(p.link_item(1, 0));
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "Rusty Sword and Helm of Iron");
        // with the chat line open a link goes in at its caret
        p.edit.paste(" or", 0);
        assert!(p.link_item(0, 1));
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "Rusty Sword and Helm of Iron or Star Rose Quartz");
        p.edit.backspace(0);
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "Rusty Sword and Helm of Iron or ", "taken whole, the space before it kept");
        for _ in 0..4 {
            p.edit.backspace(0);
        }
        p.edit_done(0x0B0F_0000);
        let sent = taken(&mut p);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(&sent[0][..2], &[0x50, 0x7E]);
        assert_eq!(wire(&sent[0]), (0, "Rusty Sword and Helm of Iron".into(), vec![(0, 11, 0, 3), (16, 12, 1, 5)]));
        assert!(p.link_draft.is_none(), "sent: nothing kept");
        // the control: no offer, no link; the words as ever
        let mut p = play(false);
        taken(&mut p);
        assert!(!p.link_item(0, 0) && p.mode != 2);
        p.chat_line_linked("Rusty Sword", &[TextLink { start: 0, len: 11, list: 0, key: 3 }]);
        let sent = taken(&mut p);
        assert_eq!(sent.len(), 1);
        assert_eq!(&sent[0][..2], &[0x0B, 0x00], "0x000B");
    }

    /// A slash command's word is not sent: its links count from after it; /s
    /// is a shout's route, /gu the guild's, /t goes whole. The control: an
    /// unknown command goes plain.
    #[test]
    fn a_commands_links_count_from_what_it_sends() {
        let mut p = play(true);
        taken(&mut p);
        let sword = |at: usize| [TextLink { start: at, len: 11, list: 0, key: 3 }];
        let first = |p: &mut Play| taken(p).into_iter().find(|m| m[1] == 0x7E).unwrap();
        p.chat_line_linked("/g see Rusty Sword", &sword(7));
        assert_eq!(wire(&first(&mut p)), (0, "see Rusty Sword".into(), vec![(4, 11, 0, 3)]));
        p.chat_line_linked("/s Rusty Sword", &sword(3));
        assert_eq!(wire(&first(&mut p)).0, 1, "a shout");
        p.chat_line_linked("/gu Rusty Sword", &sword(4));
        assert_eq!(wire(&first(&mut p)).0, 2, "the guild's");
        p.chat_line_linked("/t Bob Rusty Sword", &sword(7));
        assert_eq!(wire(&first(&mut p)), (0, "/t Bob Rusty Sword".into(), vec![(7, 11, 0, 3)]));
        p.reply_name = "Annabel".into();
        p.chat_line_linked("/r Rusty Sword", &sword(3));
        assert_eq!(wire(&first(&mut p)), (0, "/t Annabel Rusty Sword".into(), vec![(11, 11, 0, 3)]));
        p.chat_line_linked("/bug Rusty Sword", &sword(5));
        assert!(taken(&mut p).iter().all(|m| m[1] != 0x7E || m[0] == 0x0F), "a command: the plain line");
    }

    /// The line given up with a link in it is kept and comes back at the next
    /// plain open; emptied and given up, it is gone. The control: a line with no
    /// link is not kept.
    #[test]
    fn a_line_with_a_link_is_kept_until_sent_or_emptied() {
        let mut p = play(true);
        p.link_item(0, 0);
        p.keep_link_draft();
        p.mode = 0;
        p.chat_key("", true);
        assert_eq!(String::from_utf16_lossy(&p.edit.text), "Rusty Sword");
        assert_eq!(p.edit.links.len(), 1, "the link with it");
        p.edit.clear();
        p.keep_link_draft();
        assert!(p.link_draft.is_none(), "emptied: gone");
        p.chat_key("", true);
        p.edit.paste("hello", 0);
        p.keep_link_draft();
        assert!(p.link_draft.is_none(), "the control: no link, not kept");
    }

    /// A line the server sent with links: the next chat line read takes them,
    /// and only that one. The control: a link's event followed by another
    /// event leaves the line after that plain.
    #[test]
    fn a_lines_links_go_to_that_line_alone() {
        let mut p = play(true);
        let l = net::itemlink::ChatLink { start: 5, len: 5, id: 77 };
        p.event_no = 10;
        p.next_links = Some((10, vec![l]));
        p.event_no = 11;
        p.add_chat("Ann: Sword".into(), [1.0; 4]);
        assert_eq!(p.chat_pending.last().unwrap().3, vec![l]);
        p.add_chat("Ann: again".into(), [1.0; 4]);
        assert!(p.chat_pending.last().unwrap().3.is_empty(), "taken once");
        p.next_links = Some((11, vec![l]));
        p.event_no = 13;
        p.add_chat("Ann: late".into(), [1.0; 4]);
        assert!(p.chat_pending.last().unwrap().3.is_empty(), "the control: not the event right after");
    }

    /// A link opened asks the server once for its item; the record comes into
    /// its window; with Item Links off nothing opens. The control: one known
    /// already is not asked for again.
    #[test]
    fn a_link_opened_asks_for_its_item() {
        let mut p = play(true);
        taken(&mut p);
        p.open_link(42, "Rusty Sword");
        assert_eq!(taken(&mut p), vec![vec![0x52, 0x7E, 42, 0, 0, 0]]);
        let mut it = net::dump::NetItem::default();
        it.item.name = "Rusty Sword".into();
        p.links.record(42, Some(it));
        p.close_link_window();
        p.open_link(42, "Rusty Sword");
        assert!(taken(&mut p).is_empty(), "the control: known, not asked again");
        assert!(p.links.is_open());
        crate::settings::set_client(crate::settings::ClientOptions { item_links: false, ..crate::settings::client() });
        p.agree_item_links();
        assert!(!p.links.is_open(), "off: the windows closed");
        p.open_link(43, "Helm");
        assert!(!p.links.is_open(), "off: nothing opens");
        assert_eq!(taken(&mut p), vec![vec![0x4F, 0x7E, 0]], "the server told");
        crate::settings::set_client(crate::settings::ClientOptions { item_links: true, ..crate::settings::client() });
    }

    /// A click on a link in the chat box (the window's press and release on
    /// it) opens it: its window, the server asked for its item, and the click
    /// is the box's (Done). The control: a click beside it on the line is the
    /// line's, as ever.
    #[test]
    fn a_click_on_a_link_opens_it() {
        let mut p = play(true);
        let mut c = crate::chat::WndChat::new();
        c.text.add_text_linked(&crate::ui::wide("Ann: see Sword ok"), [1.0; 4], 0, |s| s.len() as i32 * 8, &[net::itemlink::ChatLink { start: 9, len: 5, id: 31 }]);
        let v = crate::pointer::testing::vui();
        for _ in 0..30 {
            c.draw(&v, &mut crate::pointer::testing::NullRaster, 0, 0.1);
        }
        // the link's place, found as a pointer would find it
        let mut on = None;
        'scan: for y in (300..448).step_by(2) {
            for x in (0..640).step_by(2) {
                if c.link_at([x as f32, y as f32]).is_some() {
                    on = Some([x as f32 + 2.0, y as f32 + 2.0]);
                    break 'scan;
                }
            }
        }
        let on = on.expect("the link drawn");
        p.wnd_chat = Some(c);
        taken(&mut p);
        assert!(p.wnd_chat.as_mut().unwrap().press(on, (1.0, 1.0)));
        assert_eq!(p.chat_release(), crate::chat::Let::Done, "the box's");
        assert_eq!(p.links.windows.iter().map(|w| w.id).collect::<Vec<_>>(), [31]);
        assert_eq!(taken(&mut p), vec![vec![0x52, 0x7E, 31, 0, 0, 0]]);
        assert!(p.wnd_chat.as_mut().unwrap().press([on[0] - 60.0, on[1]], (2.0, 2.0)));
        assert_eq!(p.chat_release(), crate::chat::Let::Click(2.0, 2.0), "the control: the line's");
    }

    /// With the history held, ✕ on the walk's lit link opens it; △ closes the
    /// newest window. The control: with no link lit, ✕ is not the walk's.
    #[test]
    fn the_walk_opens_a_link() {
        let mut p = play(true);
        p.wnd_chat = Some(crate::chat::WndChat::new());
        let c = p.wnd_chat.as_mut().unwrap();
        c.text.add_text_linked(&crate::ui::wide("Ann: Sword"), [1.0; 4], 0, |s| s.len() as i32 * 8, &[net::itemlink::ChatLink { start: 5, len: 5, id: 9 }]);
        c.set_scroll(1);
        p.mode = 33;
        assert!(!p.link_key(134), "nothing lit: ✕ is not the walk's");
        assert!(p.link_key(136) && p.link_walking());
        taken(&mut p);
        assert!(p.link_key(134));
        assert_eq!(p.links.windows.iter().map(|w| w.id).collect::<Vec<_>>(), [9]);
        assert_eq!(taken(&mut p), vec![vec![0x52, 0x7E, 9, 0, 0, 0]]);
        assert!(p.link_key(132) && !p.links.is_open(), "△ closes it");
    }
}
