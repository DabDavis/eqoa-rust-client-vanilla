//! The community's markers on the map (the user, 2026-10-26: "Players can
//! submit waypoints to be added to the in-game map. Submissions would require
//! official approval, similar to our spellfx"; the user's choices: one list
//! for every player of this client on any server, Sandstorm's too, kept on
//! eqoaclassic, suggested from the map). The list is go-eqoa-server's on
//! eqoaclassic.com (`BASE`; `EQOA_MAPMARKS=<base>` another): `GET mapmarks`
//! the markers a GM accepted, asked when the map first opens in a run and
//! every ten minutes while it is up, with the last answer's ETag
//! (`If-None-Match`: a 304 keeps the list), and kept in the map's cache
//! (`store`, "community") so a start with no network shows the last one; `POST
//! mapmarks/suggest` a player's suggestion (a spot of the map's world, a name,
//! a note), which a GM accepts or dismisses in the GM panel, as the spell
//! visuals' are. The list is asked of eqoaclassic.com whatever the game server
//! is; nothing is sent but a suggestion the player typed a name for and gave
//! Enter. Natively a request is a thread's (ureq), in a page the browser's
//! fetch; a frame never waits for one.
//!
//! On the map each marker is a pin of its kind's colour at its place (its tip
//! on the spot), its name beside it in close, with a card under the pointer
//! as the camps' and coaches' have; Map Markers (X, ○) shows and hides them
//! with those. L3 (N on a keyboard, the map's Suggest button) suggests one:
//! the map hands Play the spot (`Map::take_suggest`), Play's edit box asks a
//! name and a note (`play::mapsuggest`), and the map says what the list
//! answered for a few seconds (`notice`).

use super::store::{self, Got, Store};
use super::view::View;
use render::ui::{Quad, Ui};
use web_time::{Duration, Instant};

/// The list's home: eqoaclassic's account server, through its web server.
pub const BASE: &str = "https://eqoaclassic.com/eqoa/account/api/";
/// How often the list is asked again while the map is up.
pub const EVERY: Duration = Duration::from_secs(600);
/// A suggestion's name and note, in characters (the server refuses past
/// them; the client cuts them so).
pub const NAME_MIN: usize = 2;
pub const NAME_MAX: usize = 40;
pub const NOTE_MAX: usize = 200;
/// The edit box's purposes for a suggestion's name and note (the client's
/// own, `play::mapsuggest`; no game window uses them).
pub const NAME_PURPOSE: u32 = 0x7E30_0001;
pub const NOTE_PURPOSE: u32 = 0x7E30_0002;
/// How long the map shows what the list answered a suggestion.
pub const SHOWN: Duration = Duration::from_secs(6);
/// What the map says once a suggestion is in.
pub const SENT: &str = "Sent for a GM's approval.";
/// Where the last list is kept (`store`), and as what.
const CACHE: &str = "community";
const CACHE_FILE: &str = "marks.json";
/// The most of a list read, and of a suggestion's answer.
const LIST_MOST: u64 = 4 * 1024 * 1024;
const ANSWER_MOST: u64 = 64 * 1024;

/// What a marker marks: its colour, and its card's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Landmark,
    Camp,
    Quest,
    Merchant,
    Travel,
    Danger,
    Other,
}

impl Kind {
    /// The list's word for it; an unknown one is `Other`.
    pub fn of(s: &str) -> Kind {
        match s.trim().to_ascii_lowercase().as_str() {
            "landmark" => Kind::Landmark,
            "camp" => Kind::Camp,
            "quest" => Kind::Quest,
            "merchant" => Kind::Merchant,
            "travel" => Kind::Travel,
            "danger" => Kind::Danger,
            _ => Kind::Other,
        }
    }

    /// Its card's words.
    pub fn words(self) -> &'static str {
        match self {
            Kind::Landmark => "A landmark",
            Kind::Camp => "A camp",
            Kind::Quest => "A quest's place",
            Kind::Merchant => "A merchant",
            Kind::Travel => "A way to travel",
            Kind::Danger => "A danger",
            Kind::Other => "A place",
        }
    }

    /// Its pin's colour: none the player's gold dart, the waypoint's magenta,
    /// the group's compass colours or the XP badge's red is.
    pub fn color(self) -> [f32; 4] {
        match self {
            Kind::Landmark => [0.3, 0.82, 0.76, 1.0],
            Kind::Camp => [0.95, 0.55, 0.35, 1.0],
            Kind::Quest => [0.98, 0.9, 0.55, 1.0],
            Kind::Merchant => [0.55, 0.85, 0.4, 1.0],
            Kind::Travel => [0.55, 0.65, 1.0, 1.0],
            Kind::Danger => [0.92, 0.25, 0.3, 1.0],
            Kind::Other => [0.82, 0.8, 0.76, 1.0],
        }
    }
}

/// A marker of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub id: i64,
    /// `gWorldInfo`'s index
    pub world: i32,
    /// x and z, as the map's readout names a place
    pub at: [f32; 2],
    pub name: String,
    pub note: String,
    pub kind: Kind,
}

impl Mark {
    /// Its card: the name, the note, and what it is (`markers::draw_lines_card`).
    pub fn card(&self) -> Vec<(String, [f32; 4])> {
        let mut out = vec![(self.name.clone(), super::markers::TITLE)];
        if !self.note.trim().is_empty() {
            out.push((self.note.clone(), super::markers::TEXT));
        }
        out.push((format!("{}, a community marker", self.kind.words()), super::markers::DROPS));
        out
    }
}

/// The list's body (`GET mapmarks`): its version and its markers; a marker
/// that does not read (no world, place or name) is passed over. None for a
/// body that is not the list.
pub fn parse(text: &str) -> Option<(String, Vec<Mark>)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let list = v.get("marks")?.as_array()?;
    let version = match v.get("version") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let marks = list
        .iter()
        .filter_map(|m| {
            let num = |k: &str| m.get(k)?.as_f64().filter(|x| x.is_finite());
            let text = |k: &str| m.get(k).and_then(|s| s.as_str()).map(|s| s.trim().to_string()).unwrap_or_default();
            let name = text("name");
            if name.is_empty() {
                return None;
            }
            Some(Mark { id: m.get("id").and_then(|i| i.as_i64()).unwrap_or(0), world: num("world")? as i32, at: [num("x")? as f32, num("z")? as f32], name, note: text("note"), kind: Kind::of(&text("kind")) })
        })
        .collect();
    Some((version, marks))
}

/// `s` trimmed and cut to `most` characters.
pub fn clip(s: &str, most: usize) -> String {
    s.trim().chars().take(most).collect::<String>().trim_end().to_string()
}

/// A suggestion's body (`POST mapmarks/suggest`).
pub fn suggest_body(spot: Spot, name: &str, note: &str, client: &str) -> String {
    serde_json::json!({ "world": spot.world, "x": spot.at[0], "z": spot.at[1], "name": clip(name, NAME_MAX), "note": clip(note, NOTE_MAX), "client": client }).to_string()
}

/// A suggestion's answer: its id when the list took it, else the server's
/// own sentence (or what went wrong, when it gave none).
pub fn answer(status: u16, text: &str) -> Result<i64, String> {
    let v: Option<serde_json::Value> = serde_json::from_str(text).ok();
    let field = |k: &str| v.as_ref().and_then(|v| v.get(k).cloned());
    if (200..300).contains(&status) && field("ok") == Some(serde_json::Value::Bool(true)) {
        return Ok(field("id").and_then(|i| i.as_i64()).unwrap_or(0));
    }
    Err(field("error").and_then(|e| e.as_str().map(str::to_string)).filter(|e| !e.trim().is_empty()).unwrap_or_else(|| format!("The marker list answered {status}: nothing was kept.")))
}

/// This build, as a suggestion names it (`client`): `dev-<commit>` natively
/// (a build of one's own `local`), `web-<build>` in a page.
pub fn client() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let build = web_sys::window().and_then(|w| js_sys::Reflect::get(&w, &"eqoaBuild".into()).ok()).and_then(|b| b.as_string());
        format!("web-{}", build.unwrap_or_else(|| "none".into()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::update::COMMIT.map_or_else(|| "local".to_string(), |c| format!("dev-{}", &c[..c.len().min(7)]))
    }
}

/// A spot to suggest: its world (`gWorldInfo`'s index) and place (x, z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub world: i32,
    pub at: [f32; 2],
}

/// A suggestion being typed: its spot, its name once given, and whether the
/// map was following the player (it comes back so).
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub spot: Spot,
    pub name: Option<String>,
    pub follow: bool,
}

/// What an ask of the list brought.
#[derive(Debug, PartialEq)]
enum Fetched {
    /// 304: the list held is the list
    Same,
    /// a list, and its ETag
    New(String, Option<String>),
}

/// An answer on its way: natively a thread's, in a page a promise's.
struct Pending<T> {
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<Result<T, String>>,
    #[cfg(target_arch = "wasm32")]
    slot: std::rc::Rc<std::cell::RefCell<Option<Result<T, String>>>>,
}

impl<T: 'static> Pending<T> {
    /// The answer, once it is here.
    fn poll(&mut self) -> Option<Result<T, String>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the request stopped".into())),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.slot.borrow_mut().take()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_thread<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Pending<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new().name("mapmarks".into()).spawn(move || {
        let _ = tx.send(f());
    });
    Pending { rx }
}

#[cfg(target_arch = "wasm32")]
fn on_promise<T: 'static>(f: impl std::future::Future<Output = Result<T, String>> + 'static) -> Pending<T> {
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let s = slot.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let r = f.await;
        *s.borrow_mut() = Some(r);
    });
    Pending { slot }
}

/// The two requests, natively.
#[cfg(not(target_arch = "wasm32"))]
mod http {
    use super::{ANSWER_MOST, Fetched, LIST_MOST};
    use std::time::Duration;

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .user_agent(format!("eqoa-vanilla/{}", super::client()))
            .build()
            .into()
    }

    pub fn get(url: &str, etag: Option<String>) -> Result<Fetched, String> {
        let mut req = agent().get(url);
        if let Some(e) = &etag {
            req = req.header("If-None-Match", e);
        }
        let mut r = req.call().map_err(|e| format!("{url}: {e}"))?;
        match r.status().as_u16() {
            304 => Ok(Fetched::Same),
            200 => {
                let etag = r.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_string);
                let body = r.body_mut().with_config().limit(LIST_MOST).read_to_string().map_err(|e| format!("{url}: {e}"))?;
                Ok(Fetched::New(body, etag))
            }
            s => Err(format!("{url} answered {s}")),
        }
    }

    pub fn post(url: &str, body: String) -> Result<(u16, String), String> {
        let mut r = agent().post(url).header("Content-Type", "application/json").send(body).map_err(|e| format!("the marker list could not be reached ({e})"))?;
        let status = r.status().as_u16();
        Ok((status, r.body_mut().with_config().limit(ANSWER_MOST).read_to_string().unwrap_or_default()))
    }
}

/// The two requests in a page: the browser's own `fetch`, called through
/// `Reflect` (no request type of web-sys's needed). The list is asked with
/// `cache: "no-cache"`, so the browser revalidates it with its own ETag and
/// hands back the list it holds on a 304.
#[cfg(target_arch = "wasm32")]
mod http {
    use super::Fetched;
    use wasm_bindgen::{JsCast, JsValue};

    async fn fetch(url: &str, init: js_sys::Object) -> Result<(u16, String), String> {
        let err = |e: JsValue| format!("{url}: {}", e.as_string().unwrap_or_else(|| format!("{e:?}")));
        let w = web_sys::window().ok_or("no window")?;
        let f: js_sys::Function = js_sys::Reflect::get(&w, &"fetch".into()).map_err(err)?.dyn_into().map_err(|_| "no fetch".to_string())?;
        let p: js_sys::Promise = f.call2(&w, &url.into(), &init).map_err(err)?.dyn_into().map_err(|_| "no promise".to_string())?;
        let r: web_sys::Response = wasm_bindgen_futures::JsFuture::from(p).await.map_err(err)?.dyn_into().map_err(|_| "not a response".to_string())?;
        let status = r.status();
        let text = wasm_bindgen_futures::JsFuture::from(r.text().map_err(err)?).await.map_err(err)?.as_string().unwrap_or_default();
        Ok((status, text))
    }

    fn set(o: &js_sys::Object, k: &str, v: &JsValue) {
        let _ = js_sys::Reflect::set(o, &k.into(), v);
    }

    pub async fn get(url: String) -> Result<Fetched, String> {
        let init = js_sys::Object::new();
        set(&init, "cache", &"no-cache".into());
        match fetch(&url, init).await? {
            (200, body) => Ok(Fetched::New(body, None)),
            (s, _) => Err(format!("{url} answered {s}")),
        }
    }

    pub async fn post(url: String, body: String) -> Result<(u16, String), String> {
        let init = js_sys::Object::new();
        set(&init, "method", &"POST".into());
        let headers = js_sys::Object::new();
        set(&headers, "Content-Type", &"application/json".into());
        set(&init, "headers", &headers);
        set(&init, "body", &body.into());
        fetch(&url, init).await.map_err(|e| format!("the marker list could not be reached ({e})"))
    }
}

/// The list as the map has it, its asks and a suggestion's.
#[derive(Default)]
pub struct Community {
    pub marks: Vec<Mark>,
    /// the list's version and the ETag it came with
    version: String,
    etag: Option<String>,
    /// a list came from the server this run (the cache's is then older)
    fresh: bool,
    /// the marker whose card a finger tapped up (`Map::click`)
    pub pinned: Option<usize>,
    /// where the list is (`base`), once worked out
    base: Option<Option<String>>,
    asked: Option<Instant>,
    fetching: Option<Pending<Fetched>>,
    sending: Option<Pending<(u16, String)>>,
    /// what the last failed ask said (said once until it changes)
    said: String,
    /// the cache (`store`), read the first time the map is up
    pub cache: Option<Box<dyn Store>>,
    cache_read: bool,
    /// what the map says for a while: a suggestion's answer, and when
    notice: Option<(String, Instant)>,
    /// the suggestion being typed (`play::mapsuggest`)
    pub draft: Option<Draft>,
    /// a spot the map asks Play to suggest (`Map::take_suggest`)
    spot: Option<Spot>,
}

impl Community {
    /// Where the list is asked: `EQOA_MAPMARKS` (a test's server), else
    /// eqoaclassic's (`BASE`); none offscreen with neither (a script's run
    /// asks nothing).
    fn base(&mut self) -> Option<String> {
        self.base
            .get_or_insert_with(|| match std::env::var("EQOA_MAPMARKS") {
                Ok(b) if !b.trim().is_empty() => Some(b),
                _ if std::env::var_os("EQOA_OFFSCREEN").is_some() => None,
                _ => Some(BASE.to_string()),
            })
            .clone()
            .map(|b| if b.ends_with('/') { b } else { format!("{b}/") })
    }

    /// Where the list is asked, set (a tool's or a test's; none: nowhere).
    pub fn set_base(&mut self, base: Option<String>) {
        self.base = Some(base);
    }

    /// The markers of world `world` (`gWorldInfo`'s index), with their indices.
    pub fn of_world(&self, world: i32) -> impl Iterator<Item = (usize, &Mark)> {
        self.marks.iter().enumerate().filter(move |(_, m)| m.world == world)
    }

    /// The marker under window pixel `s` of view `v` in a `size` window, of
    /// world `world`: the nearest whose pin's head (`head`, `r` its size) is
    /// within `reach` window pixels.
    pub fn under(&self, world: i32, v: View, size: (u32, u32), s: [f32; 2], reach: f32, r: f32) -> Option<usize> {
        self.of_world(world)
            .map(|(i, m)| {
                let p = head(v.to_window(size, m.at), r);
                (i, (p[0] - s[0]).hypot(p[1] - s[1]))
            })
            .filter(|&(_, d)| d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The list taken (a fresh one or the cache's): a new one lets a tapped
    /// card go.
    fn take(&mut self, version: String, marks: Vec<Mark>, etag: Option<String>) {
        if self.marks != marks {
            self.pinned = None;
        }
        self.marks = marks;
        self.version = version;
        self.etag = etag;
    }

    /// One frame of the map's (`open`: it is up): the cache read the first
    /// time it is, the list asked then and every `EVERY` while it is, and
    /// whatever answered taken.
    pub fn frame(&mut self, open: bool) {
        if open && !self.cache_read {
            self.cache_read = true;
            if self.cache.is_none() {
                self.cache = store::open(CACHE);
            }
            if let Some(c) = self.cache.as_mut() {
                c.get(CACHE_FILE);
            }
        }
        let mut got = Vec::new();
        if let Some(c) = self.cache.as_mut() {
            while let Some(g) = c.poll() {
                got.push(g);
            }
        }
        for g in got {
            if let Got::File(name, Some(bytes)) = g
                && name == CACHE_FILE
                && !self.fresh
                && let Some((body, etag)) = unwrap_cache(&bytes)
                && let Some((version, marks)) = parse(&body)
            {
                crate::say(&format!("map: {} community markers kept from before (version {version})", marks.len()));
                self.take(version, marks, etag);
            }
        }
        if open && self.fetching.is_none() && self.asked.is_none_or(|t| t.elapsed() >= EVERY) {
            self.asked = Some(Instant::now());
            if let Some(base) = self.base() {
                let url = format!("{base}mapmarks");
                // a 304 is of no use without the list it stands for
                let etag = self.etag.clone().filter(|_| !self.version.is_empty() || !self.marks.is_empty());
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.fetching = Some(on_thread(move || http::get(&url, etag)));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = etag;
                    self.fetching = Some(on_promise(http::get(url)));
                }
            }
        }
        if let Some(r) = self.fetching.as_mut().and_then(Pending::poll) {
            self.fetching = None;
            match r {
                Ok(Fetched::Same) => self.said.clear(),
                Ok(Fetched::New(body, etag)) => match parse(&body) {
                    Some((version, marks)) => {
                        self.said.clear();
                        if version != self.version || marks != self.marks {
                            crate::say(&format!("map: {} community markers (version {version})", marks.len()));
                        }
                        self.fresh = true;
                        if let Some(c) = self.cache.as_mut() {
                            c.put(CACHE_FILE, wrap_cache(&body, etag.as_deref()));
                        }
                        self.take(version, marks, etag);
                    }
                    None => self.say("map: the community markers' list did not read"),
                },
                Err(e) => self.say(&format!("map: the community markers not asked ({e})")),
            }
        }
        if let Some(r) = self.sending.as_mut().and_then(Pending::poll) {
            self.sending = None;
            let line = match r.and_then(|(status, text)| answer(status, &text)) {
                Ok(id) => {
                    crate::say(&format!("map: a community marker suggested (number {id} of the list's)"));
                    SENT.to_string()
                }
                Err(e) => {
                    crate::say(&format!("map: a community marker not suggested: {e}"));
                    e
                }
            };
            self.notice = Some((line, Instant::now()));
            // the next look at the map asks the list again
            self.asked = None;
        }
    }

    /// A failure said once until it changes.
    fn say(&mut self, line: &str) {
        if self.said != line {
            crate::say(line);
            self.said = line.to_string();
        }
    }

    /// The suggestion sent (`play::mapsuggest`: its name and note typed), the
    /// map saying so meanwhile.
    pub fn send(&mut self, spot: Spot, name: &str, note: &str) {
        let Some(base) = self.base() else {
            self.notice("There is no marker list to send to in this run.");
            return;
        };
        let (url, body) = (format!("{base}mapmarks/suggest"), suggest_body(spot, name, note, &client()));
        crate::say(&format!("map: suggesting \"{}\" at ({:.0}, {:.0}) in world {}", clip(name, NAME_MAX), spot.at[0], spot.at[1], spot.world));
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.sending = Some(on_thread(move || http::post(&url, body)));
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.sending = Some(on_promise(http::post(url, body)));
        }
        self.notice("Sending the marker...");
    }

    /// A line the map shows for `SHOWN`.
    pub fn notice(&mut self, line: &str) {
        self.notice = Some((line.to_string(), Instant::now()));
    }

    /// The line the map shows now, while it is fresh (a send still on its way
    /// keeps its line up).
    pub fn notice_now(&self) -> Option<&str> {
        self.notice.as_ref().filter(|(_, at)| self.sending.is_some() || at.elapsed() < SHOWN).map(|(l, _)| l.as_str())
    }

    /// Whether a suggestion is on its way.
    pub fn sending(&self) -> bool {
        self.sending.is_some()
    }

    /// A spot asked to be suggested (`Map::suggest`).
    pub fn ask(&mut self, spot: Spot) {
        self.spot = Some(spot);
    }

    /// The spot asked, once (`Map::take_suggest`).
    pub fn take_spot(&mut self) -> Option<Spot> {
        self.spot.take()
    }

    /// The map's state line's part (`Map::describe`).
    pub fn describe(&self) -> String {
        match self.marks.len() {
            0 => String::new(),
            n => format!(", {n} community markers"),
        }
    }
}

/// The cache's file: the list's body as it came, and its ETag.
fn wrap_cache(body: &str, etag: Option<&str>) -> Vec<u8> {
    serde_json::json!({ "etag": etag, "body": body }).to_string().into_bytes()
}

fn unwrap_cache(bytes: &[u8]) -> Option<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    Some((v.get("body")?.as_str()?.to_string(), v.get("etag").and_then(|e| e.as_str()).map(str::to_string)))
}

/// A pin's head (its middle, a window point) over its tip at `p`, `r` its size.
pub fn head(p: [f32; 2], r: f32) -> [f32; 2] {
    [p[0], p[1] - r * 1.65]
}

fn quad(ui: &mut Ui, white: usize, pos: [[f32; 2]; 4], color: [f32; 4]) {
    ui.push(Quad { tex: white, blend: 3, pos, uv: [[0.5, 0.5]; 4], color, clip: None });
}

fn rect(ui: &mut Ui, white: usize, r: [f32; 4], color: [f32; 4]) {
    quad(ui, white, [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]], color);
}

/// The pins at their frame points (their tips) and kinds, `r` frame pixels
/// their head's half width and `o` their outline: a rounded head of the
/// kind's colour on a point down to the spot, outlined, a pale dot in the
/// head (none of the map's other marks is a pin).
pub fn draw_pins(ui: &mut Ui, white: usize, shown: &[(Kind, [f32; 2])], r: f32, o: f32) {
    let shape = |ui: &mut Ui, p: [f32; 2], grow: f32, color: [f32; 4]| {
        let c = head(p, r);
        let (a, b) = (r + grow, r * 0.62 + grow);
        // the point: a triangle from under the head to the spot
        quad(ui, white, [[c[0] - r * 0.6 - grow, c[1] + r * 0.4], [c[0] + r * 0.6 + grow, c[1] + r * 0.4], [p[0], p[1] + grow * 1.5], [p[0], p[1] + grow * 1.5]], color);
        // the head: two bars across, its corners cut
        rect(ui, white, [c[0] - a, c[1] - b, c[0] + a, c[1] + b], color);
        rect(ui, white, [c[0] - b, c[1] - a, c[0] + b, c[1] + a], color);
    };
    for &(_, p) in shown {
        shape(ui, p, o, OUTLINE);
    }
    for &(kind, p) in shown {
        shape(ui, p, 0.0, kind.color());
        let c = head(p, r);
        let d = r * 0.3;
        rect(ui, white, [c[0] - d, c[1] - d, c[0] + d, c[1] + d], DOT);
    }
}

const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
const DOT: [f32; 4] = [1.0, 0.98, 0.92, 1.0];
/// A name's plate, as the towns' are, and its words.
const PLATE: [f32; 4] = [0.02, 0.03, 0.05, 0.6];
const NAME: [f32; 4] = [0.9, 1.0, 0.96, 1.0];

/// The markers' names where `markers::place_names` put them (frame rects),
/// in font `font` scaled `t`, `pad` frame pixels inside their plates.
pub fn draw_names(ui: &mut Ui, white: usize, font: usize, placed: &[(usize, [f32; 4])], names: &[String], t: f32, pad: [f32; 2]) {
    for &(_, r) in placed {
        rect(ui, white, r, PLATE);
    }
    for &(i, r) in placed {
        if let Some(n) = names.get(i) {
            ui.text_scaled(font, r[0] + pad[0], r[1] + pad[1], t, NAME, &vi::ui::wide(n));
        }
    }
}

#[cfg(test)]
pub mod testing {
    //! A stand-in for the list's server: on a port of its own, each
    //! request on a connection of its own, recorded, and answered by a
    //! closure (status, headers, body).

    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, Default)]
    pub struct Request {
        pub method: String,
        pub path: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Request {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
        }
    }

    pub type Answer = (u16, Vec<(String, String)>, String);

    /// The server's base URL and what it was asked.
    pub fn serve(answer: impl Fn(&Request) -> Answer + Send + 'static) -> (String, Arc<Mutex<Vec<Request>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let mut req = Request { method: parts.next().unwrap_or("").to_string(), path: parts.next().unwrap_or("").to_string(), ..Default::default() };
                let mut length = 0usize;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                    if let Some((k, v)) = h.trim_end().split_once(':') {
                        if k.eq_ignore_ascii_case("content-length") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                        req.headers.push((k.trim().to_string(), v.trim().to_string()));
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                req.body = String::from_utf8_lossy(&body).into_owned();
                let (status, headers, text) = answer(&req);
                log.lock().unwrap().push(req);
                let reason = match status {
                    200 => "OK",
                    304 => "Not Modified",
                    400 => "Bad Request",
                    _ => "Other",
                };
                let mut out = format!("HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n", text.len());
                for (k, v) in headers {
                    out.push_str(&format!("{k}: {v}\r\n"));
                }
                out.push_str("\r\n");
                out.push_str(&text);
                let mut s = stream;
                let _ = s.write_all(out.as_bytes());
            }
        });
        (base, asked)
    }

    /// `frame` run until `done` holds, at most `secs` seconds.
    pub fn until(secs: f32, mut frame: impl FnMut() -> bool) -> bool {
        let t = std::time::Instant::now();
        while t.elapsed().as_secs_f32() < secs {
            if frame() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::testing::{serve, until};
    use super::*;

    const LIST: &str = r#"{"version":"3f2a","marks":[
        {"id":7,"world":0,"x":24611.5,"z":15502.25,"name":"Hidden cave","note":"Behind the falls","kind":"landmark"},
        {"id":8,"world":1,"x":100,"z":200,"name":"  Orc camp  ","kind":"CAMP"},
        {"id":9,"world":0,"x":1,"z":2,"name":"Odd one","kind":"shrine"},
        {"id":10,"world":0,"z":2,"name":"No x"},
        {"id":11,"world":0,"x":1,"z":2,"name":""}
    ]}"#;

    /// The list's body reads: each marker with its world, place, words and
    /// kind (in any case; an unknown one `Other`), one with no x or no name
    /// passed over; the controls: a body with no list, or no JSON, reads as
    /// none.
    #[test]
    fn the_list_reads() {
        let (version, marks) = parse(LIST).expect("the list");
        assert_eq!(version, "3f2a");
        assert_eq!(marks.len(), 3, "{marks:?}");
        assert_eq!(marks[0], Mark { id: 7, world: 0, at: [24611.5, 15502.25], name: "Hidden cave".into(), note: "Behind the falls".into(), kind: Kind::Landmark });
        assert_eq!((marks[1].world, marks[1].at, marks[1].name.as_str(), marks[1].note.as_str(), marks[1].kind), (1, [100.0, 200.0], "Orc camp", "", Kind::Camp));
        assert_eq!(marks[2].kind, Kind::Other, "an unknown kind");
        assert_eq!(parse(r#"{"version":"1"}"#), None, "no list");
        assert_eq!(parse("<html>busy</html>"), None, "not JSON");
        assert_eq!(parse(r#"{"version":2,"marks":[]}"#), Some(("2".into(), Vec::new())), "a version as a number");
        let card = marks[0].card();
        let words: Vec<&str> = card.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(words, ["Hidden cave", "Behind the falls", "A landmark, a community marker"]);
        assert_eq!(marks[1].card().len(), 2, "no note, no note's line");
    }

    /// A suggestion goes as the list takes it: its world, place, name and
    /// note cut to their lengths (characters, not bytes) and the build;
    /// the answer is its id, or the server's own sentence, or what went
    /// wrong when it gave none.
    #[test]
    fn a_suggestion_and_its_answer() {
        let long: String = "é".repeat(60);
        let body = suggest_body(Spot { world: 2, at: [10.5, -3.25] }, &format!("  {long}  "), &"n".repeat(300), "dev-abc1234");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!((v["world"].as_i64(), v["x"].as_f64(), v["z"].as_f64(), v["client"].as_str()), (Some(2), Some(10.5), Some(-3.25), Some("dev-abc1234")));
        assert_eq!(v["name"].as_str().unwrap().chars().count(), NAME_MAX);
        assert_eq!(v["note"].as_str().unwrap().len(), NOTE_MAX);
        assert_eq!(clip("  a b  ", 40), "a b");
        assert_eq!(answer(200, r#"{"ok":true,"id":345}"#), Ok(345));
        assert_eq!(answer(400, r#"{"ok":false,"error":"That spot has a marker already."}"#), Err("That spot has a marker already.".into()));
        assert_eq!(answer(200, r#"{"ok":false,"error":"Too many from you today."}"#), Err("Too many from you today.".into()), "a refusal with 200");
        assert_eq!(answer(502, "<html>Bad Gateway</html>"), Err("The marker list answered 502: nothing was kept.".into()));
    }

    /// The list is asked when the map is up, kept in the cache, and an
    /// ask again sends its ETag: the server's 304 keeps it; a start with no
    /// server shows the cache's. A closed map asks nothing. The control: a
    /// cache never written leaves a start with no server with no markers.
    #[test]
    fn the_list_is_asked_kept_and_asked_again() {
        let (base, asked) = serve(|r| match r.header("if-none-match") {
            Some("\"3f2a\"") => (304, Vec::new(), String::new()),
            _ => (200, vec![("ETag".into(), "\"3f2a\"".into())], LIST.to_string()),
        });
        let dir = std::env::temp_dir().join(format!("eqoa-community-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut c = Community { cache: Some(Box::new(store::Dir::new(dir.clone()))), ..Default::default() };
        c.set_base(Some(base.clone()));
        c.frame(false);
        std::thread::sleep(Duration::from_millis(100));
        assert!(asked.lock().unwrap().is_empty(), "a closed map asks nothing");
        assert!(until(5.0, || {
            c.frame(true);
            !c.marks.is_empty()
        }));
        assert_eq!(c.marks.len(), 3);
        assert_eq!(asked.lock().unwrap()[0].header("if-none-match"), None, "the first ask has nothing to send");
        assert!(std::fs::read(dir.join(CACHE_FILE)).is_ok(), "kept");
        // asked again: its ETag, a 304, the list kept
        c.asked = None;
        assert!(until(5.0, || {
            c.frame(true);
            asked.lock().unwrap().len() == 2 && c.fetching.is_none()
        }));
        assert_eq!(asked.lock().unwrap()[1].header("if-none-match"), Some("\"3f2a\""));
        assert_eq!(c.marks.len(), 3, "a 304 keeps the list");
        // a start with no server: the cache's
        let mut offline = Community { cache: Some(Box::new(store::Dir::new(dir.clone()))), ..Default::default() };
        offline.set_base(None);
        offline.frame(true);
        assert_eq!(offline.marks.len(), 3, "the cache's list");
        let _ = std::fs::remove_dir_all(&dir);
        let mut control = Community { cache: Some(Box::new(store::Dir::new(dir.clone()))), ..Default::default() };
        control.set_base(None);
        control.frame(true);
        assert!(control.marks.is_empty(), "the control: nothing kept, nothing shown");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A list that does not read, or a server that answers an error, leaves
    /// the list held; the control: a good one replaces it.
    #[test]
    fn a_bad_answer_keeps_the_list() {
        let (base, _) = serve(|r| match r.path.as_str() {
            "/bad/mapmarks" => (200, Vec::new(), "not json".into()),
            "/down/mapmarks" => (500, Vec::new(), String::new()),
            _ => (200, Vec::new(), r#"{"version":"9","marks":[{"id":1,"world":0,"x":5,"z":6,"name":"Well"}]}"#.into()),
        });
        let mut c = Community { cache: Some(Box::new(store::Dir::new(std::env::temp_dir().join(format!("eqoa-community-bad-{}", std::process::id()))))), ..Default::default() };
        c.take("1".into(), parse(LIST).unwrap().1, None);
        for sub in ["bad/", "down/"] {
            c.set_base(Some(format!("{base}{sub}")));
            c.asked = None;
            c.frame(true);
            assert!(until(5.0, || {
                c.frame(true);
                c.fetching.is_none()
            }));
            assert_eq!(c.marks.len(), 3, "{sub}: kept");
        }
        c.set_base(Some(base));
        c.asked = None;
        assert!(until(5.0, || {
            c.frame(true);
            c.marks.len() == 1
        }));
        assert_eq!(c.marks[0].name, "Well", "the control: a good list taken");
    }
}
