//! The DPS meter (the server owner, 2026-11-10: "lets add dps meters, will work
//! solo, group, raid"): who in the player's group did how much damage in the
//! fight, and how fast, on a server that keeps one (go-eqoa-server's offer,
//! `net::session::OP_DPS_METER_OFFER`; the game has no meter). The server counts
//! what each player is told it did (its floating numbers of damage, its pet's
//! among them) for every member of its group, a raid group's too, and sends the
//! fight as it stands about once a second, and once more as it ends, ten seconds
//! after the group's last damage (`OP_DPS_METER_DATA`): the most damage first,
//! at most 24 lines and the player's own always among them.
//!
//! Here the fight is kept and drawn: the one under way, and the last one after
//! it ends, until the next begins; and Overall, every fight since the login (or
//! the last reset) summed by name, its DPS over all their lengths. A line is
//! its name, its damage, its DPS (its damage over the fight's length, the same
//! span for every line, so DPS ranks as damage does) and its share of the
//! whole, over a bar as long as its damage against the top line's, in its
//! class's colour, its pet's part dimmer; under the player's own a row for its
//! pet when it did damage. A HUD panel under the quest tracker and above the
//! chat box, out at a wide window's right and up a tall one's top with the
//! minimap: a click or a tap on its title shows the other (Fight or Overall),
//! on Reset clears it all (the server drops the fight under way), on its lines
//! pages through them; the wheel steps them. `/dps` types the same, and
//! Extended > Combat has its rows (DPS Meter, off by default; DPS Meter Shows;
//! Reset DPS Meter) for a controller. Off, or on a server that keeps none,
//! nothing is drawn or asked.
//!
//! Healing and power (Heirloom petition 3030: "DPS meter should have options to
//! include healing per second, and if possible power provided per second"): a
//! server of layout 2 sends each line's healing (what its heals restored, no
//! overhealing) and the power it gave others beside its damage. DPS Meter
//! Columns (DPS, the default; DPS + HPS; DPS + HPS + Power) puts their rates
//! beside the DPS in the same panel, each under its heading, and DPS Meter Sort
//! ranks the lines, draws the bars and gives the title's whole by one of them
//! (the heading in gold). `/dps hps`, `/dps power`, `/dps damage` sort by one
//! (showing its column), `/dps columns` steps the columns. On a server of layout
//! 1 the panel is the DPS alone, as before.
//!
//! Mercenary contributions are part of the owner's row, with a distinct
//! segment in the bar. Hide mercs filters them out of both views. Layout 4
//! sends the amounts on each player row; layout 3's separate records are
//! grouped by owner when the snapshot arrives.
//!
//! Mercs as Own Rows (Heirloom petition 3136: "Can we keep the option to see
//! each merc's DPS individually?"; off by default) draws each merc as a row of
//! its own instead, "Kel (Dab's merc)" in the mercs' colour, ranked with the
//! players, its figures taken off its owner's row: a server of layout 5 sends
//! each merc's own figures beside layout 4's rows. What the server had no room
//! for stays in its owner's merc share. Hide mercs still hides them all.

use crate::ui::{Layout, UiRaster, wide};
use net::session::{DPS_METER_MERC_LINES, DpsLine, DpsMeter};
use render::ui::{Anchor, Vertical};
use std::cmp::Reverse;
use std::collections::BTreeMap;

/// What the meter shows: the fight (under way, or the last), or every fight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Fight,
    Overall,
}

impl View {
    pub fn other(self) -> View {
        match self {
            View::Fight => View::Overall,
            View::Overall => View::Fight,
        }
    }

    /// Its name on the Extended page and in `client.opt`'s line.
    pub fn name(self) -> &'static str {
        match self {
            View::Fight => "Fight",
            View::Overall => "Overall",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            View::Fight => "fight",
            View::Overall => "overall",
        }
    }
}

/// What the lines are ranked and drawn by: damage, healing or power given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Metric {
    #[default]
    Damage,
    Healing,
    Power,
}

impl Metric {
    /// Its name on the Extended page, and its column's heading.
    pub fn name(self) -> &'static str {
        match self {
            Metric::Damage => "DPS",
            Metric::Healing => "HPS",
            Metric::Power => "Power",
        }
    }

    /// Its rate's unit after the title's whole.
    pub fn unit(self) -> &'static str {
        match self {
            Metric::Damage => "DPS",
            Metric::Healing => "HPS",
            Metric::Power => "PPS",
        }
    }

    /// In `client.opt`'s line.
    pub fn key(self) -> &'static str {
        match self {
            Metric::Damage => "dps",
            Metric::Healing => "hps",
            Metric::Power => "power",
        }
    }

    /// This, or DPS when `columns` do not show it.
    pub fn fit(self, columns: Columns) -> Metric {
        if columns.has(self) { self } else { Metric::Damage }
    }

    /// The next (`d` > 0) or the one before among those `columns` show, round.
    pub fn step(self, columns: Columns, d: i32) -> Metric {
        let all = [Metric::Damage, Metric::Healing, Metric::Power];
        let shown: Vec<Metric> = all.into_iter().filter(|m| columns.has(*m)).collect();
        let at = shown.iter().position(|m| *m == self.fit(columns)).unwrap_or(0) as i32;
        shown[(at + d.signum()).rem_euclid(shown.len() as i32) as usize]
    }
}

/// The rates a row shows: the DPS alone (as the meter was), with the HPS, or
/// with the HPS and the power given a second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Columns {
    #[default]
    Dps,
    Hps,
    All,
}

impl Columns {
    /// Its name on the Extended page.
    pub fn name(self) -> &'static str {
        match self {
            Columns::Dps => "DPS",
            Columns::Hps => "DPS + HPS",
            Columns::All => "DPS + HPS + Power",
        }
    }

    /// In `client.opt`'s line.
    pub fn key(self) -> &'static str {
        match self {
            Columns::Dps => "dps",
            Columns::Hps => "hps",
            Columns::All => "all",
        }
    }

    /// Whether a row shows `m`'s rate.
    pub fn has(self, m: Metric) -> bool {
        match m {
            Metric::Damage => true,
            Metric::Healing => self != Columns::Dps,
            Metric::Power => self == Columns::All,
        }
    }

    /// The fewest columns that show `m` and all these do.
    pub fn with(self, m: Metric) -> Columns {
        match m {
            _ if self.has(m) => self,
            Metric::Power => Columns::All,
            _ => Columns::Hps,
        }
    }

    /// The next (`d` > 0) or the one before, round.
    pub fn step(self, d: i32) -> Columns {
        let all = [Columns::Dps, Columns::Hps, Columns::All];
        let at = all.iter().position(|c| *c == self).unwrap_or(0) as i32;
        all[(at + d.signum()).rem_euclid(3) as usize]
    }

    /// The metrics shown, in the columns' order from the left.
    pub fn metrics(self) -> &'static [Metric] {
        match self {
            Columns::Dps => &[Metric::Damage],
            Columns::Hps => &[Metric::Damage, Metric::Healing],
            Columns::All => &[Metric::Damage, Metric::Healing, Metric::Power],
        }
    }
}

/// One line as the panel draws it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// Its place, 1 the most damage.
    pub rank: usize,
    pub name: String,
    pub class: u8,
    pub damage: u64,
    /// Its pet's share of `damage`.
    pub pet: u64,
    pub dps: f64,
    /// Of the whole, 0 to 1.
    pub share: f64,
    /// Its bar's length: its damage against the top line's, 0 to 1.
    pub bar: f32,
    pub own: bool,
    /// No longer in the player's group (it did damage in this fight and left).
    pub left: bool,
    /// Its healing (what its heals restored), its pet's share of that, and
    /// the power it gave others; each over the fight's length.
    pub heal: u64,
    pub heal_pet: u64,
    pub power: u64,
    pub hps: f64,
    pub pps: f64,
    /// The included mercenary contribution for each metric (a merc's own row:
    /// all of it).
    pub merc: [u64; 3],
    /// A merc's own row (Mercs as Own Rows), its name "Name (Owner's merc)".
    pub merc_row: bool,
}

impl Row {
    /// Its figure of `m`.
    pub fn of(&self, m: Metric) -> u64 {
        match m {
            Metric::Damage => self.damage,
            Metric::Healing => self.heal,
            Metric::Power => self.power,
        }
    }

    /// Its rate of `m`.
    pub fn rate_of(&self, m: Metric) -> f64 {
        match m {
            Metric::Damage => self.dps,
            Metric::Healing => self.hps,
            Metric::Power => self.pps,
        }
    }

    /// Its pet's part of its figure of `m`.
    fn pet_of(&self, m: Metric) -> u64 {
        match m {
            Metric::Damage => self.pet,
            Metric::Healing => self.heal_pet,
            Metric::Power => 0,
        }
    }
}

/// What the panel shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub view: View,
    /// "Fight", "Last Fight", "Overall", or "DPS Meter" with nothing yet.
    pub title: String,
    /// The fight's length (Overall: every fight's), in seconds.
    pub secs: u64,
    /// The whole's damage over it.
    pub dps: f64,
    pub rows: Vec<Row>,
    /// The player's pet's part of its own line, when it has one: its name,
    /// its figure of `sort`, that a second and its share.
    pub pet: Option<(String, u64, f64, f64)>,
    /// And its rates of each metric (its power none).
    pub pet_rates: [f64; 3],
    /// The panel's first item showing (`Meter::first`).
    pub first: usize,
    /// The rates each row shows, and what they are ranked by: the DPS alone
    /// from a server that counts no healing (layout 1).
    pub columns: Columns,
    pub sort: Metric,
    /// The filter's state, when the server supplies a mercenary breakdown.
    pub hide_mercs: Option<bool>,
}

/// A player's figures for a fight or the sum across fights.
#[derive(Clone, Debug, Default)]
struct Total {
    damage: u64,
    pet: u64,
    heal: u64,
    heal_pet: u64,
    power: u64,
    class: u8,
    own: bool,
    name: String,
    left: bool,
    merc: [u64; 3],
    /// A merc's own line (layout 5): its owner's name.
    merc_of: Option<String>,
}

impl Total {
    fn from_line(l: DpsLine) -> Self {
        Self {
            damage: l.damage.into(), pet: l.pet.into(), heal: l.heal.into(), heal_pet: l.heal_pet.into(),
            power: l.power.into(), class: l.class, own: l.own, name: l.name, left: l.left,
            merc: l.merc_share.map(u64::from), merc_of: None,
        }
    }

    /// A merc's own line of layout 5.
    fn from_merc(l: DpsLine) -> Self {
        let owner = l.owner.clone();
        Self { own: false, merc_of: Some(owner), ..Self::from_line(DpsLine { merc_share: [0; 3], ..l }) }
    }

    fn add(&mut self, other: &Self) {
        self.damage += other.damage;
        self.pet += other.pet;
        self.heal += other.heal;
        self.heal_pet += other.heal_pet;
        self.power += other.power;
        for (amount, add) in self.merc.iter_mut().zip(other.merc) {
            *amount += add;
        }
        if other.class != u8::MAX {
            self.class = other.class;
        }
        self.own |= other.own;
    }

    fn figures(&self, hide_mercs: bool) -> [u64; 3] {
        if hide_mercs {
            [self.damage, self.heal, self.power]
        } else {
            [self.damage + self.merc[0], self.heal + self.merc[1], self.power + self.merc[2]]
        }
    }
}

#[derive(Clone, Debug)]
struct Fight {
    id: u32,
    over: bool,
    length_ms: u32,
    lines: Vec<Total>,
    totals: [u64; 3],
    places: [usize; 3],
    without_mercs: [u64; 3],
    places_without_mercs: [usize; 3],
    /// Layout 5: each merc's own line.
    mercs: Vec<Total>,
}

impl From<DpsMeter> for Fight {
    fn from(m: DpsMeter) -> Self {
        let totals = [m.total, m.heal_total, m.power_total].map(u64::from);
        let mut places = [m.place, m.heal_place, m.power_place].map(usize::from);
        let mercs: Vec<Total> = m.merc_lines.into_iter().map(Total::from_merc).collect();
        let (player_lines, merc_lines): (Vec<_>, Vec<_>) = m.lines.into_iter().partition(|line| !line.merc);
        let mut lines: Vec<_> = player_lines.into_iter().map(Total::from_line).collect();
        let (without_mercs, places_without_mercs) = if let Some(t) = m.without_mercs {
            (t.totals.map(u64::from), t.places.map(usize::from))
        } else if m.layout == DPS_METER_MERC_LINES {
            let own = lines.iter().position(|line| line.own);
            for merc in merc_lines {
                let by_name = || lines.iter().position(|line| line.name == merc.owner);
                let owner = if merc.own_merc { own.or_else(by_name) } else { by_name() };
                let share = [merc.damage, merc.heal, merc.power].map(u64::from);
                if let Some(i) = owner {
                    for (amount, add) in lines[i].merc.iter_mut().zip(share) {
                        *amount += add;
                    }
                } else if !merc.owner.is_empty() {
                    // A layout 3 raid snapshot can omit the player row.
                    lines.push(Total { name: merc.owner, class: u8::MAX, left: merc.left, own: merc.own_merc, merc: share, ..Total::default() });
                }
            }
            // Layout 3's places rank mercs as competitors and its totals only
            // cover the combined view. Rank players and sum their visible base
            // figures locally when filtering that feed.
            places = [0; 3];
            let mut sum = [0; 3];
            for line in &lines {
                for (amount, add) in sum.iter_mut().zip(line.figures(true)) {
                    *amount += add;
                }
            }
            (sum, [0; 3])
        } else {
            (totals, places)
        };
        Self { id: m.fight, over: m.over, length_ms: m.length_ms, lines, totals, places, without_mercs, places_without_mercs, mercs }
    }
}

impl Fight {
    fn add_to(&self, overall: &mut BTreeMap<String, Total>, mercs: &mut BTreeMap<(String, String), Total>) {
        for line in &self.lines {
            overall.entry(line.name.clone()).and_modify(|total| total.add(line)).or_insert_with(|| line.clone());
        }
        for m in &self.mercs {
            let key = (m.merc_of.clone().unwrap_or_default(), m.name.clone());
            mercs.entry(key).and_modify(|total| total.add(m)).or_insert_with(|| m.clone());
        }
    }
}

/// Mercs as Own Rows: each merc's own line among `lines`, its figures taken off
/// its owner's merc share (what is left there is what the server had no room
/// to send apart).
fn merc_rows(lines: &mut Vec<Total>, mercs: Vec<Total>) {
    for m in mercs {
        let owner = m.merc_of.as_deref().unwrap_or_default();
        if let Some(o) = lines.iter_mut().find(|l| l.merc_of.is_none() && l.name == owner) {
            for (share, take) in o.merc.iter_mut().zip([m.damage, m.heal, m.power]) {
                *share = share.saturating_sub(take);
            }
        }
        lines.push(m);
    }
}

/// The meter's fights: the one the server last sent, a reset's fight (its
/// messages on their way are not taken), every ended fight summed by name and
/// their lengths, and which fight was summed last.
#[derive(Clone, Debug, Default)]
pub struct Meter {
    fight: Option<Fight>,
    dropped: Option<u32>,
    overall: BTreeMap<String, Total>,
    /// Each merc's own lines summed (layout 5), by its owner's name and its own.
    overall_mercs: BTreeMap<(String, String), Total>,
    overall_ms: u64,
    counted: Option<u32>,
    /// The panel's first item showing (the wheel's and a click's), back to
    /// the top at each new fight.
    pub first: usize,
    /// A fight came in layout 2 (healing and power): the columns can show.
    heals: bool,
}

/// The least length a DPS is taken over: a fight of one hit is a second's.
const LEAST_MS: u64 = 1000;

impl Meter {
    /// A fight as the server sent it: under way it stands in for what was
    /// sent of it; another number is another fight (the one before, if it
    /// never said it was over, as a restarted server's, summed as it last
    /// stood); over, it is summed into Overall once and kept, the last fight,
    /// until the next. A reset's fight still on its way is not taken.
    pub fn take(&mut self, m: DpsMeter) {
        if Some(m.fight) == self.dropped {
            return;
        }
        self.heals |= m.layout >= 2;
        let m = Fight::from(m);
        if let Some(cur) = self.fight.take() {
            if cur.id != m.id {
                self.first = 0;
                if self.counted != Some(cur.id) {
                    self.count(&cur);
                }
            }
        }
        if m.over && self.counted != Some(m.id) {
            self.count(&m);
        }
        self.fight = Some(m);
    }

    /// `m` summed into Overall.
    fn count(&mut self, m: &Fight) {
        m.add_to(&mut self.overall, &mut self.overall_mercs);
        self.overall_ms += (m.length_ms as u64).max(LEAST_MS);
        self.counted = Some(m.id);
    }

    /// Everything cleared: the fight shown, the last one and Overall; the
    /// fight under way, if any, is dropped, as the server is told
    /// (`Session::reset_dps_meter`).
    pub fn reset(&mut self) {
        let live = self.fight.as_ref().filter(|f| !f.over).map(|f| f.id);
        *self = Meter { dropped: live.or(self.dropped), heals: self.heals, ..Meter::default() };
    }

    /// Whether a fight is under way.
    pub fn live(&self) -> bool {
        self.fight.as_ref().is_some_and(|f| !f.over)
    }

    /// What the panel shows of `view`, the player's pet named `pet_name`
    /// (its own name, "Pet" when it has none): the DPS alone, ranked by damage.
    pub fn shown(&self, view: View, pet_name: &str) -> Shown {
        self.shown_by(view, pet_name, Columns::Dps, Metric::Damage, None)
    }

    /// What the panel shows of `view` with `columns`, its lines ranked by
    /// `sort` (one the columns show, else damage); the DPS alone, by damage,
    /// until a fight came with healing and power (layout 2).
    pub fn shown_by(&self, view: View, pet_name: &str, columns: Columns, sort: Metric, hide_mercs: Option<bool>) -> Shown {
        self.shown_with(view, pet_name, columns, sort, hide_mercs, false)
    }

    /// `shown_by`, each merc a row of its own where `own_rows` (Mercs as Own
    /// Rows) and the mercs are not hidden.
    pub fn shown_with(&self, view: View, pet_name: &str, columns: Columns, sort: Metric, hide_mercs: Option<bool>, own_rows: bool) -> Shown {
        let columns = if self.heals { columns } else { Columns::Dps };
        let sort = sort.fit(columns);
        let hidden = hide_mercs == Some(true);
        let mi = sort as usize;
        let (title, ms, mut lines, sent) = match view {
            View::Fight => match &self.fight {
                None => ("DPS Meter", 0, Vec::new(), None),
                Some(f) => {
                    let title = if f.over { "Last Fight" } else { "Fight" };
                    let (totals, places) = if hidden { (f.without_mercs, f.places_without_mercs) } else { (f.totals, f.places) };
                    let mut lines = f.lines.clone();
                    // its own rows: the server's places rank players with their
                    // mercs, so the player's own is placed here
                    let sent = if own_rows && !hidden && !f.mercs.is_empty() {
                        merc_rows(&mut lines, f.mercs.clone());
                        (totals[mi], 0)
                    } else {
                        (totals[mi], places[mi])
                    };
                    (title, u64::from(f.length_ms), lines, Some(sent))
                }
            },
            View::Overall => {
                let mut sums: BTreeMap<String, Total> = self.overall.clone();
                let mut mercs = self.overall_mercs.clone();
                let mut ms = self.overall_ms;
                // the fight under way, not yet summed
                if let Some(f) = self.fight.as_ref().filter(|f| self.counted != Some(f.id)) {
                    f.add_to(&mut sums, &mut mercs);
                    ms += (f.length_ms as u64).max(LEAST_MS);
                }
                let mut lines: Vec<_> = sums.into_values().collect();
                if own_rows && !hidden {
                    merc_rows(&mut lines, mercs.into_values().collect());
                }
                let title = if lines.is_empty() { "DPS Meter" } else { "Overall" };
                (title, if lines.is_empty() { 0 } else { ms }, lines, None)
            }
        };
        let figure = |t: &Total| t.figures(hidden)[mi];
        // Break ties in the server's order, by combined damage.
        lines.sort_by_key(|t| (Reverse(figure(t)), Reverse(t.figures(false)[Metric::Damage as usize])));
        let secs_div = ms.max(LEAST_MS) as f64 / 1000.0;
        let (total, place) = sent.unwrap_or_else(|| (lines.iter().map(figure).sum(), 0));
        let top = lines.first().map_or(0, figure).max(1);
        let mut out: Vec<Row> = lines
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let figures = t.figures(hidden);
                let [damage, heal, power] = figures;
                let v = figures[mi];
                let merc_row = t.merc_of.is_some();
                Row {
                    rank: i + 1,
                    name: match &t.merc_of {
                        Some(o) if !o.is_empty() => format!("{} ({o}'s merc)", t.name),
                        Some(_) => format!("{} (merc)", t.name),
                        None => t.name,
                    },
                    class: t.class,
                    damage,
                    pet: t.pet,
                    dps: damage as f64 / secs_div,
                    share: if total > 0 { v as f64 / total as f64 } else { 0.0 },
                    bar: (v as f64 / top as f64).min(1.0) as f32,
                    own: t.own,
                    left: view == View::Fight && t.left,
                    heal,
                    heal_pet: t.heal_pet,
                    power,
                    hps: heal as f64 / secs_div,
                    pps: power as f64 / secs_div,
                    merc: if merc_row { figures } else if hidden { [0; 3] } else { t.merc },
                    merc_row,
                }
            })
            .collect();
        // the player's own, past the rest sent: last, its place said
        if let Some(o) = out.iter().position(|r| r.own)
            && place > o + 1
        {
            let mut r = out.remove(o);
            r.rank = place;
            out.push(r);
        }
        let pet = out.iter().find(|r| r.own && (r.pet_of(sort) > 0 || columns.metrics().iter().any(|m| r.pet_of(*m) > 0))).map(|r| {
            let name = if pet_name.is_empty() { "Pet" } else { pet_name };
            let v = r.pet_of(sort);
            (name.to_string(), v, v as f64 / secs_div, if total > 0 { v as f64 / total as f64 } else { 0.0 })
        });
        let pet_rates = out.iter().find(|r| r.own).map_or([0.0; 3], |r| [r.pet as f64 / secs_div, r.heal_pet as f64 / secs_div, 0.0]);
        Shown {
            view,
            title: title.to_string(),
            secs: ms / 1000,
            dps: if out.is_empty() { 0.0 } else { total as f64 / secs_div },
            rows: out,
            pet,
            pet_rates,
            first: self.first,
            columns,
            sort,
            hide_mercs,
        }
    }
}

/// An item of the panel's list: a line, or the player's pet's row under its
/// own line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Line(usize),
    Pet,
    /// "No fight yet": nothing to show.
    Nothing,
}

/// Every item of `s` in order: each line, the pet's row after the player's own.
pub fn items(s: &Shown) -> Vec<Item> {
    let mut out = Vec::with_capacity(s.rows.len() + 1);
    for (i, r) in s.rows.iter().enumerate() {
        out.push(Item::Line(i));
        if r.own && s.pet.is_some() {
            out.push(Item::Pet);
        }
    }
    if out.is_empty() {
        out.push(Item::Nothing);
    }
    out
}

/// The items `slots` show from `first` (kept so the last slot can still be
/// reached, and never on the pet's row apart from its owner's line, which it
/// follows): the player's own line, when it is not among them, in the last
/// slot (its place said by its rank).
pub fn window(s: &Shown, all: &[Item], first: usize, slots: usize) -> Vec<Item> {
    let slots = slots.max(1).min(all.len());
    let mut first = first.min(all.len() - slots);
    if first > 0 && all[first] == Item::Pet {
        first -= 1;
    }
    let mut out: Vec<Item> = all[first..first + slots].to_vec();
    let own = s.rows.iter().position(|r| r.own);
    if let Some(o) = own
        && !out.contains(&Item::Line(o))
    {
        *out.last_mut().unwrap() = Item::Line(o);
    }
    out
}

/// The panel's column in the HUD's frame (`Anchor::Right`, `Vertical::Top`):
/// the quest tracker's (`tracker::COLUMN`), from its bottom, above the chat box,
/// up.
pub const COLUMN: [i32; 4] = crate::tracker::COLUMN;
/// The title's height and a row's; the words' height in them.
pub const HEAD_H: i32 = 14;
pub const ROW_H: i32 = 13;
const TEXT_H: f32 = 10.0;
/// The most rows shown; past them the wheel and a click step through.
pub const MOST_ROWS: usize = 10;
/// Between the quest tracker and the panel.
pub const GAP: i32 = 4;

/// Where the panel is and what it shows: its rectangle in the HUD's frame, its
/// controls (the view's name, Reset, Hide mercs), its rows' rectangle and the
/// items showing; and how many items there are and how far the list steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub rect: [i32; 4],
    pub view: [i32; 4],
    pub reset: [i32; 4],
    pub merc_filter: Option<[i32; 4]>,
    pub rows: [i32; 4],
    pub shown: Vec<Item>,
    pub items: usize,
    /// The farthest `first` goes: 0 when every item shows.
    pub room: usize,
}

/// The panel for `s` with `room` frame pixels for it above the column's bottom:
/// as many rows as it has, `MOST_ROWS` at most, fewer to fit (one at least).
pub fn panel(s: &Shown, room: i32) -> Panel {
    let all = items(s);
    let controls_h = HEAD_H + if s.hide_mercs.is_some() { ROW_H } else { 0 };
    let head_h = controls_h + if s.columns == Columns::Dps { 0 } else { ROW_H };
    let fit = ((room - head_h - 2) / ROW_H).max(1) as usize;
    let slots = all.len().min(MOST_ROWS).min(fit);
    let shown = window(s, &all, s.first, slots);
    let h = head_h + shown.len() as i32 * ROW_H + 2;
    let (x0, x1, bottom) = (COLUMN[0], COLUMN[2], COLUMN[3]);
    let top = bottom - h;
    let mid = (x0 + x1) / 2 + 30;
    Panel {
        rect: [x0, top, x1, bottom],
        view: [x0, top, mid, top + HEAD_H],
        reset: [x1 - 40, top, x1, top + HEAD_H],
        merc_filter: s.hide_mercs.map(|_| [x0, top + HEAD_H, x1, top + controls_h]),
        rows: [x0, top + head_h, x1, bottom],
        shown,
        items: all.len(),
        room: all.len() - slots,
    }
}

/// The next `first` a click on the rows gives: a page on (the rows less the
/// one the player's own may take), back to the top past the last.
pub fn page(first: usize, slots: usize, room: usize) -> usize {
    if room == 0 || first >= room { 0 } else { (first + slots.saturating_sub(1).max(1)).min(room) }
}

/// A damage figure, as short as it reads: 9876, 12.3k, 1.23M.
pub fn amount(n: u64) -> String {
    match n {
        0..=9_999 => n.to_string(),
        10_000..=999_999 => format!("{:.1}k", n as f64 / 1000.0),
        _ => format!("{:.2}M", n as f64 / 1_000_000.0),
    }
}

/// A DPS: one decimal under 100, whole under 10,000, then as `amount`.
pub fn rate(v: f64) -> String {
    if v < 100.0 {
        format!("{v:.1}")
    } else if v < 10_000.0 {
        format!("{v:.0}")
    } else {
        amount(v as u64)
    }
}

/// A length: m:ss, or h:mm:ss past an hour.
pub fn clock(secs: u64) -> String {
    if secs >= 3600 { format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60) } else { format!("{}:{:02}", secs / 60, secs % 60) }
}

/// A share: whole percent.
pub fn percent(v: f64) -> String {
    format!("{:.0}%", (v * 100.0).clamp(0.0, 100.0))
}

/// A class's colour (by its kind: the tanks bronze, the melee gold, the
/// priests green, the casters blue), grey for one it does not know.
pub fn class_colour(class: u8) -> [f32; 3] {
    match class {
        0 | 2 | 3 => [0.80, 0.56, 0.38],
        1 | 4 | 5 | 6 => [0.93, 0.78, 0.30],
        7..=9 => [0.42, 0.82, 0.48],
        10..=14 => [0.52, 0.62, 1.0],
        _ => [0.6, 0.6, 0.6],
    }
}

/// `s` cut to `most` wide by `width`, "..." at its end when it is cut.
fn fit(s: &str, most: f32, width: &impl Fn(&str) -> f32) -> String {
    if width(s) <= most {
        return s.to_string();
    }
    let mut cut: Vec<char> = s.chars().collect();
    while !cut.is_empty() {
        cut.pop();
        let t = cut.iter().collect::<String>() + "...";
        if width(&t) <= most {
            return t;
        }
    }
    String::new()
}

/// One piece of text the panel draws: its words, where (its left, or its right
/// with `right`), its row's top and its colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub s: String,
    pub x: f32,
    pub right: bool,
    pub y: f32,
    pub color: [f32; 4],
}

/// One bar: its rectangle and colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub r: [f32; 4],
    pub color: [f32; 4],
}

const WHITE: [f32; 4] = [0.95, 0.95, 0.92, 1.0];
const DIM: [f32; 4] = [0.62, 0.62, 0.6, 1.0];
const OWN: [f32; 4] = [1.0, 0.86, 0.35, 1.0];
const MERC: [f32; 4] = [0.80, 0.40, 0.94, 0.70];
/// A merc's own row's words (Mercs as Own Rows).
const MERC_TEXT: [f32; 4] = [0.88, 0.70, 0.98, 1.0];
const BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.4];
const HEAD_BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.3];
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

/// What the panel draws for `s` at `p`, its words measured by `width`: the
/// title (the view's name and the fight's length, the whole's rate of the
/// sort, Reset), then each row's bar and words: its place and name, its damage,
/// DPS and share; the pet's row indented. With more columns than the DPS, the
/// columns' headings under the title (the sort's in gold) and each row's place,
/// name and rates alone.
pub fn pieces(s: &Shown, p: &Panel, width: impl Fn(&str) -> f32) -> (Vec<Bar>, Vec<Text>) {
    let (mut bars, mut texts) = (Vec::new(), Vec::new());
    let x0 = p.rect[0] as f32 + 3.0;
    let x1 = p.rect[2] as f32 - 3.0;
    let ty = |row_top: i32| row_top as f32 + ((ROW_H as f32 - TEXT_H) / 2.0).floor();
    // the title
    let head = p.rect[1];
    let title = if s.secs > 0 || s.title != "DPS Meter" { format!("{} {}", s.title, clock(s.secs)) } else { s.title.clone() };
    texts.push(Text { s: title, x: x0, right: false, y: ty(head) + 1.0, color: WHITE });
    texts.push(Text { s: "Reset".into(), x: x1, right: true, y: ty(head) + 1.0, color: DIM });
    if !s.rows.is_empty() {
        let reset_w = width("Reset");
        texts.push(Text { s: format!("{} {}", rate(s.dps), s.sort.unit()), x: x1 - reset_w - 8.0, right: true, y: ty(head) + 1.0, color: WHITE });
    }
    if let (Some(hidden), Some(button)) = (s.hide_mercs, p.merc_filter) {
        let y = button[1] as f32;
        bars.push(Bar { r: [x0 - 1.0, y, x1 + 1.0, y + ROW_H as f32 - 1.0], color: HEAD_BACK });
        bars.push(Bar { r: [x0 + 1.0, y + 3.0, x0 + 8.0, y + 10.0], color: MERC });
        texts.push(Text { s: "Hide mercs".into(), x: x0 + 12.0, right: false, y: ty(button[1]), color: WHITE });
        let (state, color) = if hidden { ("On", OWN) } else { ("Off", DIM) };
        texts.push(Text { s: state.into(), x: x1, right: true, y: ty(button[1]), color });
    }
    let multi = s.columns != Columns::Dps;
    // the columns, from the right: share, DPS, damage; the name before them.
    // More columns: each metric's rate, the last at the right
    let share_r = x1;
    let dps_r = x1 - width("100%") - 6.0;
    let dmg_r = dps_r - width("8888.8") - 6.0;
    let metrics = s.columns.metrics();
    let col_w = width("8888.8").max(width("Power")) + 6.0;
    let rate_r = |k: usize| x1 - (metrics.len() - 1 - k) as f32 * col_w;
    let name_most = if multi { rate_r(0) - col_w - x0 } else { (dmg_r - width("888.8k") - 6.0) - x0 };
    if multi {
        for (k, m) in metrics.iter().enumerate() {
            texts.push(Text { s: m.name().into(), x: rate_r(k), right: true, y: ty(p.rows[1] - ROW_H), color: if *m == s.sort { OWN } else { DIM } });
        }
    }
    let sort = s.sort;
    let top_v = s.rows.first().map_or(1, |r| r.of(sort).max(1));
    for (k, item) in p.shown.iter().enumerate() {
        let y = p.rows[1] + k as i32 * ROW_H;
        let (yt, yb) = (y as f32 + 1.0, (y + ROW_H) as f32 - 1.0);
        let full = x1 + 2.0 - (x0 - 2.0);
        match *item {
            Item::Nothing => {
                let words = if s.view == View::Fight { "No fight yet" } else { "No fights yet" };
                texts.push(Text { s: words.into(), x: x0, right: false, y: ty(y), color: DIM });
            }
            Item::Line(i) => {
                let r = &s.rows[i];
                let c = class_colour(r.class);
                let w = full * r.bar;
                let v = r.of(sort);
                if w > 0.0 && v > 0 {
                    let merc = r.merc[sort as usize].min(v);
                    let pet = r.pet_of(sort).min(v - merc);
                    let mut start = x0 - 2.0;
                    let mut end = 0;
                    let segments = [
                        (v - merc - pet, [c[0], c[1], c[2], 0.55]),
                        (pet, [c[0], c[1], c[2], 0.28]),
                        (merc, MERC),
                    ];
                    for (amount, color) in segments {
                        end += amount;
                        let next = x0 - 2.0 + w * (end as f64 / v as f64) as f32;
                        if next > start {
                            bars.push(Bar { r: [start, yt, next, yb], color });
                        }
                        start = next;
                    }
                }
                let color = if r.own {
                    OWN
                } else if r.left {
                    DIM
                } else if r.merc_row {
                    MERC_TEXT
                } else {
                    WHITE
                };
                let name = fit(&format!("{}. {}", r.rank, r.name), name_most, &width);
                texts.push(Text { s: name, x: x0, right: false, y: ty(y), color });
                if multi {
                    for (k, m) in metrics.iter().enumerate() {
                        texts.push(Text { s: rate(r.rate_of(*m)), x: rate_r(k), right: true, y: ty(y), color });
                    }
                } else {
                    texts.push(Text { s: amount(r.damage), x: dmg_r, right: true, y: ty(y), color });
                    texts.push(Text { s: rate(r.dps), x: dps_r, right: true, y: ty(y), color });
                    texts.push(Text { s: percent(r.share), x: share_r, right: true, y: ty(y), color });
                }
            }
            Item::Pet => {
                let Some((name, figure, dps, share)) = &s.pet else { continue };
                let w = full * (*figure as f64 / top_v as f64).min(1.0) as f32;
                let c = class_colour(s.rows.iter().find(|r| r.own).map_or(255, |r| r.class));
                if w > 0.0 {
                    bars.push(Bar { r: [x0 + 6.0, yt, x0 + 6.0 + (w - 8.0).max(0.0), yb], color: [c[0], c[1], c[2], 0.28] });
                }
                texts.push(Text { s: fit(name, name_most - 8.0, &width), x: x0 + 8.0, right: false, y: ty(y), color: DIM });
                if multi {
                    for (k, m) in metrics.iter().enumerate() {
                        let words = if *m == Metric::Power { "-".to_string() } else { rate(s.pet_rates[*m as usize]) };
                        texts.push(Text { s: words, x: rate_r(k), right: true, y: ty(y), color: DIM });
                    }
                } else {
                    texts.push(Text { s: amount(*figure), x: dmg_r, right: true, y: ty(y), color: DIM });
                    texts.push(Text { s: rate(*dps), x: dps_r, right: true, y: ty(y), color: DIM });
                    texts.push(Text { s: percent(*share), x: share_r, right: true, y: ty(y), color: DIM });
                }
            }
        }
    }
    (bars, texts)
}

fn corners(r: [f32; 4]) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
}

/// Draws the panel `p` for `s` in the HUD's frame, placed as the quest tracker
/// is (`Anchor::Right`, `Vertical::Top`).
pub fn draw(ui: &mut UiRaster, s: &Shown, p: &Panel) {
    ui.ui.anchor = Anchor::Right;
    ui.ui.vertical = Vertical::Top;
    let r = p.rect.map(|v| v as f32);
    ui.solid(3, BACK, corners(r));
    ui.solid(3, HEAD_BACK, corners([r[0], r[1], r[2], r[1] + HEAD_H as f32]));
    let Some(font) = ui.assets.font else { return };
    let scale = TEXT_H / ui.ui.fonts[font].height.max(1) as f32;
    let (bars, texts) = {
        let f = &ui.ui.fonts[font];
        pieces(s, p, |t| f.string_width(&wide(t)) as f32 * scale)
    };
    for b in bars {
        ui.solid(3, b.color, corners(b.r));
    }
    for t in texts {
        let w = wide(&t.s);
        let x = if t.right { t.x - ui.ui.fonts[font].string_width(&w) as f32 * scale } else { t.x };
        // outlined: the words in black a pixel round, then in their colour
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            ui.ui.text_scaled(font, x + dx, t.y + dy, scale, OUTLINE, &w);
        }
        ui.ui.text_scaled(font, x, t.y, scale, t.color, &w);
    }
}

thread_local! {
    /// Extended's Reset DPS Meter, asked and not yet done (`take_reset`).
    static RESET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Extended's Reset DPS Meter: done by play at its next frame (`take_reset`),
/// as the options window has no hold of the meter.
pub fn ask_reset() {
    RESET.with(|r| r.set(true));
}

/// Whether a reset was asked since the last look, and none asked now.
pub fn take_reset() -> bool {
    RESET.with(|r| r.replace(false))
}

/// What a click or a tap on the panel does (`hud::Pick::Meter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// the title's view: the other shown
    View,
    /// Reset
    Reset,
    HideMercs,
    /// the rows: a page on
    Rows,
}

/// `/dps`'s words: no word, the other view; `fight`, `overall` or `all`, that
/// one; `reset`; `on` or `off`, the meter; anything else, how to type it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Show(View),
    Reset,
    On(bool),
    /// ranked by a metric, its column shown
    Sort(Metric),
    /// the next columns
    Columns,
    Help,
}

pub fn command(arg: &str) -> Command {
    match arg.trim().to_ascii_lowercase().as_str() {
        "" => Command::Toggle,
        "fight" | "last" => Command::Show(View::Fight),
        "overall" | "all" => Command::Show(View::Overall),
        "reset" | "clear" => Command::Reset,
        "on" => Command::On(true),
        "off" => Command::On(false),
        "dps" | "damage" => Command::Sort(Metric::Damage),
        "hps" | "heal" | "healing" => Command::Sort(Metric::Healing),
        "power" | "pps" | "mana" => Command::Sort(Metric::Power),
        "columns" | "cols" => Command::Columns,
        _ => Command::Help,
    }
}

/// What `/dps` with no meant word says.
pub const HELP: &str = "/dps: the DPS meter's other view; /dps fight, /dps overall, /dps reset, /dps on, /dps off; /dps damage, /dps hps, /dps power (ranked by it), /dps columns";

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::DpsLine;

    fn line(name: &str, class: u8, damage: u32, pet: u32, own: bool) -> DpsLine {
        DpsLine { own, left: false, class, damage, pet, name: name.into(), ..Default::default() }
    }

    fn fight(n: u32, over: bool, ms: u32, lines: Vec<DpsLine>) -> DpsMeter {
        let total = lines.iter().map(|l| l.damage).sum();
        let place = lines.iter().position(|l| l.own).map_or(0, |p| p as u16 + 1);
        DpsMeter { layout: 1, fight: n, over, length_ms: ms, total, count: lines.len() as u16, place, lines, ..Default::default() }
    }

    /// A fight's course as the panel shows it: under way, each message the
    /// fight as it stands; over, the last fight kept and summed into Overall
    /// once (a repeat of its last message is not summed again); the next fight
    /// takes its place and Overall goes on, its DPS over both fights' lengths.
    /// A fight the server never ended (a restart) is summed as it last stood
    /// when the next one comes.
    #[test]
    fn a_fight_then_the_last_kept_and_overall_summed() {
        let mut m = Meter::default();
        assert_eq!(m.shown(View::Fight, "").title, "DPS Meter");
        assert_eq!(items(&m.shown(View::Fight, "")), [Item::Nothing], "no fight yet");
        m.take(fight(7, false, 2000, vec![line("Dab", 3, 200, 0, true), line("Jib", 10, 100, 40, false)]));
        let s = m.shown(View::Fight, "");
        assert_eq!((s.title.as_str(), s.secs, s.dps), ("Fight", 2, 150.0));
        assert_eq!(s.rows.iter().map(|r| (r.rank, r.name.as_str(), r.damage, r.dps)).collect::<Vec<_>>(), [(1, "Dab", 200, 100.0), (2, "Jib", 100, 50.0)]);
        assert!((s.rows[1].share - 1.0 / 3.0).abs() < 1e-9 && s.rows[1].bar == 0.5);
        assert!(m.live());
        assert_eq!(m.shown(View::Overall, "").rows.len(), 2, "Overall has the fight under way");
        m.take(fight(7, false, 4000, vec![line("Dab", 3, 400, 0, true), line("Jib", 10, 300, 40, false)]));
        m.take(fight(7, true, 4000, vec![line("Dab", 3, 400, 0, true), line("Jib", 10, 300, 40, false)]));
        m.take(fight(7, true, 4000, vec![line("Dab", 3, 400, 0, true), line("Jib", 10, 300, 40, false)]));
        let s = m.shown(View::Fight, "");
        assert_eq!((s.title.as_str(), s.secs), ("Last Fight", 4), "over: kept");
        assert!(!m.live());
        let o = m.shown(View::Overall, "");
        assert_eq!((o.secs, o.rows[0].damage, o.rows[1].damage), (4, 400, 300), "summed once");
        // the next fight: in its place, Overall over both
        m.take(fight(8, false, 1000, vec![line("Dab", 3, 100, 0, true)]));
        assert_eq!(m.shown(View::Fight, "").rows.len(), 1);
        let o = m.shown(View::Overall, "");
        assert_eq!((o.secs, o.rows[0].damage, o.rows[0].dps), (5, 500, 100.0));
        // fight 9 comes with 8 never over: 8 summed as it stood
        m.take(fight(9, false, 3000, vec![line("Dab", 3, 60, 0, true)]));
        let o = m.shown(View::Overall, "");
        assert_eq!((o.secs, o.rows.iter().find(|r| r.name == "Dab").unwrap().damage), (8, 560));
        assert_eq!(o.title, "Overall");
    }

    /// A reset clears the fight, the last and Overall; what the server sent of
    /// the fight under way before it heard is dropped; its next is taken. The
    /// control: with no reset the same message is taken.
    #[test]
    fn a_reset_clears_and_drops_the_fight_on_its_way() {
        let mut m = Meter::default();
        m.take(fight(3, true, 1000, vec![line("Dab", 3, 50, 0, true)]));
        m.take(fight(4, false, 1000, vec![line("Dab", 3, 70, 0, true)]));
        m.first = 3;
        m.reset();
        assert_eq!(m.shown(View::Fight, "").title, "DPS Meter");
        assert!(m.shown(View::Overall, "").rows.is_empty());
        assert_eq!(m.first, 0);
        m.take(fight(4, false, 2000, vec![line("Dab", 3, 90, 0, true)]));
        assert!(m.shown(View::Fight, "").rows.is_empty(), "the dropped fight's message");
        m.take(fight(5, false, 1000, vec![line("Dab", 3, 10, 0, true)]));
        assert_eq!(m.shown(View::Fight, "").rows[0].damage, 10, "the next is taken");
        let mut c = Meter::default();
        c.take(fight(4, false, 2000, vec![line("Dab", 3, 90, 0, true)]));
        assert_eq!(c.shown(View::Fight, "").rows[0].damage, 90, "the control");
    }

    /// The player's pet: its row under the player's own line, named, its share
    /// of the whole; a line with a pet's share draws its part dimmer. A fight
    /// of one hit is a second's (no DPS over nothing).
    #[test]
    fn the_pet_has_its_row_under_its_owner() {
        let mut m = Meter::default();
        m.take(fight(1, false, 0, vec![line("Dab", 10, 300, 200, true), line("Bob", 0, 100, 0, false)]));
        let s = m.shown(View::Fight, "Fire Elemental");
        assert_eq!(s.pet, Some(("Fire Elemental".to_string(), 200, 200.0, 0.5)));
        assert_eq!(items(&s), [Item::Line(0), Item::Pet, Item::Line(1)]);
        assert_eq!(s.rows[0].dps, 300.0, "a second's");
        assert_eq!(m.shown(View::Fight, "").pet.unwrap().0, "Pet", "no name: Pet");
        let p = panel(&s, 200);
        let (bars, texts) = pieces(&s, &p, |t| t.len() as f32 * 5.0);
        assert_eq!(bars.len(), 4, "the owner's two parts, the pet's, the other's: {bars:?}");
        assert!(texts.iter().any(|t| t.s == "Fire Elemental"));
        let mut c = Meter::default();
        c.take(fight(1, false, 0, vec![line("Dab", 10, 300, 0, true)]));
        assert_eq!(items(&c.shown(View::Fight, "x")), [Item::Line(0)], "the control: no pet's damage, no row");
    }

    /// A raid's list: as many rows as fit, the player's own in the last when it
    /// is past them, at its place; a page on and back to the top; the wheel's
    /// room is what is left out. The control: all showing, no room.
    #[test]
    fn a_long_list_keeps_the_players_own_line() {
        let mut lines: Vec<DpsLine> = (0..23).map(|i| line(&format!("M{i:02}"), 0, 1000 - i, 0, false)).collect();
        lines.push(line("Dab", 3, 10, 0, true));
        let mut f = fight(2, false, 10_000, lines);
        f.count = 40;
        f.place = 37;
        let mut m = Meter::default();
        m.take(f);
        let s = m.shown(View::Fight, "");
        assert_eq!((s.rows[23].rank, s.rows[22].rank), (37, 23), "its place, not its index");
        let p = panel(&s, 1000);
        assert_eq!(p.shown.len(), MOST_ROWS);
        assert_eq!(p.shown.last(), Some(&Item::Line(23)), "its own last");
        assert_eq!(p.shown[0], Item::Line(0));
        assert_eq!(p.room, 24 - MOST_ROWS);
        let small = panel(&s, HEAD_H + 2 + 3 * ROW_H);
        assert_eq!(small.shown, [Item::Line(0), Item::Line(1), Item::Line(23)], "three fit");
        assert_eq!(page(0, 3, small.room), 2);
        assert_eq!(page(20, 3, 21), 21);
        assert_eq!(page(21, 3, 21), 0, "back to the top");
        let mut m2 = m.clone();
        m2.first = 20;
        let at = panel(&m2.shown(View::Fight, ""), HEAD_H + 2 + 3 * ROW_H);
        assert_eq!(at.shown, [Item::Line(20), Item::Line(21), Item::Line(23)]);
        m2.first = 21;
        let end = panel(&m2.shown(View::Fight, ""), HEAD_H + 2 + 3 * ROW_H);
        assert_eq!(end.shown, [Item::Line(21), Item::Line(22), Item::Line(23)], "its own among them: none taken");
        // a window from the pet's row starts at its owner's line
        let mut pets = Meter::default();
        pets.take(fight(3, false, 1000, (0..4).map(|i| line(&format!("P{i}"), 10, 900 - i, if i == 0 { 300 } else { 0 }, i == 0)).collect()));
        pets.first = 1;
        let s = pets.shown(View::Fight, "Spectre");
        assert_eq!(items(&s)[1], Item::Pet);
        assert_eq!(panel(&s, HEAD_H + 2 + 2 * ROW_H).shown, [Item::Line(0), Item::Pet], "its owner first");
        pets.first = 2;
        assert_eq!(panel(&pets.shown(View::Fight, ""), HEAD_H + 2 + 2 * ROW_H).shown, [Item::Line(1), Item::Line(0)], "the control: past it");
        let few = panel(&fight_shown(3), 1000);
        assert_eq!((few.shown.len(), few.room), (3, 0), "the control");
        // the panel sits on the column's bottom, never above its top
        for room in [0, 30, 80, 168, 400] {
            let p = panel(&s, room);
            assert_eq!(p.rect[3], COLUMN[3]);
            assert!(p.rect[1] >= COLUMN[3] - room.max(HEAD_H + 2 + ROW_H), "{room}: {:?}", p.rect);
        }
    }

    fn fight_shown(n: u32) -> Shown {
        let mut m = Meter::default();
        m.take(fight(1, false, 1000, (0..n).map(|i| line(&format!("N{i}"), 1, 100 + i, 0, i == 0)).collect()));
        m.shown(View::Fight, "")
    }

    /// The figures as the panel writes them.
    #[test]
    fn figures_read_short() {
        assert_eq!([amount(0), amount(9999), amount(12_345), amount(999_949), amount(1_234_567)], ["0", "9999", "12.3k", "999.9k", "1.23M"]);
        assert_eq!([rate(7.46), rate(456.7), rate(1234.4), rate(12_345.0)], ["7.5", "457", "1234", "12.3k"]);
        assert_eq!([clock(0), clock(65), clock(3725)], ["0:00", "1:05", "1:02:05"]);
        assert_eq!([percent(0.384), percent(1.0), percent(1.5)], ["38%", "100%", "100%"]);
        assert_ne!(class_colour(0), class_colour(13));
        assert_eq!(class_colour(200), [0.6, 0.6, 0.6]);
    }

    /// A row's words fit its column: a long name is cut with "...", never its
    /// figures; the player's own drawn in its colour, one gone from the group
    /// dimmed.
    #[test]
    fn a_rows_words_fit_its_column() {
        let mut m = Meter::default();
        let mut gone = line("Leftmidfight", 0, 50, 0, false);
        gone.left = true;
        m.take(fight(1, false, 1000, vec![line("Averyveryverylongname", 6, 900, 0, true), gone]));
        let s = m.shown(View::Fight, "");
        let p = panel(&s, 200);
        let width = |t: &str| t.len() as f32 * 6.0;
        let (_, texts) = pieces(&s, &p, width);
        let name = texts.iter().find(|t| t.s.starts_with("1.")).unwrap();
        assert!(name.s.ends_with("...") && name.color == OWN, "{name:?}");
        let dmg = texts.iter().find(|t| t.s == "900").unwrap();
        assert!(name.x + width(&name.s) < dmg.x - width("900"), "the name clear of the damage: {name:?} {dmg:?}");
        assert!(texts.iter().any(|t| t.s.starts_with("2. Leftmid") && t.color == DIM));
        assert!(texts.iter().any(|t| t.s == "Fight 0:01") && texts.iter().any(|t| t.s == "950 DPS") && texts.iter().any(|t| t.s == "Reset"));
        for t in &texts {
            let (l, r) = if t.right { (t.x - width(&t.s), t.x) } else { (t.x, t.x + width(&t.s)) };
            assert!(l >= p.rect[0] as f32 && r <= p.rect[2] as f32, "{t:?} out of {:?}", p.rect);
        }
    }

    /// A line of layout 2: its damage, healing and power given.
    fn line2(name: &str, class: u8, damage: u32, heal: u32, power: u32, own: bool) -> DpsLine {
        DpsLine { own, left: false, class, damage, heal, power, name: name.into(), ..Default::default() }
    }

    fn fight2(n: u32, over: bool, ms: u32, lines: Vec<DpsLine>) -> DpsMeter {
        let mut f = fight(n, over, ms, lines);
        f.layout = 2;
        f.heal_total = f.lines.iter().map(|l| l.heal).sum();
        f.power_total = f.lines.iter().map(|l| l.power).sum();
        let place = |of: fn(&DpsLine) -> u32| {
            let own = f.lines.iter().find(|l| l.own).map_or(0, of);
            f.lines.iter().filter(|l| of(l) > own).count() as u16 + 1
        };
        (f.heal_place, f.power_place) = (place(|l| l.heal), place(|l| l.power));
        f
    }

    /// Healing and power (Heirloom petition 3030): ranked by healing, the
    /// healer first, its bar the longest, its share of the healing, the
    /// title's whole the HPS; by power the same. The DPS columns alone ranks
    /// by damage whatever the sort (the control); a fight of layout 1 shows
    /// the DPS alone whatever is chosen, until one of layout 2 comes.
    #[test]
    fn healing_and_power_rank_and_draw() {
        let lines = vec![line2("Tank", 0, 900, 0, 0, false), line2("Dab", 9, 300, 600, 20, true), line2("Ench", 12, 100, 200, 400, false)];
        let mut m = Meter::default();
        m.take(fight(1, false, 2000, lines.clone()));
        let s = m.shown_by(View::Fight, "", Columns::All, Metric::Healing, None);
        assert_eq!((s.columns, s.sort), (Columns::Dps, Metric::Damage), "layout 1: the DPS alone");
        m.take(fight2(2, false, 2000, lines));
        let s = m.shown_by(View::Fight, "", Columns::Hps, Metric::Healing, None);
        assert_eq!((s.columns, s.sort), (Columns::Hps, Metric::Healing));
        assert_eq!(s.rows.iter().map(|r| (r.rank, r.name.as_str())).collect::<Vec<_>>(), [(1, "Dab"), (2, "Ench"), (3, "Tank")]);
        assert_eq!((s.rows[0].hps, s.rows[0].dps, s.rows[0].bar, s.rows[1].bar), (300.0, 150.0, 1.0, 200.0 / 600.0));
        assert!((s.rows[0].share - 0.75).abs() < 1e-9);
        assert_eq!(s.dps, 400.0, "the title's whole: the HPS");
        let p = m.shown_by(View::Fight, "", Columns::All, Metric::Power, None);
        assert_eq!(p.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Ench", "Dab", "Tank"]);
        assert_eq!((p.rows[0].pps, p.dps), (200.0, 210.0));
        let hidden = m.shown_by(View::Fight, "", Columns::Hps, Metric::Power, None);
        assert_eq!(hidden.sort, Metric::Damage, "a sort the columns do not show: damage");
        let c = m.shown_by(View::Fight, "", Columns::Dps, Metric::Healing, None);
        assert_eq!((c.sort, c.rows[0].name.as_str()), (Metric::Damage, "Tank"), "the control: the DPS alone");
        // Overall sums the healing and power too
        m.take(fight2(2, true, 2000, vec![line2("Dab", 9, 300, 600, 20, true)]));
        m.take(fight2(3, false, 2000, vec![line2("Dab", 9, 0, 200, 0, true)]));
        let o = m.shown_by(View::Overall, "", Columns::All, Metric::Healing, None);
        // fights 1 (its lines carry healing, as layout 1 never does) and 2
        // summed once each, 3 under way: 600 + 600 + 200 over 6 s
        assert_eq!((o.rows[0].heal, o.secs), (1400, 6));
        assert!((o.rows[0].hps - 1400.0 / 6.0).abs() < 1e-9);
    }

    /// Ranked by healing, the player's own line pinned past the lines sent
    /// is placed by its place by healing (the server's), not by damage's.
    #[test]
    fn the_own_line_is_placed_by_the_sort() {
        let mut lines: Vec<DpsLine> = (0..23).map(|i| line2(&format!("M{i:02}"), 0, 1000 - i, 500 - i, 0, false)).collect();
        lines.push(line2("Dab", 3, 10, 5, 0, true));
        let mut f = fight2(4, false, 1000, lines);
        (f.count, f.place, f.heal_place) = (40, 37, 31);
        let mut m = Meter::default();
        m.take(f);
        let s = m.shown_by(View::Fight, "", Columns::Hps, Metric::Healing, None);
        assert_eq!((s.rows[23].own, s.rows[23].rank), (true, 31), "by healing: 31st");
        assert_eq!(m.shown(View::Fight, "").rows[23].rank, 37, "the control: by damage, 37th");
    }

    /// More columns: their headings under the title, the sort's in gold, a
    /// row's rates under them, every word inside the panel and the name clear
    /// of the first rate; the pet's row with its damage's and healing's rates
    /// and no power. The rows begin under the headings.
    #[test]
    fn the_columns_draw_under_their_headings() {
        let mut lines = vec![line2("Averyveryverylongname", 9, 900, 450, 30, true), line2("Tank", 0, 1200, 0, 0, false)];
        lines[0].pet = 300;
        lines[0].heal_pet = 150;
        let mut m = Meter::default();
        m.take(fight2(1, false, 1000, lines));
        let s = m.shown_by(View::Fight, "Spirit", Columns::All, Metric::Healing, None);
        assert_eq!(s.pet, Some(("Spirit".to_string(), 150, 150.0, 1.0 / 3.0)));
        let p = panel(&s, 300);
        assert_eq!(p.rows[1], p.rect[1] + HEAD_H + ROW_H, "under the headings");
        let width = |t: &str| t.len() as f32 * 6.0;
        let (_, texts) = pieces(&s, &p, width);
        let heads: Vec<_> = texts.iter().filter(|t| ["DPS", "HPS", "Power"].contains(&t.s.as_str())).collect();
        assert_eq!(heads.len(), 3, "{texts:?}");
        assert!(heads.iter().all(|t| (t.color == OWN) == (t.s == "HPS")), "the sort's heading gold: {heads:?}");
        let name = texts.iter().find(|t| t.s.starts_with("1.")).unwrap();
        let dps = texts.iter().find(|t| t.s == "900").unwrap();
        assert!(name.s.ends_with("...") && name.x + width(&name.s) < dps.x - width("900"), "{name:?} {dps:?}");
        assert!(texts.iter().any(|t| t.s == "450") && texts.iter().any(|t| t.s == "30.0"), "its HPS and power a second: {texts:?}");
        assert!(texts.iter().any(|t| t.s == "-" && t.color == DIM), "the pet's row has no power");
        assert!(texts.iter().any(|t| t.s == "Fight 0:01") && texts.iter().any(|t| t.s == "450 HPS"));
        for t in &texts {
            let (l, r) = if t.right { (t.x - width(&t.s), t.x) } else { (t.x, t.x + width(&t.s)) };
            assert!(l >= p.rect[0] as f32 && r <= p.rect[2] as f32, "{t:?} out of {:?}", p.rect);
        }
        let one = panel(&m.shown(View::Fight, ""), 300);
        assert_eq!(one.rows[1], one.rect[1] + HEAD_H, "the control: the DPS alone has no headings");
    }

    /// The sort steps round the columns shown; the columns round three; a
    /// sort asked widens the columns to show it.
    #[test]
    fn the_sort_and_columns_step() {
        assert_eq!(Metric::Damage.step(Columns::All, 1), Metric::Healing);
        assert_eq!(Metric::Power.step(Columns::All, 1), Metric::Damage);
        assert_eq!(Metric::Damage.step(Columns::All, -1), Metric::Power);
        assert_eq!(Metric::Damage.step(Columns::Hps, -1), Metric::Healing);
        assert_eq!(Metric::Healing.step(Columns::Dps, 1), Metric::Damage);
        assert_eq!(Columns::All.step(1), Columns::Dps);
        assert_eq!(Columns::Dps.step(-1), Columns::All);
        assert_eq!(Columns::Dps.with(Metric::Healing), Columns::Hps);
        assert_eq!(Columns::Hps.with(Metric::Power), Columns::All);
        assert_eq!(Columns::All.with(Metric::Healing), Columns::All);
        assert_eq!(command("hps"), Command::Sort(Metric::Healing));
        assert_eq!(command("Power"), Command::Sort(Metric::Power));
        assert_eq!(command("damage"), Command::Sort(Metric::Damage));
        assert_eq!(command("columns"), Command::Columns);
    }

    /// `/dps`'s words.
    #[test]
    fn the_command_words() {
        assert_eq!(command(""), Command::Toggle);
        assert_eq!(command(" Overall "), Command::Show(View::Overall));
        assert_eq!(command("all"), Command::Show(View::Overall));
        assert_eq!(command("fight"), Command::Show(View::Fight));
        assert_eq!(command("RESET"), Command::Reset);
        assert_eq!(command("off"), Command::On(false));
        assert_eq!(command("nonsense"), Command::Help);
        assert_eq!(View::Fight.other(), View::Overall);
    }

    /// A merc's line of layout 3: its owner named, the player's own merc's
    /// flagged.
    fn merc3(name: &str, class: u8, damage: u32, heal: u32, power: u32, owner: &str, own_merc: bool) -> DpsLine {
        DpsLine { class, damage, heal, power, name: name.into(), merc: true, own_merc, owner: owner.into(), ..Default::default() }
    }

    #[test]
    fn legacy_mercenary_lines_fold_into_their_owners() {
        let mut dab = line2("Dab", 11, 300, 0, 0, true);
        dab.pet = 120; // its summoned pet's
        let lines = vec![
            merc3("Ralthas", 0, 500, 0, 0, "Dab", true),
            dab,
            merc3("Kellasis", 12, 100, 0, 700, "", true), // owner name omitted
            line2("Jib", 9, 80, 600, 0, false),
            merc3("Ralthas", 0, 60, 0, 0, "Jib", false),
            line2("Ralthas", 6, 40, 0, 0, false),
        ];
        let mut f = fight2(1, true, 2000, lines);
        f.layout = 3;
        let mut m = Meter::default();
        m.take(f);
        for view in [View::Fight, View::Overall] {
            let shown = m.shown_by(view, "Spectre", Columns::All, Metric::Power, Some(false));
            assert_eq!(shown.rows[0].name, "Dab");
            assert_eq!(shown.rows[0].power, 700);
            assert_eq!(shown.rows[0].merc, [600, 0, 700]);
            let hidden = m.shown_by(view, "Spectre", Columns::Dps, Metric::Damage, Some(true));
            let figures: Vec<_> = hidden.rows.iter().map(|r| (r.name.as_str(), r.damage)).collect();
            assert_eq!(figures, [("Dab", 300), ("Jib", 80), ("Ralthas", 40)]);
            assert_eq!(hidden.pet.as_ref().map(|p| p.1), Some(120));
            assert!(hidden.rows.iter().all(|r| r.merc == [0; 3]));
        }
    }

    #[test]
    fn integrated_mercenary_shares_update_ranks_and_bar_segments() {
        let mut dab = line2("Dab", 11, 300, 50, 0, true);
        dab.pet = 100;
        dab.merc_share = [900, 280, 60];
        let jib = line2("Jib", 8, 500, 80, 20, false);
        let mut f = fight2(1, true, 2000, vec![dab, jib]);
        f.layout = 4;
        (f.total, f.heal_total, f.power_total, f.place) = (1700, 410, 80, 1);
        // The full fight has contributors beyond this snapshot's two rows.
        f.without_mercs = Some(net::session::DpsRankedTotals { totals: [1000, 130, 20], places: [3, 2, 2] });
        let mut m = Meter::default();
        m.take(f.clone());
        m.take(f);
        let visible = m.shown_by(View::Fight, "Spectre", Columns::Dps, Metric::Damage, Some(false));
        assert_eq!((visible.rows[0].damage, visible.rows[0].dps, visible.dps), (1200, 600.0, 850.0));
        let p = panel(&visible, 400);
        let (bars, texts) = pieces(&visible, &p, |t| t.len() as f32 * 4.0);
        let row: Vec<_> = bars.iter().filter(|b| b.r[1] == p.rows[1] as f32 + 1.0).collect();
        assert_eq!(row.len(), 3);
        assert_eq!(row[2].color, MERC);
        assert_eq!(row[0].r[2], row[1].r[0]);
        assert_eq!(row[1].r[2], row[2].r[0]);
        let full = row[2].r[2] - row[0].r[0];
        assert!(((row[2].r[2] - row[2].r[0]) / full - 0.75).abs() < 1e-6);
        assert!(texts.iter().any(|t| t.s == "Hide mercs"));
        assert!(texts.iter().any(|t| t.s == "Off"));

        let hidden = m.shown_by(View::Fight, "Spectre", Columns::Dps, Metric::Damage, Some(true));
        assert_eq!(hidden.rows[0].name, "Jib");
        assert_eq!((hidden.rows[1].damage, hidden.rows[1].rank, hidden.dps), (300, 3, 500.0));
        let p = panel(&hidden, 400);
        let (bars, texts) = pieces(&hidden, &p, |t| t.len() as f32 * 4.0);
        assert!(bars.iter().all(|b| b.r[1] < p.rows[1] as f32 || b.color != MERC));
        assert!(texts.iter().any(|t| t.s == "On"));
        let all = m.shown_by(View::Overall, "", Columns::All, Metric::Healing, Some(false));
        assert_eq!((all.rows[0].heal, all.rows[0].power), (330, 60));
        let all_hidden = m.shown_by(View::Overall, "", Columns::All, Metric::Healing, Some(true));
        assert_eq!(all_hidden.rows[0].name, "Jib");
        assert_eq!((all_hidden.rows[1].heal, all_hidden.rows[1].power), (50, 0));
    }

    /// Mercs as Own Rows (Heirloom petition 3136): layout 5's merc lines drawn
    /// as rows of their own, "Kel (Dab's merc)" in the mercs' colour, ranked
    /// with the players, their figures off their owner's row; what the server
    /// sent no line for stays in the owner's share. Overall sums them by owner
    /// and name. The controls: off, and hidden, the rows are layout 4's.
    #[test]
    fn mercs_as_own_rows() {
        let mut dab = line2("Dab", 11, 300, 50, 0, true);
        dab.merc_share = [900, 280, 60];
        let jib = line2("Jib", 8, 500, 80, 20, false);
        let mut f = fight2(1, true, 2000, vec![dab, jib]);
        f.layout = 5;
        (f.total, f.heal_total, f.power_total, f.place) = (1700, 410, 80, 1);
        f.without_mercs = Some(net::session::DpsRankedTotals { totals: [800, 130, 20], places: [2, 2, 2] });
        let merc = |name: &str, class: u8, damage: u32, heal: u32, power: u32| DpsLine {
            class, damage, heal, power, name: name.into(), merc: true, own_merc: true, owner: "Dab".into(), ..DpsLine::default()
        };
        // Kel and Ord sent apart; 100 damage of Dab's share the server had no room for
        f.merc_lines = vec![merc("Kel", 9, 600, 280, 60), merc("Ord", 0, 200, 0, 0)];
        let mut m = Meter::default();
        m.take(f);

        let off = m.shown_with(View::Fight, "", Columns::Dps, Metric::Damage, Some(false), false);
        assert_eq!(off.rows.iter().map(|r| (r.name.as_str(), r.damage)).collect::<Vec<_>>(), [("Dab", 1200), ("Jib", 500)], "off: layout 4's");

        let on = m.shown_with(View::Fight, "", Columns::Dps, Metric::Damage, Some(false), true);
        let rows: Vec<_> = on.rows.iter().map(|r| (r.name.as_str(), r.damage, r.merc_row, r.rank)).collect();
        assert_eq!(rows, [("Kel (Dab's merc)", 600, true, 1), ("Jib", 500, false, 2), ("Dab", 400, false, 3), ("Ord (Dab's merc)", 200, true, 4)]);
        let dab = &on.rows[2];
        assert_eq!((dab.merc, dab.own), ([100, 0, 0], true), "Dab keeps the share sent no line");
        assert_eq!(on.rows[0].merc, [600, 280, 60], "a merc's row is all merc");
        assert_eq!(on.dps, 850.0, "the whole is the same");
        let p = panel(&on, 400);
        let (bars, texts) = pieces(&on, &p, |t| t.len() as f32 * 4.0);
        let first: Vec<_> = bars.iter().filter(|b| b.r[1] == p.rows[1] as f32 + 1.0).collect();
        assert!(!first.is_empty() && first.iter().all(|b| b.color == MERC), "the merc's bar in the mercs' colour");
        assert!(texts.iter().any(|t| t.s.contains("Kel (Dab's merc)") && t.color == MERC_TEXT));

        let hidden = m.shown_with(View::Fight, "", Columns::Dps, Metric::Damage, Some(true), true);
        assert!(hidden.rows.iter().all(|r| !r.merc_row), "hidden: no merc rows");

        let all = m.shown_with(View::Overall, "", Columns::Dps, Metric::Damage, Some(false), true);
        assert_eq!(all.rows.iter().map(|r| (r.name.as_str(), r.damage)).collect::<Vec<_>>(), [("Kel (Dab's merc)", 600), ("Jib", 500), ("Dab", 400), ("Ord (Dab's merc)", 200)]);
    }
}
