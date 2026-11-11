//! Auto-cast and auto targeting, the client's own (the server owner's of
//! 2026-11-11: "lets add auto-cast to abilities to fire off
//! cooldown/timer/buff gone/pet's die similar to the mercs, also auto
//! targeting").
//!
//! An ability on a quick bar the player marks as auto (a right click on its
//! row, a long press on a touch screen, ○ or E held a second on it, or
//! `/autocast`) is used by the client itself when it is ready and its
//! condition holds, through the same `Send0032` a press sends: the server
//! sees an ordinary cast, its own refusals and lockouts as ever, and nothing
//! new goes on the wire (so a PS2-only server is served alike). Its
//! condition is the slot's kind ([`Kind`]), guessed from the ability's scope
//! and its description ([`guess`]) and changed with `/autocast`:
//!
//! - Attack: in a fight (the attack held, or a creature hit the player, its
//!   pet or its group, or they hit one, in the last [`FIGHT_MS`]) with the
//!   target a living creature (an NPC's plate, not a merchant's) within the
//!   ability's range: off its recast, it fires;
//! - Buff: its effect (by its name in the player's effects, channel 0x43:
//!   go-eqoa-server and Sandstorm put a spell's name there) is not on the
//!   player, or a long one ([`REFRESH_LONGER`]) is within [`REFRESH_BEFORE`]
//!   of the end the client last saw it run to (a server sends no time left of
//!   the player's own effects, so the length is learned, the longest seen);
//!   cast at the player;
//! - Buff on the pet (an ability of scope 3: the server lands it on the
//!   player's pet whatever is targeted, cast.go's castTargetsLocked): only
//!   with a pet out; its effect is looked for in the pet's effects
//!   (go-eqoa-server's 0x7E33, with Extended > Pet Effects on: each with its
//!   time left, so it is cast again within [`REFRESH_BEFORE`] of its end),
//!   and on the player's own (a server that put a pet's buffs there). A
//!   server that sends no pet's effects (a PS2's, or Pet Effects off): a cast
//!   the server began (its lockout, 0x00CD) holds the slot for the buff's
//!   length ([`length`]: its description's "5min", else [`LENGTH_MS`]), and a
//!   pet died, let go or summoned anew ends the hold (petition 1616: a pet
//!   buff was cast three times and switched off, its effect never on the
//!   caster's bar);
//! - Pet: the player has no pet (none summoned, or it died or was let go);
//! - Heal: the player below [`HEAL_PCT`] of its hit points and the heal's
//!   own effect (a heal over time) not on it; cast at the player.
//!
//! The order, as the server's mercs choose (`entity/merccast.go`): a heal,
//! the pet, out of a fight the buffs, the attacks, in a fight the buffs; in a
//! kind, the bars' order (the player's own). One auto cast at a time, [`GAP_MS`]
//! apart and never in a cast (its window and the server's 0.4 s after), and
//! never while dead, invisible (a cast ends it), with a window up (a menu,
//! a merchant, a trade), or short of the ability's power (it waits). The
//! client does not know a stun, a silence or a sitting player: the server
//! refuses those casts as it refuses a press. EverQuest Online Adventures
//! casts on the move (the walk walks through a cast), so moving blocks none.
//!
//! A cast the server never began (no lockout came for it, 0x00CD: refused,
//! no target, out of range, short of power) whose outcome is not seen either
//! (a buff's effect, a pet) is a miss: tried again later and later, and after
//! [`MISSES_OFF`] such the slot is taken off auto, the player told. A buff the
//! server began whose effect shows nowhere the client looks (the pet's on a
//! server that sends none of them, one a stronger buff keeps off) is no miss:
//! the slot is held for the buff's length (petition 1616).
//!
//! Auto targeting (Extended > Combat > Auto Target, off by default, or
//! `/autotarget`): with no target, or a dead one, the nearest living creature
//! that hit the player in the last [`FIGHT_MS`] is targeted, else the nearest
//! that hit its pet or its group; a player, a pet (a charmed creature is one)
//! or an NPC that hit none of them never. A server tells a client none of a
//! creature's target (go-eqoa-server's NPC channels carry 0), so the hits
//! the client is told of (0x00DB) are what says who fights whom.
//!
//! What is auto is kept for each character (`autocast.opt`, a line a
//! character, as the quest guide keeps its own), by the ability's id, so it
//! follows the ability round the bars; Extended > Combat > Auto-Cast turns it
//! all off (`/autocast off`).

use net::dump::Ability;
use std::collections::BTreeMap;

/// What an auto slot waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Heal,
    Pet,
    Buff,
    Attack,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Heal => "heal",
            Kind::Pet => "pet",
            Kind::Buff => "buff",
            Kind::Attack => "attack",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s.to_ascii_lowercase().as_str() {
            "heal" | "heals" => Kind::Heal,
            "pet" | "summon" => Kind::Pet,
            "buff" | "buffs" => Kind::Buff,
            "attack" | "nuke" | "dmg" | "damage" | "art" => Kind::Attack,
            _ => return None,
        })
    }

    /// When it fires on ability `a`, for the chat box: a buff on the pet's
    /// (scope 3) is its.
    pub fn when_for(self, a: &Ability) -> &'static str {
        if self == Kind::Buff && a.scope == 3 {
            return "when its effect is gone from your pet";
        }
        self.when()
    }

    /// When it fires, for the chat box.
    pub fn when(self) -> &'static str {
        match self {
            Kind::Heal => "when you are below half health",
            Kind::Pet => "when you have no pet",
            Kind::Buff => "when its effect is gone from you",
            Kind::Attack => "off its timer in a fight, at your target",
        }
    }
}

/// The least time between two auto casts.
pub const GAP_MS: i64 = 1500;
/// What the server keeps a cast busy past its cast time (go-eqoa-server's
/// `castBusy`, Sandstorm's 0.37-0.55 s).
pub const BUSY_MS: i64 = 400;
/// A hit given or taken this recently keeps a fight on.
pub const FIGHT_MS: i64 = 10_000;
/// A Heal slot fires below this share of the player's hit points.
pub const HEAL_PCT: i32 = 50;
/// The same slot is not tried again sooner than this after it was tried.
pub const RETRY_MS: i64 = 3000;
/// A buff's effect, or a pet, not seen this long after the cast's time: a
/// miss.
pub const SEE_MS: i64 = 8000;
/// The wait after a miss, times the misses in a row.
pub const MISS_WAIT_MS: i64 = 30_000;
/// After this many misses in a row the slot is taken off auto.
pub const MISSES_OFF: u32 = 3;
/// A buff seen to last at least this long is cast again before its end...
pub const REFRESH_LONGER: i64 = 60_000;
/// ...this long before.
pub const REFRESH_BEFORE: i64 = 6000;
/// A buff's length when the client has neither seen it run nor read it in
/// its description (go-eqoa-server's and the lists' usual, "5min").
pub const LENGTH_MS: i64 = 300_000;
/// Auto targeting picks at most one target this often.
pub const TARGET_GAP_MS: i64 = 1000;

/// What a word of an ability's name and description says of it.
fn has(s: &str, words: &[&str]) -> bool {
    words.iter().any(|w| s.contains(w))
}

/// The kind an ability's scope (0 self, 1 a target, 2 the group, 3 the pet)
/// and its words make it, or none where the client cannot tell when it
/// would be wanted (a gate, an illusion, invisibility, an item made, a
/// resurrection, a pet's own spell...): the player may still name one with
/// `/autocast`.
pub fn guess(a: &Ability) -> Option<Kind> {
    let d = format!("{} {}", a.name, a.desc).to_lowercase();
    if has(&d, &["gate", "teleport", "portal", "invis", "conceal", "hide", "illusion", "assume the form", "form of", "disguise", "resurrect", "ressurect", "revive", "rez ", "an item", "summon item", "creates", "draught", "charm", "lull", "pacify", "calm", "feign", "camouflage", "levitat", "breath", "shrink", "grow", "track", "forage", "bind", "evacuat", "succor", "pickpocket", "stance", "raiment", "experience", "familiar", "eye of"]) {
        return None;
    }
    let summon = has(&d, &["pet summon", "summons pet", "summon pet", "summons a pet"]) || (d.contains("summons") && has(&d, &["to aid you", "by your side", "to fight", "guardian", "golem", "animation", "elemental", "skeleton", "spectre", "zombie", "mummy"]) && !d.contains("aura"));
    if summon {
        return Some(Kind::Pet);
    }
    // a buff on the pet (scope 3: the server lands it on the pet whatever is
    // targeted): its words, never a heal of it ("Pet Heal", "Increase
    // Hitpoints by 539") nor a harm
    if a.scope == 3 {
        let heal = has(&d, &["pet heal", "heal hp", "heals ", "heal your", "increase hitpoints by", "increase hp by", "restores health", "restore health", "mend", "repair"]);
        let buff = has(&d, &["buff", "proc", "increas", "empower", "strengthen", "reinforce", "mastery", "mark", "aura", "shield", "regenerat", "over time", "per tic", "min"]);
        return (buff && !heal).then_some(Kind::Buff);
    }
    // anything wider (4) the player casts
    if !(0..=2).contains(&a.scope) {
        return None;
    }
    let harmful = has(&d, &["decrease", "damage", "injur", "strike", "attack", "drain", "steal", "siphon", "resisted by", "debuff", "taunt", "hate", "root", "snare", "stun", "lance", "bolt", "blast", "burn", "shot", "blow", "slash", "pierce", "crush", "melee", "smite", "nuke", "lightning", "fire", "frost", "poison", "disease", "storm", "shock", "sleet", "spray", "jet", "slam", "malady", "affliction", "fever", "blight", "corrosive", "venom", "gale", "wilting", "sickening", "decaying wind"]);
    // a buff's own words go first: "increases the target's armor", "Defensive
    // Proc" (a proc names damage it does when the player is struck)
    let lasting = has(&d, &["buff", "proc", "increase", "aura", "skin", "shield", "armor", "min", "regenerat", "over time", "per tic", "surround"]);
    if a.scope == 1 && harmful && !(lasting && !has(&d, &["decrease", "drain", "steal", "siphon", "resisted by", "target, decrease", "strike", "injur"])) {
        return Some(Kind::Attack);
    }
    let heal = has(&d, &["heal", "increase hitpoints by", "increase hp by", "restores health", "restore health", "mend"]) && !has(&d, &["pet heal"]);
    let timed = has(&d, &["min", "buff", "30m", "hour"]);
    if heal && !timed {
        return Some(Kind::Heal);
    }
    if a.scope == 0 || a.scope == 2 || (a.scope == 1 && lasting) {
        return Some(Kind::Buff);
    }
    None
}

/// A buff's length as its description gives it ("Pet, Increase STR & DEX by
/// 10, 5min", "for thirty minutes", "1 hour"), in ms: the first number with a
/// unit of time after it.
pub fn length(desc: &str) -> Option<i64> {
    let d = desc.to_lowercase();
    let words: Vec<&str> = d.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let unit = |u: &str| -> Option<i64> {
        Some(match u {
            "m" | "min" | "mins" | "minute" | "minutes" => 60_000,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000,
            "s" | "sec" | "secs" | "second" | "seconds" => 1000,
            _ => return None,
        })
    };
    let number = |w: &str| -> Option<i64> {
        if let Ok(n) = w.parse::<i64>() {
            return Some(n);
        }
        const WORDS: [(&str, i64); 13] = [("one", 1), ("two", 2), ("three", 3), ("four", 4), ("five", 5), ("six", 6), ("ten", 10), ("fifteen", 15), ("twenty", 20), ("thirty", 30), ("forty", 40), ("fifty", 50), ("sixty", 60)];
        WORDS.iter().find(|(s, _)| *s == w).map(|&(_, n)| n)
    };
    for (i, w) in words.iter().enumerate() {
        // "5min", "30m", "1hr"
        let digits = w.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && digits < w.len()
            && let (Ok(n), Some(u)) = (w[..digits].parse::<i64>(), unit(&w[digits..]))
        {
            return Some(n * u);
        }
        // "5 min", "thirty minutes"
        if let (Some(n), Some(u)) = (number(w), words.get(i + 1).and_then(|u| unit(u))) {
            return Some(n * u);
        }
    }
    None
}

/// Whether an effect's name (channel 0x43: at most 31 characters, the
/// server's cut) is the ability named `name`.
pub fn effect_is(effect: &str, name: &str) -> bool {
    if effect.is_empty() {
        return false;
    }
    if effect.eq_ignore_ascii_case(name) {
        return true;
    }
    let n = effect.chars().count();
    n >= 31 && name.chars().take(n).collect::<String>().eq_ignore_ascii_case(effect)
}

/// The player's target as a foe, as play sees it this frame: a living
/// creature with an NPC's plate (not a merchant's, a banker's...), and how
/// far it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foe {
    pub distance: f32,
}

/// What play knows this game frame, for the choice.
#[derive(Clone, Debug, Default)]
pub struct Now {
    /// the session's clock, ms
    pub ms: i64,
    /// dead, or lying as the dead do
    pub dead: bool,
    /// in a cast: its window, or the server's busy time after it
    pub casting: bool,
    /// a window up: a menu, a merchant, a trade, the spell book...
    pub window: bool,
    pub invisible: bool,
    /// hit points and their most, power
    pub hp: i32,
    pub hp_max: i32,
    pub power: i32,
    pub has_pet: bool,
    /// the pet's id, when there is one
    pub pet: Option<u32>,
    /// the pet's effects with the ms left of each, from a server that sends
    /// them (0x7E33; none: it sends none, or Pet Effects is off)
    pub pet_effects: Option<Vec<(String, i64)>>,
    /// the attack held (the 0x40 carries it)
    pub attacking: bool,
    /// the target, a foe
    pub foe: Option<Foe>,
    /// the player's effects' names
    pub effects: Vec<String>,
    /// the ability bars shown (`Play::ability_bars`)
    pub bars: i32,
}

/// An auto cast to send: the ability (its id), and whether at the player
/// (a buff, a heal, a summoning) rather than the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cast {
    pub id: i64,
    pub kind: Kind,
    pub at_self: bool,
}

/// One auto slot's last try.
#[derive(Clone, Copy, Debug, Default)]
struct Try {
    /// not before this again
    next: i64,
    /// a buff's or a summoning's outcome still to be seen, by when
    watch: Option<i64>,
    misses: u32,
    /// when it was sent
    sent: i64,
    /// the server began it (its lockout came, 0x00CD)
    began: bool,
    /// a begun buff whose effect shows nowhere the client looks: not cast
    /// again before this
    held: Option<i64>,
}

/// A creature in a fight with the player's side: when it last hit the
/// player, when it last hit the pet or the group, when the player's side
/// last hit it.
#[derive(Clone, Copy, Debug, Default)]
struct Fight {
    on_me: Option<i64>,
    on_friend: Option<i64>,
    by_us: Option<i64>,
}

/// A creature auto targeting may take, as play sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub id: u32,
    /// an NPC's plate (`m_p256` 0): not a player, not a pet
    pub npc: bool,
    pub alive: bool,
    pub distance: f32,
}

/// The auto slots of the character played, and what they need between frames.
#[derive(Debug, Default)]
pub struct AutoCast {
    /// by the ability's id
    pub on: BTreeMap<i64, Kind>,
    /// whose (the server id and the name), as `questguide` keys its record
    who: Option<(i64, String)>,
    /// the last auto cast sent
    last: Option<i64>,
    tries: BTreeMap<i64, Try>,
    /// each effect on the player since when (none: on before the client saw
    /// it begin), and the longest each was seen to last
    since: BTreeMap<String, Option<i64>>,
    lasted: BTreeMap<String, i64>,
    /// the effects have been looked at once (a login's are not "new")
    looked: bool,
    fights: BTreeMap<u32, Fight>,
    /// the last target auto targeting took
    targeted: Option<i64>,
    /// the pet last seen: another (or none) ends a pet buff's hold
    pet: Option<u32>,
    /// ○ or E held on the HUD's selection: its ability, since when, done
    hold: Option<(i64, i64, bool)>,
    /// what to say in the chat box
    pub said: Vec<String>,
}

/// The file the auto slots are kept in.
pub const FILE: &str = "autocast.opt";

impl AutoCast {
    /// The character played: its auto slots read when it is another.
    pub fn character(&mut self, server_id: i64, name: &str) {
        if self.who.as_ref().is_some_and(|(i, n)| *i == server_id && n == name) {
            return;
        }
        *self = AutoCast { who: Some((server_id, name.to_string())), on: kept_of(&crate::settings::auto_cast_file(), server_id, name), ..Default::default() };
    }

    fn keep(&self) {
        if let Some((id, name)) = &self.who {
            let text = with_kept(&crate::settings::auto_cast_file(), *id, name, &self.on);
            crate::settings::save_auto_cast_file(&text);
        }
    }

    /// The slot of ability `id` auto as `kind`, or not (`None`); kept.
    pub fn set(&mut self, id: i64, kind: Option<Kind>) {
        match kind {
            Some(k) => {
                self.on.insert(id, k);
            }
            None => {
                self.on.remove(&id);
            }
        }
        self.tries.remove(&id);
        self.keep();
    }

    /// The slot of `a` turned: auto as its guess, or not. The line to say.
    pub fn toggle(&mut self, a: &Ability) -> String {
        if self.on.contains_key(&a.id) {
            self.set(a.id, None);
            return format!("Auto-cast off: {}.", a.name);
        }
        match guess(a) {
            Some(k) => {
                self.set(a.id, Some(k));
                format!("Auto-cast on: {} ({}, {}).", a.name, k.word(), k.when_for(a))
            }
            None => format!("Auto-cast can't tell when to use {}: type /autocast {} attack, buff, pet or heal.", a.name, a.name),
        }
    }

    /// A 0x00DB: `attacker` hit (or healed) `defender`; `me`, the pet and the
    /// group are the player's side.
    pub fn hit(&mut self, attacker: u32, defender: u32, ms: i64, me: Option<u32>, friends: &[u32]) {
        let ours = |id: u32| Some(id) == me || friends.contains(&id);
        if ours(attacker) && !ours(defender) {
            self.fights.entry(defender).or_default().by_us = Some(ms);
        } else if ours(defender) && !ours(attacker) {
            let f = self.fights.entry(attacker).or_default();
            if Some(defender) == me {
                f.on_me = Some(ms);
            } else {
                f.on_friend = Some(ms);
            }
        }
        self.fights.retain(|_, f| [f.on_me, f.on_friend, f.by_us].into_iter().flatten().any(|t| ms - t < 6 * FIGHT_MS));
    }

    /// The effects on the player now: when each began, and how long each that
    /// ended had lasted.
    pub fn effects(&mut self, names: &[String], ms: i64) {
        let gone: Vec<String> = self.since.keys().filter(|n| !names.contains(n)).cloned().collect();
        for n in gone {
            if let Some(Some(s)) = self.since.remove(&n) {
                let l = self.lasted.entry(n).or_insert(0);
                *l = (*l).max(ms - s);
            }
        }
        for n in names {
            if !n.is_empty() && !self.since.contains_key(n) {
                self.since.insert(n.clone(), self.looked.then_some(ms));
            }
        }
        self.looked = true;
    }

    /// Whether the buff named `name` is on the player and not near its end.
    fn buff_held(&self, name: &str, effects: &[String], ms: i64) -> bool {
        let Some(e) = effects.iter().find(|e| effect_is(e, name)) else { return false };
        match (self.since.get(e).copied().flatten(), self.lasted.get(e).copied()) {
            (Some(s), Some(l)) if l >= REFRESH_LONGER => ms < s + l - REFRESH_BEFORE,
            _ => true,
        }
    }

    /// Whether buff `a`'s effect shows where the client looks: on the
    /// player, or (a buff on the pet) in the pet's effects.
    fn shows(a: &Ability, now: &Now) -> bool {
        now.effects.iter().any(|e| effect_is(e, &a.name)) || (a.scope == 3 && now.pet_effects.as_ref().is_some_and(|l| l.iter().any(|(e, _)| effect_is(e, &a.name))))
    }

    /// Whether buff `a` is up and not near its end: on the player
    /// (`buff_held`), or on the pet with more than `REFRESH_BEFORE` left.
    fn buff_up(&self, a: &Ability, now: &Now) -> bool {
        self.buff_held(&a.name, &now.effects, now.ms) || (a.scope == 3 && now.pet_effects.as_ref().is_some_and(|l| l.iter().any(|(e, left)| effect_is(e, &a.name) && *left > REFRESH_BEFORE)))
    }

    /// Buff `a`'s length: as the client saw it run on the player, else as its
    /// description says, else `LENGTH_MS`.
    fn length_of(&self, a: &Ability) -> i64 {
        self.lasted.iter().find(|(e, _)| effect_is(e, &a.name)).map(|(_, &l)| l).or_else(|| length(&a.desc)).unwrap_or(LENGTH_MS)
    }

    /// What each auto slot's last try came to: a buff's effect seen, or a pet,
    /// is a success; a buff the server began (its lockout, 0x00CD) whose
    /// effect is not seen is held for its length; neither by its time a miss,
    /// waited out longer each time, and the slot taken off auto after
    /// `MISSES_OFF` (said). Another pet (or none) ends the pet buffs' holds.
    fn watch(&mut self, abilities: &BTreeMap<i64, Ability>, now: &Now) {
        if now.pet != self.pet {
            self.pet = now.pet;
            for (id, t) in self.tries.iter_mut() {
                if abilities.get(id).is_some_and(|a| a.scope == 3) {
                    t.held = None;
                }
            }
        }
        let mut off = Vec::new();
        let mut holds = Vec::new();
        for (id, t) in self.tries.iter_mut() {
            let Some(by) = t.watch else { continue };
            let (Some(kind), Some(a)) = (self.on.get(id), abilities.get(id)) else { continue };
            let seen = match kind {
                Kind::Buff => Self::shows(a, now),
                Kind::Pet => now.has_pet,
                _ => true,
            };
            if seen {
                t.watch = None;
                t.misses = 0;
                t.held = None;
                continue;
            }
            if !t.began && a.lockout > 0 {
                t.began = true;
                t.misses = 0;
                if *kind == Kind::Buff {
                    holds.push((*id, a.clone()));
                }
            }
            if now.ms >= by {
                t.watch = None;
                if !t.began {
                    t.misses += 1;
                    t.next = now.ms + MISS_WAIT_MS * t.misses as i64;
                    if t.misses >= MISSES_OFF {
                        off.push((*id, a.name.clone(), *kind));
                    }
                }
            }
        }
        for (id, a) in holds {
            let until = now.ms + (a.cast_time.clamp(0, 30) * 1000).max(0) + self.length_of(&a);
            if let Some(t) = self.tries.get_mut(&id) {
                t.held = Some(until);
            }
        }
        for (id, name, kind) in off {
            self.set(id, None);
            let what = if kind == Kind::Pet { "no pet came of it" } else { "the server never began it" };
            self.said.push(format!("Auto-cast off: {name} ({what} {MISSES_OFF} times)."));
        }
    }

    /// The auto cast to send now, if any (and it is noted as sent).
    pub fn choose(&mut self, abilities: &BTreeMap<i64, Ability>, now: &Now) -> Option<Cast> {
        self.watch(abilities, now);
        // no hit points, alive: the stat channel not yet told of a respawn (a
        // heal went at a full player on the rig, 2026-11-11)
        let stale = now.hp_max > 0 && now.hp <= 0;
        if now.dead || stale || now.casting || now.window || now.invisible || self.on.is_empty() {
            return None;
        }
        if self.last.is_some_and(|t| now.ms - t < GAP_MS) {
            return None;
        }
        let fighting = now.attacking || now.foe.is_some() && self.fights.values().any(|f| [f.on_me, f.on_friend, f.by_us].into_iter().flatten().any(|t| now.ms - t < FIGHT_MS));
        let shown = |slot: i64| (0..5 * now.bars.max(1) as i64).contains(&slot) || (252..256).contains(&slot);
        let mut ready: Vec<(&Ability, Kind)> = self
            .on
            .iter()
            .filter_map(|(id, &k)| abilities.get(id).map(|a| (a, k)))
            .filter(|(a, _)| a.valid != 0 && shown(a.slot) && a.lockout <= 0 && (a.power_cost <= 0 || now.power >= a.power_cost as i32))
            .filter(|(a, _)| self.tries.get(&a.id).is_none_or(|t| now.ms >= t.next && t.watch.is_none()))
            .collect();
        // the bars' order: the first bar's slots, then the rest
        ready.sort_by_key(|(a, _)| a.slot);
        let wanted = |a: &Ability, k: Kind| match k {
            Kind::Heal => now.hp_max > 0 && now.hp * 100 < HEAL_PCT * now.hp_max && !now.effects.iter().any(|e| effect_is(e, &a.name)),
            Kind::Pet => !now.has_pet,
            Kind::Buff => {
                let t = self.tries.get(&a.id);
                // a buff on the pet only with a pet out; up, or held for its
                // length (begun, not seen), or just sent and not yet landed:
                // not again
                !(a.scope == 3 && !now.has_pet)
                    && !self.buff_up(a, now)
                    && !t.and_then(|t| t.held).is_some_and(|h| now.ms < h)
                    && !t.is_some_and(|t| now.ms - t.sent < (a.cast_time.clamp(0, 30) * 1000).max(0) + SEE_MS)
            }
            Kind::Attack => fighting && now.foe.is_some_and(|f| a.range <= 0.0 || f.distance <= a.range + 1.0),
        };
        let order: &[Kind] = if fighting { &[Kind::Heal, Kind::Pet, Kind::Attack, Kind::Buff] } else { &[Kind::Heal, Kind::Pet, Kind::Buff, Kind::Attack] };
        let (a, kind) = order.iter().find_map(|&k| ready.iter().find(|(a, kk)| *kk == k && wanted(a, k)).copied())?;
        let cast_ms = (a.cast_time.clamp(0, 30) * 1000).max(0);
        let watch = matches!(kind, Kind::Buff | Kind::Pet).then_some(now.ms + cast_ms + SEE_MS);
        let misses = self.tries.get(&a.id).map_or(0, |t| t.misses);
        self.tries.insert(a.id, Try { next: now.ms + RETRY_MS, watch, misses, sent: now.ms, ..Default::default() });
        // a buff cast again before its end begins again
        if kind == Kind::Buff
            && let Some(e) = now.effects.iter().find(|e| effect_is(e, &a.name))
        {
            self.since.insert(e.clone(), Some(now.ms + cast_ms));
        }
        self.last = Some(now.ms);
        Some(Cast { id: a.id, kind, at_self: kind != Kind::Attack })
    }

    /// Auto targeting: the creature to target now, if any: with `target`
    /// none or dead, of the creatures `seen` that hit the player lately, the
    /// nearest, else of those that hit its pet or group, the nearest.
    pub fn pick_target(&mut self, ms: i64, target_gone: bool, seen: &[Seen]) -> Option<u32> {
        if !target_gone || self.targeted.is_some_and(|t| ms - t < TARGET_GAP_MS) {
            return None;
        }
        let recent = |t: Option<i64>| t.is_some_and(|t| ms - t < FIGHT_MS);
        let near = |pick: &dyn Fn(&Fight) -> bool| {
            seen.iter()
                .filter(|s| s.npc && s.alive && self.fights.get(&s.id).is_some_and(pick))
                .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal))
                .map(|s| s.id)
        };
        let id = near(&|f| recent(f.on_me)).or_else(|| near(&|f| recent(f.on_friend)))?;
        self.targeted = Some(ms);
        Some(id)
    }

    /// ○ or E pressed (`down`) on the HUD's selection, ability `id` (none:
    /// not an ability), or let go.
    pub fn hold(&mut self, id: Option<i64>, down: bool, ms: i64) {
        self.hold = match (down, id) {
            (true, Some(id)) => Some((id, ms, false)),
            _ => None,
        };
    }

    /// The hold a second on: its ability, once.
    pub fn held_long(&mut self, ms: i64) -> Option<i64> {
        let (id, since, done) = self.hold.as_mut()?;
        if *done || ms - *since < HOLD_MS {
            return None;
        }
        *done = true;
        Some(*id)
    }
}

/// ○ or E held this long on a slot turns its auto.
pub const HOLD_MS: i64 = 1000;

/// A character's line in the file: `<server id>:<name>` then `<id>=<kind>`
/// for each auto slot, tab-separated.
fn kept_of(text: &str, server_id: i64, name: &str) -> BTreeMap<i64, Kind> {
    let me = format!("{server_id}:{name}");
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        if parts.next() != Some(me.as_str()) {
            continue;
        }
        for p in parts {
            if let Some((id, k)) = p.split_once('=')
                && let (Ok(id), Some(k)) = (id.parse::<i64>(), Kind::parse(k))
            {
                out.insert(id, k);
            }
        }
    }
    out
}

/// The file with this character's line made `on` (dropped when empty),
/// the others' as they are.
fn with_kept(text: &str, server_id: i64, name: &str, on: &BTreeMap<i64, Kind>) -> String {
    let me = format!("{server_id}:{name}");
    let mut out: String = text.lines().filter(|l| l.split('\t').next() != Some(me.as_str()) && !l.trim().is_empty()).map(|l| format!("{l}\n")).collect();
    if !on.is_empty() {
        out.push_str(&me);
        for (id, k) in on {
            out.push_str(&format!("\t{id}={}", k.word()));
        }
        out.push('\n');
    }
    out
}

/// `/autocast`'s words.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// no word: the list
    List,
    /// `on`, `off`: the whole system (the Extended row)
    System(bool),
    /// `clear`: every slot off
    Clear,
    /// a slot by its number on the bars (1-20) or an ability by name, and a
    /// kind (none: turned as a right click turns it; `off`: off)
    Slot { which: String, kind: Option<Option<Kind>> },
    Help,
}

pub const HELP: &str = "/autocast: your auto slots; /autocast <slot 1-20 or name> [attack|buff|pet|heal|off]; /autocast on|off|clear. Right-click a quick-bar slot (long-press on touch, hold ○ or E a second) to turn it.";

pub fn command(arg: &str) -> Command {
    let arg = arg.trim();
    match arg.to_ascii_lowercase().as_str() {
        "" | "list" => return Command::List,
        "on" => return Command::System(true),
        "off" => return Command::System(false),
        "clear" => return Command::Clear,
        "help" | "?" => return Command::Help,
        _ => {}
    }
    let (which, last) = match arg.rsplit_once(' ') {
        Some((w, l)) => (w.trim(), Some(l)),
        None => (arg, None),
    };
    let kind = last.and_then(|l| if l.eq_ignore_ascii_case("off") { Some(None) } else { Kind::parse(l).map(Some) });
    match kind {
        Some(k) => Command::Slot { which: which.to_string(), kind: Some(k) },
        None => Command::Slot { which: arg.to_string(), kind: None },
    }
}

/// The ability `which` names: a number is a bar slot (1-20, the first bar's
/// first slot 1), else a name, whole, then its start (case aside).
pub fn find<'a>(abilities: &'a BTreeMap<i64, Ability>, which: &str) -> Option<&'a Ability> {
    let on_bar = || abilities.values().filter(|a| a.valid != 0);
    if let Ok(n) = which.parse::<i64>() {
        return on_bar().find(|a| a.slot == n - 1);
    }
    let w = which.to_lowercase();
    on_bar().find(|a| a.name.to_lowercase() == w).or_else(|| on_bar().find(|a| a.name.to_lowercase().starts_with(&w))).or_else(|| abilities.values().find(|a| a.name.to_lowercase() == w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ab(id: i64, slot: i64, name: &str, desc: &str, scope: i64) -> Ability {
        Ability { id, slot, valid: 1, name: name.into(), desc: desc.into(), scope, power_cost: 10, cast_time: 2, range: 30.0, ..Default::default() }
    }

    fn book(list: &[Ability]) -> BTreeMap<i64, Ability> {
        list.iter().map(|a| (a.id, a.clone())).collect()
    }

    /// A player alive, in the world, full health, 100 power, out of a fight.
    fn calm(ms: i64) -> Now {
        Now { ms, hp: 100, hp_max: 100, power: 100, has_pet: true, bars: 2, ..Default::default() }
    }

    fn auto(on: &[(i64, Kind)]) -> AutoCast {
        AutoCast { on: on.iter().copied().collect(), ..Default::default() }
    }

    #[test]
    fn the_guess_reads_the_scope_and_the_words() {
        let g = |name: &str, desc: &str, scope: i64| guess(&ab(1, 0, name, desc, scope));
        assert_eq!(g("Fiery Burst", "Target, Decrease Hitpoints by 84", 1), Some(Kind::Attack));
        assert_eq!(g("Whirling Blades +", "CM: Lowers the target's armor. A slashing strike at the target. Modified by STR. Resisted by SLASH.", 1), Some(Kind::Attack));
        assert_eq!(g("Life Tap", "Drains the target's life force, dealing damage and restoring health to the caster.", 1), Some(Kind::Attack));
        assert_eq!(g("Group Focus", "Group Buff DEX and WIS by 15, 30min", 2), Some(Kind::Buff));
        assert_eq!(g("Deft Hands", "Self, Increase DEX by 1, 30min", 0), Some(Kind::Buff));
        assert_eq!(g("Shroud of Hate", "Defensive Proc", 0), Some(Kind::Buff));
        assert_eq!(g("Alesthenies' Mercy", "CM: A buff that increases the target's armor.", 1), Some(Kind::Buff));
        assert_eq!(g("Healing", "Heals the target. Modified by CHA.", 1), Some(Kind::Heal));
        assert_eq!(g("Mend", "Heal some of your wounds. Modified by WIS.", 0), Some(Kind::Heal));
        assert_eq!(g("Fire Elementaling", "MAG Pet Summon - Fire Elemental to fight by your side.", 0), Some(Kind::Pet));
        assert_eq!(g("Decaying Flesh", "Summons a zombie to aid you.", 0), Some(Kind::Pet));
        assert_eq!(g("Primal Spirit", "Summons Pet", 0), Some(Kind::Pet));
        // go-eqoa-server's imported spells with no description: the name's words
        assert_eq!(g("Spirit Blast", "", 1), Some(Kind::Attack));
        assert_eq!(g("Shock of Spirits", "", 1), Some(Kind::Attack));
        assert_eq!(g("Group Deftness", "", 2), Some(Kind::Buff));
        assert_eq!(g("Deftness", "", 1), None, "a target's spell of no telling words: the player says");
        assert_eq!(g("Clay Golem", "Tank: Summons a clay golem that taunts enemies and restores its own health during battle.", 0), Some(Kind::Pet));
        // never guessed: travel, forms, invisibility, an item, a pet's own
        assert_eq!(g("Gate", "Self, Gate to bind point", 0), None);
        assert_eq!(g("Lionesswere", "Assume the form of the lionesswere.", 0), None);
        assert_eq!(g("Fiery Pebble", "Summons an Item, 3 charges", 0), None);
        assert_eq!(g("Ghostly Death", "Pet, Invis, 10min", 3), None);
        assert_eq!(g("Impart", "Pet Heal", 3), None);
        // an aura on the weapon is a buff, not a pet
        assert_eq!(g("Chilling Aura", "Summons an aura that surrounds your weapon, inflicting unholy damage.", 0), Some(Kind::Buff));
    }

    #[test]
    fn an_attack_fires_off_its_timer_in_a_fight_and_not_on_it() {
        let mut list = vec![ab(7, 0, "Fiery Burst", "Target, Decrease Hitpoints by 84", 1)];
        let mut c = auto(&[(7, Kind::Attack)]);
        let mut now = calm(10_000);
        now.attacking = true;
        now.foe = Some(Foe { distance: 10.0 });
        // control: ready, in a fight, a foe in range: it fires, at the target
        assert_eq!(c.choose(&book(&list), &now), Some(Cast { id: 7, kind: Kind::Attack, at_self: false }));
        // on its timer (the server's lockout, 0x00CD): nothing, however long
        list[0].lockout = 3300;
        let mut c = auto(&[(7, Kind::Attack)]);
        for t in [10_000, 20_000, 60_000] {
            now.ms = t;
            assert_eq!(c.choose(&book(&list), &now), None, "on its timer at {t}");
        }
        // off it again: it fires
        list[0].lockout = 0;
        assert!(c.choose(&book(&list), &now).is_some());
    }

    #[test]
    fn an_attack_waits_for_a_fight_a_foe_and_its_range() {
        let list = book(&[ab(7, 0, "Fiery Burst", "Target, Decrease Hitpoints by 84", 1)]);
        let now = calm(10_000);
        // no fight: never (it does not pull)
        let mut n = now.clone();
        n.foe = Some(Foe { distance: 5.0 });
        assert_eq!(auto(&[(7, Kind::Attack)]).choose(&list, &n), None, "a foe targeted but no fight");
        // a fight but the target no foe (none, or a friend)
        let mut n = now.clone();
        n.attacking = true;
        assert_eq!(auto(&[(7, Kind::Attack)]).choose(&list, &n), None, "no foe");
        // a foe out of range
        n.foe = Some(Foe { distance: 45.0 });
        assert_eq!(auto(&[(7, Kind::Attack)]).choose(&list, &n), None, "out of range");
        // control: in range
        n.foe = Some(Foe { distance: 25.0 });
        assert!(auto(&[(7, Kind::Attack)]).choose(&list, &n).is_some());
        // a fight told by a hit taken, not the attack held
        let mut c = auto(&[(7, Kind::Attack)]);
        c.hit(0x55, 0x10, 9_000, Some(0x10), &[]);
        let mut n = now.clone();
        n.foe = Some(Foe { distance: 5.0 });
        assert!(c.choose(&list, &n).is_some(), "hit a second ago: a fight");
        let mut c = auto(&[(7, Kind::Attack)]);
        c.hit(0x55, 0x10, 9_000 - FIGHT_MS, Some(0x10), &[]);
        assert_eq!(c.choose(&list, &n), None, "hit too long ago: none");
    }

    #[test]
    fn a_buff_fires_when_its_effect_is_gone_and_not_while_on() {
        let list = book(&[ab(3, 1, "Deft Hands", "Self, Increase DEX by 1, 30min", 0)]);
        let mut now = calm(10_000);
        now.effects = vec!["Deft Hands".into()];
        let mut c = auto(&[(3, Kind::Buff)]);
        c.effects(&now.effects, now.ms);
        assert_eq!(c.choose(&list, &now), None, "on the player: none");
        now.effects.clear();
        now.ms += 5000;
        c.effects(&now.effects, now.ms);
        assert_eq!(c.choose(&list, &now), Some(Cast { id: 3, kind: Kind::Buff, at_self: true }), "gone: cast at the player");
    }

    #[test]
    fn a_long_buff_is_cast_again_before_its_end_once_its_length_is_seen() {
        let list = book(&[ab(3, 1, "Group Focus", "Group Buff DEX and WIS by 15, 30min", 2)]);
        let mut c = auto(&[(3, Kind::Buff)]);
        let on = vec!["Group Focus".to_string()];
        // seen from its start to its end: 120 s
        c.effects(&[], 0);
        c.effects(&on, 1000);
        c.effects(&[], 121_000);
        assert_eq!(c.lasted.get("Group Focus"), Some(&120_000));
        // on again from 200 s: held until 6 s before 320 s
        c.effects(&on, 200_000);
        let mut now = calm(300_000);
        now.effects = on.clone();
        assert_eq!(c.choose(&list, &now), None, "far from its end");
        now.ms = 315_000;
        assert!(c.choose(&list, &now).is_some(), "within 6 s of its end: again");
        // and that cast began it again: not again right after
        now.ms = 330_000;
        assert_eq!(c.choose(&list, &now), None, "begun again by the cast");
    }

    #[test]
    fn a_summoning_fires_with_no_pet_only() {
        let list = book(&[ab(9, 2, "Fire Elementaling", "MAG Pet Summon", 0)]);
        let mut now = calm(10_000);
        assert_eq!(auto(&[(9, Kind::Pet)]).choose(&list, &now), None, "a pet out");
        now.has_pet = false;
        assert_eq!(auto(&[(9, Kind::Pet)]).choose(&list, &now), Some(Cast { id: 9, kind: Kind::Pet, at_self: true }), "it died");
    }

    #[test]
    fn a_heal_fires_below_half_health() {
        let list = book(&[ab(4, 0, "Mend", "Heal some of your wounds.", 0)]);
        let mut now = calm(10_000);
        now.hp = 60;
        assert_eq!(auto(&[(4, Kind::Heal)]).choose(&list, &now), None, "60%");
        now.hp = 49;
        assert!(auto(&[(4, Kind::Heal)]).choose(&list, &now).is_some(), "49%");
    }

    #[test]
    fn dead_casting_a_window_invisible_or_short_of_power_block_every_slot() {
        let list = book(&[ab(4, 0, "Mend", "Heal", 0), ab(9, 1, "Fire Elementaling", "MAG Pet Summon", 0)]);
        let mut base = calm(10_000);
        base.hp = 10;
        base.has_pet = false;
        let on = [(4, Kind::Heal), (9, Kind::Pet)];
        assert!(auto(&on).choose(&list, &base).is_some(), "control: one fires");
        for (what, f) in [
            ("dead", (|n: &mut Now| n.dead = true) as fn(&mut Now)),
            ("casting", |n| n.casting = true),
            ("a window up", |n| n.window = true),
            ("invisible", |n| n.invisible = true),
            ("no power", |n| n.power = 5),
            ("no hit points yet after a respawn", |n| n.hp = 0),
        ] {
            let mut n = base.clone();
            f(&mut n);
            assert_eq!(auto(&on).choose(&list, &n), None, "{what}");
        }
    }

    #[test]
    fn one_at_a_time_a_gap_apart_heal_first() {
        let list = book(&[ab(9, 0, "Fire Elementaling", "MAG Pet Summon", 0), ab(4, 1, "Mend", "Heal", 0)]);
        let mut c = auto(&[(4, Kind::Heal), (9, Kind::Pet)]);
        let mut now = calm(10_000);
        now.hp = 10;
        now.has_pet = false;
        assert_eq!(c.choose(&list, &now).map(|c| c.id), Some(4), "the heal before the pet, though the pet's slot is first");
        now.ms += GAP_MS - 1;
        assert_eq!(c.choose(&list, &now), None, "inside the gap");
        now.ms += 1;
        assert_eq!(c.choose(&list, &now).map(|c| c.id), Some(9), "then the pet");
        now.ms += GAP_MS - 1;
        assert_eq!(c.choose(&list, &now), None, "the heal not again before its retry");
        now.ms += 1;
        assert_eq!(c.choose(&list, &now).map(|c| c.id), Some(4), "after it (the server's lockout aside), while still low; the pet's outcome still awaited");
    }

    #[test]
    fn a_buff_that_never_shows_is_tried_later_and_then_taken_off() {
        let list = book(&[ab(3, 0, "Odd", "Self, buff", 0)]);
        let mut c = auto(&[(3, Kind::Buff)]);
        let mut now = calm(0);
        let mut casts = 0;
        for t in (0..400_000).step_by(500) {
            now.ms = t;
            if c.choose(&list, &now).is_some() {
                casts += 1;
            }
        }
        assert_eq!(casts, MISSES_OFF as usize, "tried {MISSES_OFF} times");
        assert!(!c.on.contains_key(&3), "then off");
        assert_eq!(c.said.len(), 1);
        // control: one whose effect shows stays on
        let mut c = auto(&[(3, Kind::Buff)]);
        now.ms = 0;
        assert!(c.choose(&list, &now).is_some());
        now.effects = vec!["Odd".into()];
        now.ms = 3000;
        c.choose(&list, &now);
        assert!(c.on.contains_key(&3) && c.tries[&3].misses == 0);
    }

    /// A buff on the pet (scope 3) guessed a buff; a pet's heal and its
    /// invisibility not.
    #[test]
    fn a_pet_buff_is_guessed_a_buff() {
        let g = |name: &str, desc: &str| guess(&ab(1, 0, name, desc, 3));
        assert_eq!(g("Elemental Mark", "Pet, Increase INT by 75, Add Offensive Proc, Add DS Proc, 10min"), Some(Kind::Buff));
        assert_eq!(g("Excite", "Empowers your pet, increasing its dexterity, stamina, and strength while restoring health over time. Modified by AGI."), Some(Kind::Buff));
        assert_eq!(g("Burning Mark", ""), Some(Kind::Buff));
        assert_eq!(g("Infuse", "Pet Heal"), None);
        assert_eq!(g("Renewal", "Pet, Increase Hitpoints by 539"), None);
        assert_eq!(g("Fusion", "Pet, Heal Hp by 722"), None);
        assert_eq!(g("Repair", "CM: Heals your pet. Modified by AGI."), None);
        assert_eq!(g("Ghostly Death", "Pet, Invis, 10min"), None);
        assert_eq!(Kind::Buff.when_for(&ab(1, 0, "Elemental Mark", "", 3)), "when its effect is gone from your pet");
    }

    #[test]
    fn a_buffs_length_is_read_off_its_description() {
        assert_eq!(length("Pet, Increase STR & DEX by 10, 5min"), Some(300_000));
        assert_eq!(length("Pet, Increase INT by 75, Add Offensive Proc, Add DS Proc, 10min"), Some(600_000));
        assert_eq!(length("increasing strength and dexterity for thirty minutes."), Some(1_800_000));
        assert_eq!(length("Self, lasts 1 hour"), Some(3_600_000));
        assert_eq!(length("Group Buff DEX and WIS by 15, 30m"), Some(1_800_000));
        assert_eq!(length("Pet Heal"), None);
        assert_eq!(length("Power cost: 0, Cast: 0, Recast: 0."), None);
    }

    /// Petition 1616: a buff on the pet, its effect never on the caster's
    /// bar, on a server that sends no pet's effects. Begun by the server (its
    /// lockout came): held for its length (the description's 5 minutes), then
    /// cast again, and never switched off; another pet ends the hold; with no
    /// pet, none. The control: a cast the server never began (no lockout)
    /// still counts a miss, and three switch the slot off.
    #[test]
    fn a_pet_buff_begun_waits_its_length_and_is_never_switched_off() {
        let mut list = vec![ab(5, 0, "Motivate", "Pet, Increase STR & DEX by 10, 5min", 3)];
        let mut c = auto(&[(5, Kind::Buff)]);
        let mut now = calm(10_000);
        now.pet = Some(0x400);
        assert_eq!(c.choose(&book(&list), &now), Some(Cast { id: 5, kind: Kind::Buff, at_self: true }), "the first cast");
        // the server began it: its lockout (0x00CD), 2 s cast + 1 s recast + 0.3
        list[0].lockout = 3300;
        now.ms = 10_100;
        assert_eq!(c.choose(&book(&list), &now), None);
        list[0].lockout = 0;
        let mut casts = vec![10_000];
        for t in (10_500..1_000_000).step_by(500) {
            now.ms = t;
            if c.choose(&book(&list), &now).is_some() {
                casts.push(t);
                list[0].lockout = 3300;
                now.ms = t + 100;
                c.choose(&book(&list), &now);
                list[0].lockout = 0;
            }
        }
        assert!(c.on.contains_key(&5) && c.said.is_empty(), "never switched off");
        assert_eq!(casts.len(), 4, "every 5 minutes and its cast: {casts:?}");
        for w in casts.windows(2) {
            let gap = w[1] - w[0];
            assert!((300_000..=305_000).contains(&gap), "waited its length: {gap}");
        }
        // another pet: the hold ends at once
        let mut c = auto(&[(5, Kind::Buff)]);
        now.ms = 0;
        c.choose(&book(&list), &now);
        list[0].lockout = 3300;
        now.ms = 100;
        c.choose(&book(&list), &now);
        list[0].lockout = 0;
        now.ms = 20_000;
        assert_eq!(c.choose(&book(&list), &now), None, "held");
        now.pet = Some(0x401);
        now.ms = 21_000;
        assert!(c.choose(&book(&list), &now).is_some(), "a new pet: buffed");
        // no pet: none
        let mut c = auto(&[(5, Kind::Buff)]);
        now.pet = None;
        now.has_pet = false;
        assert_eq!(c.choose(&book(&list), &now), None, "no pet");
        // refused (no lockout ever came): misses, and off after three
        let mut c = auto(&[(5, Kind::Buff)]);
        now.pet = Some(0x400);
        now.has_pet = true;
        let mut tries = 0;
        for t in (0..400_000).step_by(500) {
            now.ms = t;
            if c.choose(&book(&list), &now).is_some() {
                tries += 1;
            }
        }
        assert_eq!(tries, MISSES_OFF as usize);
        assert!(!c.on.contains_key(&5) && c.said.len() == 1, "{:?}", c.said);
    }

    /// A buff on the pet with the pet's effects sent (0x7E33): not cast while
    /// on it with time left, cast again within `REFRESH_BEFORE` of its end or
    /// when it is gone; seen there, a success.
    #[test]
    fn a_pet_buff_follows_the_pets_effects() {
        let list = book(&[ab(5, 0, "Motivate", "Pet, Increase STR & DEX by 10, 5min", 3)]);
        let mut c = auto(&[(5, Kind::Buff)]);
        let mut now = calm(10_000);
        now.pet = Some(0x400);
        now.pet_effects = Some(vec![("Motivate".into(), 200_000)]);
        assert_eq!(c.choose(&list, &now), None, "on the pet");
        now.pet_effects = Some(vec![("Motivate".into(), REFRESH_BEFORE - 1000)]);
        assert!(c.choose(&list, &now).is_some(), "near its end");
        // landed again: seen on the pet, no miss
        now.pet_effects = Some(vec![("Motivate".into(), 300_000)]);
        now.ms += 4000;
        assert_eq!(c.choose(&list, &now), None);
        assert_eq!(c.tries[&5].misses, 0);
        assert!(c.tries[&5].watch.is_none());
        now.pet_effects = Some(vec![]);
        now.ms += 20_000;
        assert!(c.choose(&list, &now).is_some(), "gone off the pet");
    }

    #[test]
    fn only_slots_on_the_bars_shown_fire() {
        let mut a = ab(4, 7, "Mend", "Heal", 0);
        let mut now = calm(0);
        now.hp = 10;
        now.bars = 1;
        assert_eq!(auto(&[(4, Kind::Heal)]).choose(&book(std::slice::from_ref(&a)), &now), None, "slot 8 with one bar");
        now.bars = 2;
        assert!(auto(&[(4, Kind::Heal)]).choose(&book(std::slice::from_ref(&a)), &now).is_some(), "with two");
        a.valid = 0;
        assert_eq!(auto(&[(4, Kind::Heal)]).choose(&book(&[a]), &now), None, "off the bars");
    }

    #[test]
    fn auto_targeting_picks_the_attacker_and_never_a_neutral_or_a_player() {
        let me = Some(0x10);
        let mut c = AutoCast::default();
        // 0x50 hit the pet, 0x51 hit the player, 0x52 is near but hit no one,
        // 0x53 a player who hit the player (a duel's), 0x54 dead
        c.hit(0x50, 0x20, 1000, me, &[0x20]);
        c.hit(0x51, 0x10, 1000, me, &[0x20]);
        c.hit(0x53, 0x10, 1000, me, &[0x20]);
        c.hit(0x54, 0x10, 1000, me, &[0x20]);
        let seen = [
            Seen { id: 0x50, npc: true, alive: true, distance: 3.0 },
            Seen { id: 0x51, npc: true, alive: true, distance: 9.0 },
            Seen { id: 0x52, npc: true, alive: true, distance: 1.0 },
            Seen { id: 0x53, npc: false, alive: true, distance: 2.0 },
            Seen { id: 0x54, npc: true, alive: false, distance: 1.0 },
        ];
        assert_eq!(c.pick_target(2000, false, &seen), None, "a target held: none");
        assert_eq!(c.pick_target(2000, true, &seen), Some(0x51), "the one on the player, over the nearer one on the pet");
        // without it, the pet's
        let mut c2 = AutoCast { fights: c.fights.clone(), ..Default::default() };
        assert_eq!(c2.pick_target(2000, true, &seen[..1]), Some(0x50));
        // only the neutral, the player and the dead: none
        let mut c3 = AutoCast { fights: c.fights.clone(), ..Default::default() };
        assert_eq!(c3.pick_target(2000, true, &seen[2..]), None);
        // a hit long ago: none
        assert_eq!(AutoCast { fights: c.fights.clone(), ..Default::default() }.pick_target(1000 + FIGHT_MS, true, &seen), None);
        // one a second
        assert_eq!(c.pick_target(2500, true, &seen), None);
        assert_eq!(c.pick_target(3000, true, &seen), Some(0x51));
    }

    #[test]
    fn the_file_keeps_each_characters_line() {
        let mut on = BTreeMap::new();
        on.insert(7, Kind::Attack);
        on.insert(3, Kind::Buff);
        let t = with_kept("1:Other\t5=pet\n", 2, "Dab", &on);
        assert_eq!(kept_of(&t, 2, "Dab"), on);
        assert_eq!(kept_of(&t, 1, "Other").get(&5), Some(&Kind::Pet), "the other's kept");
        let t = with_kept(&t, 2, "Dab", &BTreeMap::new());
        assert!(kept_of(&t, 2, "Dab").is_empty() && t.contains("1:Other"));
    }

    #[test]
    fn the_command_reads_its_words() {
        assert_eq!(command(""), Command::List);
        assert_eq!(command("off"), Command::System(false));
        assert_eq!(command("3"), Command::Slot { which: "3".into(), kind: None });
        assert_eq!(command("Fiery Burst attack"), Command::Slot { which: "Fiery Burst".into(), kind: Some(Some(Kind::Attack)) });
        assert_eq!(command("Fiery Burst off"), Command::Slot { which: "Fiery Burst".into(), kind: Some(None) });
        assert_eq!(command("Spirit of the Wolf"), Command::Slot { which: "Spirit of the Wolf".into(), kind: None });
        let list = book(&[ab(7, 2, "Fiery Burst", "", 1)]);
        assert_eq!(find(&list, "3").map(|a| a.id), Some(7));
        assert_eq!(find(&list, "fiery").map(|a| a.id), Some(7));
        assert!(find(&list, "4").is_none());
    }

    #[test]
    fn effect_names_match_cut_at_31() {
        assert!(effect_is("Deft Hands", "deft hands"));
        let long = "A Very Long Spell Name Of Many Words";
        assert!(effect_is(&long.chars().take(31).collect::<String>(), long));
        assert!(!effect_is("A Very", long));
        assert!(!effect_is("", "x"));
    }
}
