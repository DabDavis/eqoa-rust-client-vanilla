//! `VIWndGenericRenderer` (viwndgenericrenderer.cpp): the window that runs
//! STATION.PAK's pages. A stack of pages with one dialog over them, the
//! pages' actions turned into account-server requests (`OnAction`), the
//! server's answers (`VIStationListener`'s calls, here `on_event`), the
//! news, and Save Account.
//!
//! Left out, each where it would be: the memory card's checks (`McErr`; a
//! card read error, a "DONE" code 3), the network interface's polling
//! (`VINetStatus`: a computer's network is up), and customer service
//! (`VICSInterface`: its own DRDP session to a server named by the card's
//! `eqahosts.txt`, and the CS windows; the page's two buttons open its
//! "Connecting to Customer Service" dialog, which then waits).

use super::api::{Api, Event};
use super::dialog::{DOut, Dialog};
use super::page::{self, Out, Page};
use super::pak::Pak;
use crate::editbox::EditBox;
use crate::ui::Layout;
use net::auth::{Account, Billing, Plan, Reply};
use vi::strtab::StringTable;
use vi::ui::{Vui, Window, wide};

/// Who has the keys (`VIStation::m_xB40C`, the window manager's focus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Renderer,
    Editor,
}

/// The news, being fetched (`VUrl::GetPoll`): the body once whole, or why
/// not (`GetPoll`'s 3 and 4); None while under way.
pub trait NewsFetch {
    fn poll(&mut self) -> Option<Result<Vec<u8>, String>>;
}

impl NewsFetch for crate::pregame::link::HttpGet {
    fn poll(&mut self) -> Option<Result<Vec<u8>, String>> {
        crate::pregame::link::HttpGet::poll(self)
    }
}

/// Starts fetching (host, port, path): `link::HttpGet`, or a test's.
pub type NewsStart = Box<dyn FnMut(&str, u16, &str) -> Box<dyn NewsFetch>>;

/// The disc's region (`VIGameOptions::m_region`, which `Load` sets from
/// the disc): the USA's.
const REGION: i32 = 0;

/// The expansions SLUS hands STATION (`VIGetExpansionFlags`): Frontiers'.
const EXPANSION: i32 = 3;

/// The red a form's label turns when its field is empty.
const RED: [f32; 4] = [0.909_803_9, 0.549_019_6, 0.223_529_41, 1.0];

/// The Country list's choices (`sCountries`).
const COUNTRIES: [&str; 3] = ["US - USA", "CA - Canada", "MX - Mexico"];

pub struct Renderer {
    pub focus: Focus,
    /// `m_editor`: the editor the pages' texts open
    pub editor: EditBox,
    pub editor_visible: bool,
    /// the page stack, its top LAST
    pub pages: Vec<Page>,
    pub dialog: Dialog,
    pub api: Api,
    /// what the API said, for the listener's calls (`flush`)
    queue: Vec<Event>,
    sku: String,
    game_card: String,
    sub_status: i32,
    pub login_status: i32,
    payment_type: i32,
    pub uid: i32,
    /// the login reply's game features (for CLIENT, `Login::features`)
    pub features: i32,
    /// `m_xE64`: 1 from an account's creation until its login's answer
    xe64: i32,
    /// an action run once the subscription's status is known
    action: String,
    check_net: bool,
    pub login: String,
    pub password: String,
    saved_login: String,
    saved_password: String,
    pub save_account: bool,
    billing: Billing,
    account: Account,
    pub logged_in: bool,
    pak: Pak,
    /// `Process`'s answer: 1 play, 2 the network brought down
    pub result: i32,
    news_url: String,
    pub news_start: NewsStart,
    news: Option<Box<dyn NewsFetch>>,
    /// what the fetch gave, kept until the polls are counted out
    news_got: Option<Result<Vec<u8>, String>>,
    pub news_active: bool,
    news_wait: i32,
    news_poll: i32,
    plans: Vec<Plan>,
    save_delay: i32,
    save_window: Window,
    /// the module's statics: one run of STATION each
    first_disconnect: bool,
    account_saved: bool,
    ping_delay: i32,
    status_frames: i32,
    /// the server the pages log in at (`stationLiveHosts.txt`'s place)
    server: String,
    words: StringTable,
    language: i32,
    /// the dialog's `Init` (it needs a raster, so it is made at the first draw)
    pub dialog_inited: bool,
    /// where the page drew its objects, for a pointer
    pub hits: crate::pointer::Hits,
    /// the saved login to write, when the save's frames run out
    pub saving: Option<(String, String, bool)>,
}

impl Renderer {
    /// `VIWndGenericRenderer()` at `now`.
    pub fn new(now: i64, words: StringTable, language: i32) -> Renderer {
        Renderer {
            focus: Focus::Renderer,
            editor: EditBox::default(),
            editor_visible: false,
            pages: Vec::new(),
            dialog: Dialog::default(),
            api: Api::new(now),
            queue: Vec::new(),
            sku: String::new(),
            game_card: String::new(),
            sub_status: -1,
            login_status: -1,
            payment_type: -1,
            uid: -1,
            features: 0,
            xe64: 0,
            action: String::new(),
            check_net: false,
            login: String::new(),
            password: String::new(),
            saved_login: String::new(),
            saved_password: String::new(),
            save_account: false,
            billing: Billing::default(),
            account: Account::default(),
            logged_in: false,
            pak: Pak::default(),
            result: 0,
            news_url: String::new(),
            news_start: Box::new(|h: &str, p: u16, path: &str| Box::new(crate::pregame::link::HttpGet::start(h, p, path)) as Box<dyn NewsFetch>),
            news: None,
            news_got: None,
            news_active: false,
            news_wait: 0,
            news_poll: 0,
            plans: Vec::new(),
            save_delay: 0,
            save_window: Window::default(),
            first_disconnect: true,
            account_saved: false,
            ping_delay: 0,
            status_frames: 0,
            server: String::new(),
            words,
            language,
            dialog_inited: false,
            hits: crate::pointer::Hits::default(),
            saving: None,
        }
    }

    /// One WORDS.TXT entry in the UI's language.
    fn word(&self, id: i32) -> Vec<u16> {
        wide(self.words.get(id, self.language))
    }

    fn word8(&self, id: i32) -> String {
        self.words.get(id, self.language).chars().map(|c| if (c as u32) < 256 { c } else { '?' }).collect()
    }

    /// `Load`: the saved login, STATION.PAK, the first page, the dialog, the
    /// editor and the servers. `pak` is the disc's; `saved` the options'
    /// login (`VIGameOptions::Username`, `Password`, `m_x90`); `server` the
    /// host the pages log in at, at `port`.
    pub fn load(&mut self, pak: &[u8], saved: (String, String, bool), server: &str, port: i32) {
        self.saved_login = saved.0.chars().take(63).collect();
        self.login = self.saved_login.clone();
        self.saved_password = saved.1.chars().take(63).collect();
        self.password = self.saved_password.clone();
        self.save_account = saved.2;
        self.pak = Pak::load(pak);
        self.load_page("data/ui/login.xml");
        // the editor: hidden, at (32, 190), empty
        self.editor_visible = false;
        self.editor.set_pos([32, 190]);
        self.editor.clear();
        // `LoadServers`: the server Network Setup keeps, at the auth port,
        // in stationLiveHosts.txt's place; the news from options.cfg
        self.server = server.to_string();
        self.api.add_server(server, port);
        let cfg = self.pak.find("/stationdata/config/options.cfg").map(|f| String::from_utf8_lossy(f).into_owned());
        self.read_config(cfg.as_deref().unwrap_or("NewsURL:http://everquestonlineadventures.station.sony.com/privatebeta/news_blank.jsp"));
    }

    /// `ReadConfig`: "name:value" lines; NewsURL is the only name.
    pub fn read_config(&mut self, text: &str) {
        for line in text.split(['\r', '\n']).filter(|l| !l.is_empty()) {
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else { break };
            if k == "NewsURL" {
                self.news_url = v.to_string();
            }
        }
    }

    /// The page on top.
    pub fn page(&self) -> Option<&Page> {
        self.pages.last()
    }

    fn page_mut(&mut self) -> Option<&mut Page> {
        self.pages.last_mut()
    }

    fn page_name(&self) -> String {
        self.page().map(|p| p.name.clone()).unwrap_or_default()
    }

    fn text_of(&self, name: &str) -> Option<String> {
        self.page()?.text(name).map(str::to_string)
    }

    /// `LoadPage(name)`: the pak's copy (the disc's loose files have none).
    pub fn load_page(&mut self, name: &str) {
        let Some(bytes) = self.pak.find(name) else { return };
        let words = &self.words;
        let lang = self.language;
        let word = |id: i32| wide(words.get(id, lang));
        if let Some(p) = page::load(bytes, name, &word) {
            self.push_page(p);
        }
    }

    /// `PushPage(page)`: filled from what the earlier pages held, made the
    /// top of the stack.
    pub fn push_page(&mut self, mut page: Page) {
        let name = page.name.clone();
        let has = |s: &str| name.contains(s);
        let region = REGION;
        if has("UpAcct") || has("UPACCT") {
            page.clear_options("Country");
            for c in COUNTRIES {
                page.add_option("Country", c);
            }
            if self.account.country.is_empty() {
                page.set_text("Country", match region {
                    1 => "GB - United Kingdom",
                    2 => "FR - France",
                    3 => "DE - Germany",
                    _ => "US - USA",
                });
            }
        } else if has("IDCreat2") || has("IDCREAT2") {
            let f: Vec<Option<String>> = ["Country", "PostalCode", "BirthDay", "BirthMonth", "BirthYear", "Gender", "EmailAddress"].iter().map(|n| page.text(n).map(str::to_string)).collect();
            page.clear_options("Country");
            for c in COUNTRIES {
                page.add_option("Country", c);
            }
            if self.account.country.is_empty() {
                let c = match region {
                    0 => Some("US - USA"),
                    1 => Some("GB - United Kingdom"),
                    2 => Some("FR - France"),
                    3 => Some("DE - Germany"),
                    _ => None,
                };
                if let Some(c) = c {
                    self.account.country = c.chars().take(32).collect();
                }
            }
            if f[..6].iter().all(Option::is_some) {
                let a = &mut self.account;
                let keep = |dst: &mut String, v: &Option<String>, n: usize| {
                    if let Some(v) = v
                        && !v.is_empty()
                        && dst.is_empty()
                    {
                        *dst = v.chars().take(n).collect();
                    }
                };
                keep(&mut a.country, &f[0], 32);
                keep(&mut a.postal, &f[1], 32);
                keep(&mut a.day, &f[2], 16);
                keep(&mut a.month, &f[3], 16);
                keep(&mut a.year, &f[4], 16);
                keep(&mut a.gender, &f[5], 16);
                keep(&mut a.email, &f[6], 128);
            }
        } else if has("IDCreate") || has("IDCREATE") {
            let f: Vec<Option<String>> = ["StationName", "StationPassword", "StationPassword2", "FirstName", "MiddleName", "LastName"].iter().map(|n| page.text(n).map(str::to_string)).collect();
            if f.iter().all(Option::is_some) {
                let a = &mut self.account;
                let keep = |dst: &mut String, v: &Option<String>, n: usize| {
                    if let Some(v) = v
                        && !v.is_empty()
                        && dst.is_empty()
                    {
                        *dst = v.chars().take(n).collect();
                    }
                };
                keep(&mut a.name, &f[0], 32);
                keep(&mut a.password, &f[1], 32);
                keep(&mut a.first, &f[3], 64);
                keep(&mut a.middle, &f[4], 32);
                keep(&mut a.last, &f[5], 64);
            }
        } else if has("BillPlan") || has("BILLPLAN") {
            page.clear_options("BillingPlan");
            for (k, plan) in self.plans.iter().enumerate() {
                let text = format!("{} (${})", plan.short_desc, plan.price);
                page.add_option("BillingPlan", &text);
                if k == 0 {
                    page.set_text("BillingPlan", &text);
                }
                if !self.sku.is_empty() && self.sku == plan.sku {
                    page.set_text("CurrentPlanName", &text);
                }
            }
        }
        self.pages.push(page);
        let (login, password, save) = (self.login.clone(), self.password.clone(), self.save_account);
        let a = self.account.clone();
        let creating = has("IDCreate") || has("IDCREATE");
        if has("login") {
            let p = self.page_mut().unwrap();
            if save {
                p.set_text("StationName", &login);
                p.set_text("StationPassword", &password);
                p.set_toggle("SaveAccountToggle", true);
            }
            let filled = p.text("StationName").is_some_and(|s| !s.is_empty()) && p.text("StationPassword").is_some_and(|s| !s.is_empty());
            if filled {
                p.select("LogInButton");
            }
        } else if creating {
            let (t, s) = (self.word(1241), self.word(1376));
            self.dialog.open(&t, None);
            self.dialog.set_text(&s);
            let p = self.page_mut().unwrap();
            for (n, v) in [("StationName", &a.name), ("StationPassword", &a.password), ("StationPassword2", &a.password), ("FirstName", &a.first), ("MiddleName", &a.middle), ("LastName", &a.last)] {
                p.set_text(n, v);
            }
        } else if has("IDCreat2") || has("IDCREAT2") {
            let p = self.page_mut().unwrap();
            for (n, v) in [("Country", &a.country), ("PostalCode", &a.postal), ("BirthDay", &a.day), ("BirthMonth", &a.month), ("BirthYear", &a.year), ("Gender", &a.gender), ("EmailAddress", &a.email)] {
                p.set_text(n, v);
            }
        }
    }

    /// `PageBack`: what the account pages hold kept, the top page dropped.
    pub fn page_back(&mut self) {
        if self.pages.len() < 2 {
            return;
        }
        let name = self.page_name();
        let t = |r: &Self, n: &str| r.text_of(n);
        if name.contains("IDCreat2") || name.contains("IDCREAT2") {
            let f: Vec<Option<String>> = ["Country", "PostalCode", "BirthDay", "BirthMonth", "BirthYear", "Gender", "EmailAddress"].iter().map(|n| t(self, n)).collect();
            if f[..6].iter().all(Option::is_some) {
                let a = &mut self.account;
                a.country = f[0].clone().unwrap().chars().take(32).collect();
                a.postal = f[1].clone().unwrap().chars().take(32).collect();
                a.day = f[2].clone().unwrap().chars().take(16).collect();
                a.month = f[3].clone().unwrap().chars().take(16).collect();
                a.year = f[4].clone().unwrap().chars().take(16).collect();
                a.gender = f[5].clone().unwrap().chars().take(16).collect();
                if let Some(e) = &f[6] {
                    a.email = e.chars().take(128).collect();
                }
            }
        } else if name.contains("IDCreate") || name.contains("IDCREATE") {
            let f: Vec<Option<String>> = ["StationName", "StationPassword", "StationPassword2", "FirstName", "MiddleName", "LastName"].iter().map(|n| t(self, n)).collect();
            if f.iter().all(Option::is_some) {
                let a = &mut self.account;
                a.name = f[0].clone().unwrap().chars().take(32).collect();
                a.password = f[1].clone().unwrap().chars().take(32).collect();
                a.first = f[3].clone().unwrap().chars().take(64).collect();
                a.middle = f[4].clone().unwrap().chars().take(32).collect();
                a.last = f[5].clone().unwrap().chars().take(64).collect();
            }
        }
        self.pages.pop();
    }

    fn open(&mut self, title: i32, action: Option<&str>) {
        let t = self.word(title);
        self.dialog.open(&t, action);
    }

    fn say(&mut self, id: i32) {
        let t = self.word(id);
        self.dialog.set_text(&t);
    }

    /// `CheckName`: a field needs a letter or a digit; its label turns red
    /// when it has neither.
    fn check_name(&mut self, s: &str, label: &str) -> bool {
        if s.chars().any(|c| c.is_ascii_alphanumeric()) {
            return true;
        }
        if let Some(p) = self.page_mut() {
            p.set_text_color(label, RED);
        }
        false
    }

    /// `OnAction(action)`: a page's action by name, the form checks, then
    /// the request.
    pub fn on_action(&mut self, action: &str, now: i64) {
        if self.pages.is_empty() && action == "DisconnectConfirmed" {
            self.result = 2;
        }
        if self.pages.is_empty() {
            return;
        }
        let words_fields = |r: &Self, names: &[&str]| -> Vec<Option<String>> { names.iter().map(|n| r.text_of(n)).collect() };
        match action {
            "SubmitStationLogin" => {
                self.open(1300, None);
                self.say(758);
                match (self.text_of("StationName"), self.text_of("StationPassword")) {
                    (Some(n), Some(p)) => {
                        self.dialog.confirm = true;
                        self.api.validate_login(&n, &p, now, &mut self.queue);
                    }
                    _ => self.say(1264),
                }
            }
            "ContinueAcctCreate" => {
                let f = words_fields(self, &["StationName", "StationPassword", "StationPassword2", "FirstName", "MiddleName", "LastName"]);
                if !f.iter().all(Option::is_some) {
                    self.say(1264);
                    return;
                }
                let [name, pw, pw2, first, middle, last] = std::array::from_fn(|k| f[k].clone().unwrap());
                let mut ok = self.check_name(&name, "StationNameLabel");
                ok &= self.check_name(&pw, "Password1Label");
                ok &= self.check_name(&pw2, "Password2Label");
                ok &= self.check_name(&first, "FirstNameLabel");
                ok &= self.check_name(&last, "LastNameLabel");
                let valid = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid(&name) {
                    self.open(1294, None);
                    self.say(1312);
                } else if !valid(&pw) {
                    self.open(1292, None);
                    self.say(1311);
                } else if !ok {
                    self.open(1288, None);
                    self.say(1310);
                } else if pw != pw2 {
                    self.open(1288, None);
                    self.say(1314);
                } else {
                    let a = &mut self.account;
                    a.name = name.chars().take(32).collect();
                    a.password = pw.chars().take(32).collect();
                    a.first = first.chars().take(64).collect();
                    a.middle = middle.chars().take(32).collect();
                    a.last = last.chars().take(64).collect();
                    self.load_page("data/ui/IDCreat2.xml");
                }
            }
            "SubmitAcctCreate" => {
                let f = words_fields(self, &["Country", "PostalCode", "BirthDay", "BirthMonth", "BirthYear", "Gender", "EmailAddress"]);
                if !f[..6].iter().all(Option::is_some) {
                    self.say(1264);
                    return;
                }
                let [country, postal, day, month, year, gender] = std::array::from_fn(|k| f[k].clone().unwrap());
                let email = f[6].clone();
                let mut ok = self.check_name(&country, "CountryLabel");
                ok &= self.check_name(&postal, "PostalCodeLabel");
                ok &= self.check_name(&day, "BirthDayLabel");
                ok &= self.check_name(&month, "BirthMonthLabel");
                ok &= self.check_name(&year, "BirthYearLabel");
                ok &= self.check_name(&gender, "GenderLabel");
                if self.email_refused(email.as_deref().unwrap_or("")) {
                    return;
                }
                if country.starts_with("US -") && !is_zip(&postal) {
                    self.open(1293, None);
                    self.say(1354);
                } else if !ok {
                    self.open(1288, None);
                    self.say(1310);
                } else {
                    let a = &mut self.account;
                    a.country = country.chars().take(32).collect();
                    a.postal = postal.chars().take(32).collect();
                    a.day = day.chars().take(16).collect();
                    a.month = month.chars().take(16).collect();
                    a.year = year.chars().take(16).collect();
                    a.gender = gender.chars().take(16).collect();
                    if let Some(e) = email {
                        a.email = e.chars().take(128).collect();
                    }
                    self.open(1241, None);
                    self.say(758);
                    self.dialog.confirm = true;
                    self.xe64 = 1;
                    let a = self.account.clone();
                    self.api.create_account(&a, now, &mut self.queue);
                }
            }
            "SubmitForgotPassword" => {
                let f = words_fields(self, &["Username", "BirthDay", "BirthMonth", "BirthYear", "EmailAddress"]);
                if !f.iter().all(Option::is_some) {
                    self.say(1264);
                    return;
                }
                let [user, day, month, year, email] = std::array::from_fn(|k| f[k].clone().unwrap());
                let mut ok = self.check_name(&user, "UsernameLabel");
                ok &= self.check_name(&day, "BirthDayLabel");
                ok &= self.check_name(&month, "BirthMonthLabel");
                ok &= self.check_name(&year, "BirthYearLabel");
                ok &= self.check_name(&email, "EmailAddressLabel");
                if self.email_refused(&email) {
                    return;
                }
                if !ok {
                    self.open(1288, None);
                    self.say(1310);
                } else {
                    self.open(1268, None);
                    self.say(758);
                    self.dialog.confirm = true;
                    self.api.request66(&user, &month, &day, &year, &email, now, &mut self.queue);
                }
            }
            "SubmitAcctUpdate" => {
                let f = words_fields(self, &["Country", "PostalCode", "EmailAddress"]);
                let (Some(country), Some(postal)) = (f[0].clone(), f[1].clone()) else {
                    self.say(1264);
                    return;
                };
                let mut ok = self.check_name(&country, "CountryLabel");
                ok &= self.check_name(&postal, "PostalCodeLabel");
                let email = f[2].clone().unwrap_or_default();
                if self.email_refused(&email) {
                    return;
                }
                if country.starts_with("US -") && !is_zip(&postal) {
                    self.open(1293, None);
                    self.say(1354);
                } else if !ok {
                    self.open(1288, None);
                    self.say(1310);
                } else {
                    self.open(1369, None);
                    self.say(758);
                    self.dialog.confirm = true;
                    let uid = self.uid;
                    self.api.request64(uid, &country, &postal, &email, now, &mut self.queue);
                }
            }
            "SubmitChangePassword" => {
                self.open(1229, None);
                self.say(758);
                let f = words_fields(self, &["CurrentPassword", "NewPassword", "NewPassword2"]);
                match (&f[0], &f[1], &f[2]) {
                    (Some(cur), Some(pw), Some(pw2)) => {
                        if cur.is_empty() || pw.is_empty() || pw2.is_empty() {
                            self.say(1305);
                        } else if *cur != self.password {
                            self.say(1245);
                        } else if pw != pw2 {
                            self.open(1225, None);
                            self.say(1308);
                        } else {
                            self.dialog.confirm = true;
                            let uid = self.uid;
                            self.api.change_password(uid, cur, pw, now, &mut self.queue);
                        }
                    }
                    _ => self.say(1264),
                }
            }
            "SubmitCheckAccountKey" => {
                self.open(1231, None);
                self.say(758);
                match self.text_of("AccountKey") {
                    Some(k) => {
                        let up = k.to_ascii_uppercase();
                        if let Some(p) = self.page_mut() {
                            p.set_text("AccountKey", &up);
                        }
                        self.dialog.confirm = true;
                        let uid = self.uid;
                        self.api.request_id(48, uid, &up, now, &mut self.queue);
                    }
                    None => self.say(1264),
                }
            }
            "SubmitCheckGameCard" => {
                self.open(1230, None);
                self.say(758);
                match self.text_of("GameCardNo") {
                    Some(c) => {
                        let up = c.to_ascii_uppercase();
                        self.game_card = up.chars().take(63).collect();
                        if let Some(p) = self.page_mut() {
                            p.set_text("GameCardNo", &up);
                        }
                        let uid = self.uid;
                        self.api.request_id(54, uid, &up, now, &mut self.queue);
                    }
                    None => self.say(1264),
                }
            }
            "SubmitConsumeGameCard" => {
                self.open(1230, None);
                self.say(758);
                self.billing.kind = "gamecard".into();
                self.billing.number = self.game_card.chars().take(64).collect();
                let (uid, b, sku) = (self.uid, self.billing.clone(), self.sku.clone());
                self.api.request46(uid, &b, &sku, now, &mut self.queue);
            }
            "SubmitRental" => match self.text_of("RentalNumber") {
                Some(n) => {
                    self.open(1232, None);
                    self.say(758);
                    self.billing.kind = "rental".into();
                    self.billing.number = n.chars().take(64).collect();
                    let (uid, b, sku) = (self.uid, self.billing.clone(), self.sku.clone());
                    self.api.request46(uid, &b, &sku, now, &mut self.queue);
                }
                None => {
                    self.open(767, None);
                    self.say(1291);
                }
            },
            "SubmitPurchaseSubscription" => {
                self.open(1329, None);
                self.say(758);
                match self.text_of("BillingPlan") {
                    Some(sel) => {
                        for plan in &self.plans {
                            if format!("{} (${})", plan.short_desc, plan.price) == sel {
                                self.sku = plan.sku.clone();
                            }
                        }
                        let (uid, b, sku) = (self.uid, self.billing.clone(), self.sku.clone());
                        self.api.request46(uid, &b, &sku, now, &mut self.queue);
                    }
                    None => self.say(1264),
                }
            }
            "ContinueUpdateCCInfo" | "ContinueCCInfo" => {
                let f = words_fields(self, &["FirstName", "LastName", "Address", "City", "State", "PostalCode", "Country"]);
                if !f.iter().all(Option::is_some) {
                    self.open(1213, None);
                    self.say(1264);
                    return;
                }
                let [first, last, address, city, state, postal, country] = std::array::from_fn(|k| f[k].clone().unwrap());
                let mut ok = self.check_name(&first, "FirstNameLabel");
                ok &= self.check_name(&last, "LastNameLabel");
                ok &= self.check_name(&address, "AddressLabel");
                ok &= self.check_name(&city, "CityLabel");
                ok &= self.check_name(&state, "StateLabel");
                ok &= self.check_name(&postal, "PostalCodeLabel");
                ok &= self.check_name(&country, "CountryLabel");
                if country.starts_with("US -") && !is_zip(&postal) {
                    self.open(1293, None);
                    self.say(1354);
                } else if !ok {
                    self.open(1284, None);
                    self.say(1310);
                } else {
                    let b = &mut self.billing;
                    b.address = address.chars().take(127).collect();
                    b.city = city.chars().take(63).collect();
                    b.country = country.chars().take(31).collect();
                    b.state = state.chars().take(63).collect();
                    b.first = first.chars().take(31).collect();
                    b.last = last.chars().take(31).collect();
                    b.postal = postal.chars().take(31).collect();
                    self.load_page(if action == "ContinueCCInfo" { "data/ui/CCSetup2.xml" } else { "data/ui/ChangeCC2.xml" });
                }
            }
            "SubmitUpdateCCInfo" | "SubmitCCInfo" => {
                let f = words_fields(self, &["CardType", "CardNo", "ExpireMonth", "ExpireYear"]);
                if !f.iter().all(Option::is_some) {
                    self.open(1213, None);
                    self.say(1264);
                    return;
                }
                let [kind, number, month, year] = std::array::from_fn(|k| f[k].clone().unwrap());
                if action == "SubmitUpdateCCInfo" {
                    self.open(1329, None);
                    self.say(758);
                }
                let mut ok = self.check_name(&kind, "CardTypeLabel");
                ok &= self.check_name(&number, "CardNoLabel");
                ok &= self.check_name(&month, "ExpireMonthLabel");
                ok &= self.check_name(&year, "ExpireYearLabel");
                if !number.chars().all(|c| c.is_ascii_digit()) {
                    self.open(1285, None);
                    self.say(1326);
                } else if !ok {
                    self.open(1284, None);
                    self.say(1310);
                } else {
                    let b = &mut self.billing;
                    b.year = year.chars().take(15).collect();
                    b.month = month.chars().take(15).collect();
                    b.number = number.chars().take(63).collect();
                    b.kind = kind.chars().take(15).collect();
                    let uid = self.uid;
                    if action == "SubmitCCInfo" {
                        self.dialog.confirm = true;
                        self.open(758, None);
                        self.say(1252);
                        self.api.request_id(62, uid, "", now, &mut self.queue);
                    } else {
                        let (b, sku) = (self.billing.clone(), self.sku.clone());
                        self.api.request46(uid, &b, &sku, now, &mut self.queue);
                    }
                }
            }
            "CancelAccount" => {
                self.open(1220, None);
                self.say(758);
                let uid = self.uid;
                self.api.request_id(52, uid, "", now, &mut self.queue);
            }
            "UpdateBilling" => {
                if self.payment_type == -1 || self.sub_status == -1 {
                    if self.login_status == 9 {
                        self.open(1331, None);
                        self.say(1375);
                    } else {
                        self.open(1274, None);
                        self.say(1337);
                        self.action = action.to_string();
                        self.dialog.confirm = true;
                        let uid = self.uid;
                        self.api.request_id(50, uid, "", now, &mut self.queue);
                    }
                } else if self.sub_status == 5 {
                    self.open(1214, None);
                    self.say(1388);
                } else if self.login_status == 9 {
                    self.open(1214, None);
                    self.say(1377);
                } else if self.payment_type == 2 {
                    self.load_page("data/ui/UpGlobalCollect.xml");
                } else if self.payment_type == 0 {
                    self.load_page("data/ui/UpCC.xml");
                } else {
                    self.load_page("data/ui/UpGC.xml");
                }
            }
            "CancelAccountLink" => {
                if self.sub_status == -1 {
                    self.open(1273, None);
                    self.say(1336);
                    self.action = action.to_string();
                    self.dialog.confirm = true;
                    let uid = self.uid;
                    self.api.request_id(50, uid, "", now, &mut self.queue);
                } else if self.sub_status == 5 {
                    self.open(1220, None);
                    self.say(1387);
                } else if self.sub_status == 0 || self.sub_status == 1 {
                    self.open(1220, None);
                    self.say(1382);
                } else {
                    self.load_page("data/ui/Cancel.xml");
                }
            }
            "ForgotPassword" => {
                self.open(1313, None);
                self.say(1248);
            }
            "CustomerService" => self.load_page("data/ui/CSPage.xml"),
            "NewCSTicket" | "CSTicketBrowser" => {
                // `VICSInterface::OpenRequest`/`OpenInbox`: its dialog; the
                // CS session itself is not ported (this module's header)
                self.dialog.open8("Connecting to Customer Service...", None);
                self.dialog.set_text8("Connecting to the Customer Service Server.  Please wait.");
            }
            "RentEQOA" => {
                self.open(1332, None);
                self.say(1334);
            }
            "PopDialog" => self.dialog.close(),
            "PlayEQOA" => {
                if self.login_status == 7 {
                    self.open(1346, None);
                    self.say(1386);
                } else if self.logged_in {
                    self.dialog.confirm = true;
                    self.open(1275, Some("REALLYPlayEQOA"));
                    self.say(1810);
                    self.news_active = true;
                    self.news_wait = 4;
                    self.news_poll = 4;
                    self.dialog.prompt = false;
                }
            }
            "REALLYPlayEQOA" => {
                if self.logged_in {
                    self.result = 1;
                }
            }
            "Disconnect" => {
                self.dialog.confirm = true;
                self.open(1234, Some("DisconnectConfirmed"));
                self.say(1212);
            }
            "DisconnectConfirmed" => self.result = 2,
            _ => {}
        }
    }

    /// `CheckEmail`'s refusals, each its dialog (an empty address is good).
    fn email_refused(&mut self, s: &str) -> bool {
        let id = match check_email(s) {
            1 => 1390,
            2 => 1392,
            3 => 1391,
            4 => 1393,
            _ => return false,
        };
        self.open(1286, None);
        self.say(id);
        true
    }

    /// `SaveAccount`: the login into the options when the toggle (or a
    /// change) asks; the save itself a few frames on.
    pub fn save_account(&mut self) {
        if self.pages.is_empty() {
            return;
        }
        let save;
        if !self.account_saved {
            let name = self.page_name();
            if name.contains("login") || name.contains("LOGIN") {
                self.save_account = self.page().is_some_and(|p| p.toggle("SaveAccountToggle"));
            }
            if !self.save_account {
                save = true;
                self.saved_login.clear();
                self.saved_password.clear();
            } else if self.saved_login != self.login || self.saved_password != self.password {
                save = true;
                self.saved_login = self.login.clone();
                self.saved_password = self.password.clone();
            } else {
                save = false;
            }
        } else {
            save = self.save_account;
        }
        if !save {
            return;
        }
        let (u, p) = if self.save_account { (self.login.clone(), self.password.clone()) } else { (String::new(), String::new()) };
        self.saving = Some((u, p, self.save_account));
        self.account_saved = true;
        self.save_delay = 3;
    }

    /// The listener's calls for what the API said, in order (on the console
    /// they come inside the API's own calls); a call that makes a request
    /// adds what that request says.
    pub fn flush(&mut self, now: i64) {
        while !self.queue.is_empty() {
            for e in std::mem::take(&mut self.queue) {
                self.on_event(e, now);
            }
        }
    }

    /// The API's events: `VIStationListener`'s calls.
    pub fn on_event(&mut self, e: Event, now: i64) {
        match e {
            Event::Error(_, text) => {
                // `OnError`: the editor hidden, the keys back here, the dialog
                self.editor_visible = false;
                self.focus = Focus::Renderer;
                self.dialog.open(&[], None);
                self.dialog.set_text8(&text);
            }
            Event::Status(text) => {
                // `OnStatus`: an interface is up (always, here)
                if !self.first_disconnect {
                    self.open(1235, None);
                    self.dialog.set_text8(&text);
                }
                self.check_net = true;
                self.first_disconnect = false;
            }
            Event::Connected => {
                self.dialog.close();
                self.check_net = false;
            }
            Event::Reply(r) => self.on_reply(r, now),
        }
    }

    fn on_reply(&mut self, r: Reply, now: i64) {
        match r {
            Reply::Login { result, uid, accstat, subtime, gamefeatures, .. } => self.on_validate_login(result, uid, accstat, subtime, gamefeatures),
            Reply::Created { success, text, .. } => {
                if !self.dialog.is_open() {
                    return;
                }
                self.dialog.confirm = false;
                if success {
                    self.login = self.account.name.clone();
                    self.password = self.account.password.clone();
                    self.dialog.confirm = true;
                    self.save_account = true;
                    self.say(1384);
                    let (l, p) = (self.login.clone(), self.password.clone());
                    self.api.validate_login(&l, &p, now, &mut self.queue);
                    return;
                }
                if text.contains("exists") {
                    let fmt = self.word8(1371);
                    let name = self.account.name.clone();
                    let s = format_s(&fmt, &[&name, &name]);
                    self.dialog.set_text8(&s);
                    self.page_back();
                    return;
                }
                self.dialog.set_text8(&text);
            }
            Reply::PasswordChanged { success, text } => {
                if !self.dialog.is_open() {
                    return;
                }
                self.dialog.confirm = false;
                if success {
                    self.say(1344);
                    if let Some(p) = self.text_of("NewPassword") {
                        self.password = p;
                        self.page_back();
                    }
                } else {
                    self.dialog.set_text8(&text);
                }
            }
            Reply::Pair { ty: 45, a, .. } => {
                if a == 0 {
                    self.say(1373);
                    self.load_page("data/ui/BillMeth.xml");
                    self.dialog.close();
                } else {
                    self.say(1289);
                }
            }
            Reply::Pair { ty: 47, a, b } => {
                if a == 0 {
                    self.logged_in = true;
                    self.say(1343);
                    self.load_page("data/ui/conn.xml");
                    self.dialog.close();
                    return;
                }
                self.sku.clear();
                let id = match b {
                    1 => 1309,
                    4 => 1399,
                    51 => 1358,
                    52 => 1283,
                    53 => 1270,
                    10 => 1357,
                    11 => 1349,
                    12 => 1389,
                    16 => 1348,
                    17 => 1347,
                    19 => 1352,
                    20 => 1351,
                    21 => 1350,
                    _ => 1355,
                };
                self.say(id);
            }
            Reply::Pair { ty: 49, a, b } => {
                // `OnConsumeAccountKey`
                if !self.dialog.is_open() {
                    return;
                }
                self.dialog.confirm = false;
                if a == 0 {
                    self.say(1374);
                    self.dialog.confirm = true;
                    let (l, p) = (self.login.clone(), self.password.clone());
                    self.api.validate_login(&l, &p, now, &mut self.queue);
                    self.save_account = true;
                    return;
                }
                match b {
                    43 => self.say(1295),
                    41 => self.say(1296),
                    40 => {
                        let fmt = self.word(1360);
                        let game = self.word(1803);
                        self.dialog.set_text(&format_w(&fmt, &game));
                    }
                    44 => self.say(1359),
                    _ if a == 1 => self.say(1809),
                    _ => self.say(1290),
                }
            }
            Reply::Pair { ty: 53, a, b } => {
                // `OnCancelAccount`
                if a == 0 {
                    self.open(1220, None);
                    self.say(1208);
                    self.load_page("data/ui/login.xml");
                    self.dialog.confirm = false;
                    return;
                }
                match b {
                    1 => self.say(1379),
                    2 | 3 => self.say(1356),
                    _ => self.say(1261),
                }
            }
            Reply::Pair { .. } => {}
            Reply::SubscriptionInfo { a, b, status, payment, sku } => {
                if !self.dialog.is_open() {
                    return;
                }
                self.dialog.confirm = false;
                if a != 0 {
                    match b {
                        1 => {
                            self.say(1378);
                            self.load_page("data/ui/EnterKey.xml");
                        }
                        2 | 3 => {
                            self.say(1316);
                            self.load_page("data/ui/BillMeth.xml");
                        }
                        _ => self.say(1263),
                    }
                    return;
                }
                self.sub_status = status;
                match status {
                    0 => {
                        self.say(1398);
                        self.load_page("data/ui/BillMeth.xml");
                    }
                    3 => {
                        self.say(1383);
                        self.load_page("data/ui/BillMeth.xml");
                    }
                    1 | 2 | 4 | 5 | 7 => {
                        self.dialog.close();
                        self.payment_type = payment;
                        self.sku = sku.chars().take(32).collect();
                        let a = std::mem::take(&mut self.action);
                        self.on_action(&a, now);
                    }
                    6 => {
                        self.say(1401);
                        self.load_page("data/ui/BillMeth.xml");
                    }
                    _ => self.say(1239),
                }
            }
            Reply::R55 { a, b, sku, .. } => {
                if a == 0 {
                    if !sku.is_empty() {
                        self.sku = sku.chars().take(31).collect();
                    }
                    self.load_page("data/ui/GCCons.xml");
                    self.dialog.close();
                    return;
                }
                self.say(match b {
                    51 => 1358,
                    52 => 1283,
                    53 => 1270,
                    _ => 1287,
                });
            }
            Reply::Plans { a, b, plans } => {
                self.dialog.confirm = false;
                self.dialog.close();
                self.plans = plans;
                if a == 0 && b == 0 {
                    self.load_page("data/ui/BillPlan.xml");
                    return;
                }
                self.open(767, None);
                self.say(1262);
            }
            Reply::One { ty: 65, a } => {
                if !self.dialog.is_open() {
                    return;
                }
                self.dialog.confirm = false;
                self.say(if a != 0 { 1345 } else { 1254 });
                self.load_page("data/ui/Conn.xml");
            }
            Reply::One { ty: 67, a } => {
                if !self.dialog.is_open() {
                    return;
                }
                self.dialog.confirm = false;
                if a != 0 {
                    self.say(1396);
                    self.page_back();
                } else {
                    self.say(1353);
                }
            }
            Reply::One { .. } | Reply::Ignored(_) | Reply::SessionKey(_) => {}
        }
    }

    /// `OnValidateLogin`: the login's answer.
    fn on_validate_login(&mut self, result: i32, uid: i32, accstat: i32, subtime: i32, gamefeatures: i32) {
        let status = self.xe64;
        let mut expansion = false;
        self.login_status = -1;
        self.xe64 = 0;
        self.uid = -1;
        self.payment_type = -1;
        self.sub_status = -1;
        if !self.dialog.is_open() {
            return;
        }
        self.dialog.confirm = false;
        if result != 0 {
            self.say(1394);
            return;
        }
        self.features = gamefeatures;
        if matches!(accstat, 3 | 1 | 5 | 9 | 7) {
            self.login_status = accstat;
            self.save_account();
            if accstat == 1 {
                if (subtime.wrapping_sub(1) as u32) <= 0xD2EFE {
                    let days = subtime / 86400;
                    let text = if days > 0 { format_i(&self.word8(1381), days) } else { self.word8(1783) };
                    self.dialog.set_text8(&text);
                }
            } else if status != 1 {
                if EXPANSION & 2 != 0 && gamefeatures & 2 == 0 {
                    expansion = true;
                    self.say(1660);
                } else if EXPANSION & 2 == 0 && gamefeatures & 2 != 0 {
                    self.say(1661);
                } else {
                    self.say(1301);
                }
            }
            if let (Some(n), Some(p)) = (self.text_of("StationName"), self.text_of("StationPassword")) {
                self.login = n;
                self.password = p;
            }
            self.save_account();
            self.uid = uid;
            self.logged_in = true;
            if expansion {
                self.load_page("data/ui/EnterKey.xml");
                return;
            }
            self.load_page("data/ui/conn.xml");
            return;
        }
        self.login_status = accstat;
        match (self.text_of("StationName"), self.text_of("StationPassword")) {
            (Some(n), Some(p)) => {
                self.login = n;
                self.password = p;
            }
            _ if self.login.is_empty() => {
                self.say(1363);
                return;
            }
            _ => {}
        }
        self.save_account();
        self.uid = uid;
        self.logged_in = true;
        match accstat {
            2 => {
                if status != 1 {
                    self.dialog.close();
                }
                self.load_page("data/ui/EnterKey.xml");
            }
            10 => {
                self.say(1397);
                self.load_page("data/ui/EnterKey.xml");
            }
            6 => {
                let (fmt, game) = (self.word(1400), self.word(1803));
                self.dialog.set_text(&format_w(&fmt, &game));
                self.load_page("data/ui/BillMeth.xml");
            }
            0 => {
                if EXPANSION & 2 != 0 && gamefeatures & 2 == 0 {
                    self.say(1660);
                    self.load_page("data/ui/EnterKey.xml");
                } else {
                    let (fmt, game) = (self.word(1380), self.word(1803));
                    self.dialog.set_text(&format_w(&fmt, &game));
                    self.load_page("data/ui/BillMeth.xml");
                }
            }
            8 => self.say(1385),
            _ => self.say(1364),
        }
    }

    /// `OnDrawSelf`: the API pumped (from the sixth frame), the page and the
    /// dialog over it, the "Saving" box and its save, the news.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, now: i64) {
        if !self.dialog_inited {
            self.dialog.init(vui, r);
            self.dialog_inited = true;
        }
        if self.check_net {
            // the console's Connect blocks here, its thread drawing
            self.api.poll_connect(&mut self.queue);
            self.flush(now);
            if let Some(p) = self.pages.last_mut() {
                let mut hits = crate::pointer::Hits::default();
                p.draw(vui, r, &mut hits);
                self.hits = hits;
            }
            if self.dialog.is_open() {
                self.dialog.draw(vui, r);
            }
            return;
        }
        if self.pages.is_empty() {
            return;
        }
        if self.ping_delay >= 6 {
            self.api.process(now, &mut self.queue);
            self.flush(now);
        }
        if self.ping_delay < 1000 {
            self.ping_delay += 1;
        }
        if let Some(p) = self.pages.last_mut() {
            let mut hits = crate::pointer::Hits::default();
            p.draw(vui, r, &mut hits);
            self.hits = hits;
        }
        if self.dialog.is_open() {
            self.dialog.draw(vui, r);
        }
        self.status_frames += 1;
        if self.save_delay != 0 && self.result != 2 {
            self.save_delay -= 1;
            let s = self.word(1765);
            self.save_window.init(vui, r, [320, 230], 460, 5, 0);
            let rows = self.save_window.count_lines(vui, r, &s, 380);
            self.save_window.init(vui, r, [320, 230], 460, rows, 0);
            self.save_window.open(true);
            self.save_window.draw(vui, r);
            let w = self.save_window.size[0] - 80;
            self.save_window.text_box(vui, r, &s, 40, 0, w, rows, [0.0, 0.0, 0.0, 1.0], 8);
            if self.save_delay == 0
                && let Some((u, p, on)) = self.saving.take()
                && !crate::settings::save_login(&u, &p, on)
            {
                crate::say("Station: the saved account was not kept");
            }
        }
        if !self.news_active {
            return;
        }
        if self.news_wait != 0 {
            self.dialog.prompt = false;
            self.news_wait -= 1;
            return;
        }
        if self.news.is_none() {
            // `new VUrl(m_newsURL)`: SOE's host is the server the player
            // set, the port the URL's (or EQOA_NEWS_PORT)
            let (port, path) = news_target(&self.news_url);
            let port = crate::pregame::port("EQOA_NEWS_PORT", port);
            self.news = Some((self.news_start)(&self.server, port, &path));
            self.news_got = None;
        }
        // `GetPoll`: whole (3), failed (4), or still coming
        if self.news_got.is_none() {
            self.news_got = self.news.as_mut().unwrap().poll();
        }
        let failed = matches!(self.news_got, Some(Err(_)));
        if self.news_poll != 0 && !failed {
            self.dialog.prompt = true;
            self.news_poll -= 1;
            return;
        }
        self.dialog.prompt = true;
        self.news_poll = 0;
        match self.news_got.take() {
            Some(Ok(b)) => {
                // the leading tabs and newlines skipped; a byte a character
                let start = b.iter().position(|&c| c != b'\t' && c != b'\n').unwrap_or(b.len());
                let text: String = b[start..].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
                self.open(if EXPANSION & 2 != 0 { 1804 } else { 1260 }, Some("REALLYPlayEQOA"));
                self.dialog.set_text8(&text);
                self.news_active = false;
                self.dialog.confirm = false;
                self.news = None;
            }
            Some(Err(e)) => {
                crate::say(&format!("Station: the news: {e}"));
                self.say(1238);
                self.news_active = false;
                self.dialog.confirm = false;
                self.news = None;
            }
            None => {}
        }
    }
}

/// The news URL's port (80 when it names none) and path, out of
/// "http://host[:port]/path" (`VUrl`'s parse); its host is not used.
pub fn news_target(url: &str) -> (u16, String) {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let port = hostport.split_once(':').and_then(|(_, p)| p.parse().ok()).unwrap_or(80);
    (port, path.to_string())
}

/// `IsZip`: five digits.
fn is_zip(s: &str) -> bool {
    s.len() == 5 && s.chars().all(|c| c.is_ascii_digit())
}

/// `CheckEmail`: 0 good (an empty one too), 1 a character it refuses, 2 no
/// '@', 3 no '.', 4 ends in '.'.
pub fn check_email(s: &str) -> i32 {
    if s.is_empty() {
        return 0;
    }
    let b = s.as_bytes();
    let (mut at, mut dot) = (false, false);
    for (i, &c) in b.iter().enumerate() {
        let name_char = c.is_ascii_alphanumeric() || c == b'_';
        if !name_char && c != b'.' && c != b'-' && c != b'+' && c != b'@' {
            return 1;
        }
        if c == b'@' {
            at = true;
        }
        if c == b'.' {
            dot = true;
            if i + 1 == b.len() {
                return 4;
            }
        }
    }
    if at {
        if dot { 0 } else { 3 }
    } else {
        2
    }
}

/// `sprintf(fmt, a, b, ...)` for a word's "%s"s.
fn format_s(fmt: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = fmt;
    let mut k = 0;
    while let Some(i) = rest.find("%s") {
        out.push_str(&rest[..i]);
        out.push_str(args.get(k).copied().unwrap_or(""));
        k += 1;
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// `sprintf(fmt, n)` for a word's "%i".
fn format_i(fmt: &str, n: i32) -> String {
    match fmt.find("%i").or_else(|| fmt.find("%d")) {
        Some(i) => format!("{}{}{}", &fmt[..i], n, &fmt[i + 2..]),
        None => fmt.to_string(),
    }
}

/// `snprintf16(buf, n, fmt, word)` for a word's "%S".
fn format_w(fmt: &[u16], w: &[u16]) -> Vec<u16> {
    let pat = [b'%' as u16, b'S' as u16];
    match fmt.windows(2).position(|x| x == pat) {
        Some(i) => [&fmt[..i], w, &fmt[i + 2..]].concat(),
        None => fmt.to_vec(),
    }
}

impl Renderer {
    /// `OnKeyDown` (the keys reach it when it has the focus): Escape is
    /// Triangle, the arrows and Enter the pad's; then the dialog, else the
    /// page (Triangle on the only page asks to disconnect).
    pub fn key_down(&mut self, mut key: i32, now: i64, cues: &mut Vec<i32>) {
        if self.result != 0 {
            return;
        }
        key = match key {
            27 => 132,
            19 => 138,
            20 => 139,
            18 => 137,
            17 => 136,
            13 => 134,
            k => k,
        };
        if self.dialog.is_open() {
            for o in self.dialog.key_down(key) {
                match o {
                    DOut::Sound(s) => cues.push(s),
                    DOut::Action(a) => self.on_action(&a, now),
                }
            }
            self.flush(now);
            return;
        }
        if self.pages.len() == 1 && key == 132 && !self.dialog.is_open() {
            self.on_action("Disconnect", now);
            self.flush(now);
            return;
        }
        let mut out = Vec::new();
        if let Some(p) = self.pages.last_mut() {
            p.key_down(key, &mut out);
        }
        self.outs(out, now, cues);
        self.flush(now);
    }

    /// What a page's objects asked for, in order.
    fn outs(&mut self, out: Vec<Out>, now: i64, cues: &mut Vec<i32>) {
        for o in out {
            match o {
                Out::Sound(s) => cues.push(s),
                Out::Back => self.page_back(),
                Out::LoadPage(p) => self.load_page(&p),
                Out::Action(a) => self.on_action(&a, now),
                Out::Edit { text, password, max_len, title } => {
                    // `VIGUIText::OnKeyDown`: the editor on the text, the keys its
                    self.editor.password = password;
                    self.editor.set_max_length(max_len);
                    self.editor.title = wide(&title);
                    self.editor.set_text(&wide(&text));
                    self.focus = Focus::Editor;
                    self.editor.focus(now);
                    self.editor_visible = true;
                }
            }
        }
    }

    /// The editor's answer to its parent, the renderer (`OnMessage`: 259
    /// its text, 260 cancelled), which hands it to the dialog when one is
    /// open (which does nothing with it, as shipped: the editor stays) and
    /// else to the page's selection, a text (`VIGUIText::OnMessage`): the
    /// editor hidden, its text the text's on 259, the keys back here.
    pub fn edit_done(&mut self, done: bool) {
        if self.dialog.is_open() {
            return;
        }
        let text: String = self.editor.text.iter().map(|&c| if c < 256 { c as u8 as char } else { '?' }).collect();
        if let Some(p) = self.pages.last_mut() {
            p.message(if done { 259 } else { 260 }, Some(&text));
        }
        self.editor_visible = false;
        self.focus = Focus::Renderer;
    }
}


#[cfg(test)]
mod tests {
    use super::super::api::testing::{Fake, framed, hello};
    use super::*;
    use crate::pointer::testing::NullRaster;
    use crate::pregame::link::Got;

    /// The disc's STATION.PAK and WORDS.TXT (the tests SKIP without it).
    struct Disc {
        pak: Vec<u8>,
        words: StringTable,
    }

    fn disc() -> Option<Disc> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut i = iso::Iso::open(&iso).unwrap();
        Some(Disc { pak: i.read("STATION.PAK").unwrap(), words: StringTable::parse(&i.read("DATA/WORDS.TXT").unwrap()).unwrap() })
    }

    /// A renderer on the disc's pages, its API on a scripted server, drawn
    /// on a raster that keeps nothing: STATION's frames, 17 ms apart.
    struct T {
        r: Renderer,
        f: Fake,
        vui: Vui,
        now: i64,
        cues: Vec<i32>,
        news: std::rc::Rc<std::cell::RefCell<Vec<(String, u16, String)>>>,
        news_reply: std::rc::Rc<std::cell::RefCell<Option<Result<Vec<u8>, String>>>>,
    }

    struct FakeNews(std::rc::Rc<std::cell::RefCell<Option<Result<Vec<u8>, String>>>>);

    impl NewsFetch for FakeNews {
        fn poll(&mut self) -> Option<Result<Vec<u8>, String>> {
            self.0.borrow_mut().take()
        }
    }

    impl T {
        fn new(d: &Disc, saved: (&str, &str, bool)) -> T {
            let f = Fake::default();
            let mut r = Renderer::new(0, d.words.clone(), 0);
            r.api = Api::with_connector(f.connector(), 0);
            let news = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let news_reply = std::rc::Rc::new(std::cell::RefCell::new(None));
            let (n, nr) = (news.clone(), news_reply.clone());
            r.news_start = Box::new(move |h: &str, p: u16, path: &str| {
                n.borrow_mut().push((h.to_string(), p, path.to_string()));
                Box::new(FakeNews(nr.clone())) as Box<dyn NewsFetch>
            });
            r.load(&d.pak, (saved.0.to_string(), saved.1.to_string(), saved.2), "192.0.2.1", 9735);
            let vui = Vui::new(d.words.clone(), Default::default(), Vec::new());
            let mut t = T { r, f, vui, now: 0, cues: Vec::new(), news, news_reply };
            // past the sixth frame the API connects; the server's hello
            t.frames(8);
            t.f.say(Got::Data(hello([7; 32])));
            t.frames(2);
            assert!(!t.r.api.connecting(), "the hello is the connect");
            t
        }

        fn frames(&mut self, n: i32) {
            for _ in 0..n {
                self.now += 17;
                self.r.draw(&self.vui, &mut NullRaster, self.now);
            }
        }

        fn key(&mut self, k: i32) {
            self.r.key_down(k, self.now, &mut self.cues);
            self.frames(1);
        }

        fn page(&self) -> &str {
            &self.r.page().unwrap().name
        }

        fn text(&self, name: &str) -> Option<String> {
            self.r.page().unwrap().text(name).map(str::to_string)
        }

        /// The field `name` selected, X: the editor on it; `s` typed and
        /// done (the editor's 259).
        fn type_in(&mut self, name: &str, s: &str) {
            self.r.pages.last_mut().unwrap().select(name);
            self.key(134);
            assert_eq!(self.r.focus, Focus::Editor, "{name} opens the editor");
            self.r.editor.set_text(&wide(s));
            self.r.edit_done(true);
            assert_eq!(self.r.focus, Focus::Renderer);
            assert_eq!(self.text(name).as_deref(), Some(s));
        }

        fn down(&mut self, n: i32) {
            for _ in 0..n {
                self.key(139);
            }
        }

        fn word(&self, id: i32) -> String {
            self.vui.words.get(id, 0).to_string()
        }

        fn dialog(&self) -> String {
            self.r.dialog.text()
        }

        /// What the client sent since the last call.
        fn sent(&mut self, from: &mut usize) -> Vec<u8> {
            let all = self.f.sent();
            let b = all[*from..].to_vec();
            *from = all.len();
            b
        }

        fn says(&mut self, b: Vec<u8>) {
            self.f.say(Got::Data(b));
            self.frames(2);
        }
    }

    /// The login reply as STATION reads it (`VIStationReply37`, 168 bytes on
    /// the wire): `result` at 12, uid 80, accstat 84, gamefeatures 164.
    fn login_reply(result: u32, uid: u32, accstat: u32, features: u32) -> Vec<u8> {
        framed(37, 168, |b| {
            b[12..16].copy_from_slice(&result.to_be_bytes());
            b[80..84].copy_from_slice(&uid.to_be_bytes());
            b[84..88].copy_from_slice(&accstat.to_be_bytes());
            b[164..168].copy_from_slice(&features.to_be_bytes());
        })
    }

    /// Log In on the login page goes out as `ValidateLogin` makes it, the
    /// dialog says so; the answer's success is conn.xml and the account. The
    /// control: a refused login stays on the page and says so.
    #[test]
    fn a_login_goes_out_as_the_game_sends_it_and_its_answer_moves_on() {
        let Some(d) = disc() else { return };
        crate::settings::testing::reset(false);
        let mut t = T::new(&d, ("", "", false));
        assert_eq!(t.page(), "data/ui/login.xml");
        let mut at = 0;
        t.type_in("StationName", "player0001");
        t.type_in("StationPassword", "password123");
        assert_eq!(t.sent(&mut at), Vec::<u8>::new(), "nothing sent before Log In");
        t.down(2);
        t.key(134);
        assert_eq!(t.r.dialog.window.title.chars(), wide(&t.word(1300)), "Logging in");
        assert_eq!(t.dialog(), t.word(758));
        assert_eq!(t.sent(&mut at), net::auth::login_request("player0001", "password123").to_vec());
        // the control: refused
        t.says(login_reply(1, 0, 0, 0));
        assert_eq!(t.dialog(), t.word(1394));
        assert_eq!(t.page(), "data/ui/login.xml");
        assert!(!t.r.logged_in);
        // accepted
        t.key(134);
        t.r.pages.last_mut().unwrap().select("LogInButton");
        t.key(134);
        assert_eq!(t.sent(&mut at), net::auth::login_request("player0001", "password123").to_vec());
        t.says(login_reply(0, 4242, 3, 3));
        assert_eq!(t.dialog(), t.word(1301), "Login Succeeded");
        assert_eq!(t.page(), "data/ui/conn.xml");
        assert!(t.r.logged_in);
        assert_eq!((t.r.uid, t.r.login_status, t.r.features), (4242, 3, 3));
        assert_eq!((t.r.login.as_str(), t.r.password.as_str()), ("player0001", "password123"));
        crate::settings::testing::reset(false);
    }

    /// Save Account: ticked, a login keeps the account in game.opt three of
    /// STATION's frames on (the "Saving" box meanwhile); the next run's login
    /// page is filled and its Log In selected. The control: unticked, the
    /// login keeps nothing and the next page is empty.
    #[test]
    fn save_account_keeps_the_login_for_the_next_run() {
        let Some(d) = disc() else { return };
        crate::settings::testing::reset(false);
        let mut t = T::new(&d, ("", "", false));
        t.type_in("StationName", "player0001");
        t.type_in("StationPassword", "password123");
        t.r.pages.last_mut().unwrap().select("SaveAccountToggle");
        t.key(134);
        assert!(t.r.page().unwrap().toggle("SaveAccountToggle"));
        t.r.pages.last_mut().unwrap().select("LogInButton");
        t.key(134);
        t.f.say(Got::Data(login_reply(0, 7, 3, 3)));
        t.frames(1);
        assert_eq!(crate::settings::saved_login(), (String::new(), String::new(), false), "not before the save's frames");
        t.frames(4);
        assert_eq!(crate::settings::saved_login(), ("player0001".to_string(), "password123".to_string(), true));
        let next = T::new(&d, ("player0001", "password123", true));
        assert_eq!(next.text("StationName").as_deref(), Some("player0001"));
        assert_eq!(next.text("StationPassword").as_deref(), Some("password123"));
        let p = next.r.page().unwrap();
        assert!(p.toggle("SaveAccountToggle"));
        assert_eq!(p.grid.selected(), p.find("LogInButton"), "Log In selected");
        // the control: unticked
        let mut t = T::new(&d, ("player0001", "password123", true));
        t.r.pages.last_mut().unwrap().select("SaveAccountToggle");
        t.key(134);
        t.r.pages.last_mut().unwrap().select("LogInButton");
        t.key(134);
        t.f.say(Got::Data(login_reply(0, 7, 3, 3)));
        t.frames(5);
        assert_eq!(crate::settings::saved_login(), (String::new(), String::new(), false));
        let next = T::new(&d, ("", "", false));
        assert_eq!(next.text("StationName").as_deref(), Some(""));
        crate::settings::testing::reset(false);
    }

    /// Create User Name: IDCreate.xml with its dialog, the form's checks,
    /// IDCreat2.xml with the disc's region's country, and the request as
    /// `CreateAccount` makes it; the success logs the new account in. The
    /// controls: passwords that differ and a US postal code that is not five
    /// digits send nothing.
    #[test]
    fn a_new_account_from_the_pages() {
        let Some(d) = disc() else { return };
        crate::settings::testing::reset(false);
        let mut t = T::new(&d, ("", "", false));
        let mut at = 0;
        t.down(4);
        t.key(134);
        assert_eq!(t.page(), "data/ui/IDCreate.xml");
        assert_eq!(t.dialog(), t.word(1376), "the page's dialog");
        t.key(134);
        t.frames(3);
        assert!(!t.r.dialog.is_open());
        t.type_in("StationName", "newbie");
        t.type_in("StationPassword", "newpass1");
        t.type_in("StationPassword2", "secret13");
        t.type_in("FirstName", "Van");
        t.type_in("LastName", "Lane");
        t.r.pages.last_mut().unwrap().select("LastName");
        t.down(1);
        t.key(134);
        assert_eq!(t.dialog(), t.word(1314), "the control: the passwords differ");
        assert_eq!(t.page(), "data/ui/IDCreate.xml");
        t.key(134);
        t.frames(3);
        t.type_in("StationPassword2", "newpass1");
        t.r.pages.last_mut().unwrap().select("LastName");
        t.down(1);
        t.key(134);
        assert_eq!(t.page(), "data/ui/IDCreat2.xml");
        assert_eq!(t.text("Country").as_deref(), Some("US - USA"), "the disc's region");
        t.type_in("PostalCode", "1234");
        t.r.pages.last_mut().unwrap().select("EmailAddress");
        t.down(1);
        t.key(134);
        assert_eq!(t.dialog(), t.word(1354), "the control: not a zip code");
        assert_eq!(t.sent(&mut at), Vec::<u8>::new());
        t.key(134);
        t.frames(3);
        t.type_in("PostalCode", "12345");
        t.r.pages.last_mut().unwrap().select("EmailAddress");
        t.down(1);
        t.key(134);
        let a = net::auth::Account {
            name: "newbie".into(),
            password: "newpass1".into(),
            first: "Van".into(),
            middle: String::new(),
            last: "Lane".into(),
            country: "US - USA".into(),
            postal: "12345".into(),
            day: "1".into(),
            month: "Jan".into(),
            year: "1977".into(),
            gender: "Male".into(),
            email: String::new(),
        };
        assert_eq!(t.sent(&mut at), net::auth::create_account(&a));
        assert_eq!(t.dialog(), t.word(758));
        // created: the new account logs in
        t.says(framed(41, 276, |b| b[12..16].copy_from_slice(&1u32.to_be_bytes())));
        assert_eq!(t.dialog(), t.word(1384));
        assert_eq!(t.sent(&mut at), net::auth::login_request("newbie", "newpass1").to_vec());
        t.says(login_reply(0, 99, 3, 3));
        assert_eq!(t.page(), "data/ui/conn.xml");
        assert_eq!(t.dialog(), t.word(1384), "a new account's login keeps the creation's words");
        t.frames(5);
        assert_eq!(crate::settings::saved_login(), ("newbie".to_string(), "newpass1".to_string(), true), "a new account is saved");
        crate::settings::testing::reset(false);
    }

    /// Manage Account, Change Password: the request as `ChangePassword`
    /// makes it (the account's id, both passwords DES'd), and its success
    /// the new password and the page before. The control: a current password
    /// that is not the login's sends nothing.
    #[test]
    fn a_password_change_from_the_pages() {
        let Some(d) = disc() else { return };
        crate::settings::testing::reset(false);
        let mut t = T::new(&d, ("", "", false));
        let mut at = 0;
        t.type_in("StationName", "player0001");
        t.type_in("StationPassword", "password123");
        t.r.pages.last_mut().unwrap().select("LogInButton");
        t.key(134);
        t.says(login_reply(0, 4242, 3, 3));
        t.key(134);
        t.frames(3);
        t.sent(&mut at);
        t.down(1);
        t.key(134);
        assert_eq!(t.page(), "data/ui/AcctMgmt.xml");
        t.key(134);
        assert_eq!(t.page(), "data/ui/PassChng.xml");
        t.type_in("CurrentPassword", "wrong");
        t.type_in("NewPassword", "password456");
        t.type_in("NewPassword2", "password456");
        t.down(1);
        t.key(134);
        assert_eq!(t.dialog(), t.word(1245), "the control: not the current password");
        assert_eq!(t.sent(&mut at), Vec::<u8>::new());
        t.key(134);
        t.frames(3);
        t.type_in("CurrentPassword", "password123");
        t.r.pages.last_mut().unwrap().select("NewPassword2");
        t.down(1);
        t.key(134);
        assert_eq!(t.sent(&mut at), net::auth::change_password(4242, "password123", "password456"));
        t.says(framed(43, 272, |b| b[12..16].copy_from_slice(&1u32.to_be_bytes())));
        assert_eq!(t.dialog(), t.word(1344));
        assert_eq!(t.r.password, "password456");
        assert_eq!(t.page(), "data/ui/AcctMgmt.xml");
        crate::settings::testing::reset(false);
    }

    /// Play: "Getting News..." four frames with no prompt, the news fetched
    /// from the server the player set at the URL's port and path, shown raw
    /// under Frontiers' title, and X plays (the result 1); a failed fetch says
    /// so in the same dialog, whose X plays too. The control: X while the
    /// news is coming (a confirmation, Cancel) does not play.
    #[test]
    fn play_shows_the_news_then_plays() {
        let Some(d) = disc() else { return };
        crate::settings::testing::reset(false);
        for ok in [true, false] {
            let mut t = T::new(&d, ("", "", false));
            t.type_in("StationName", "player0001");
            t.type_in("StationPassword", "password123");
            t.r.pages.last_mut().unwrap().select("LogInButton");
            t.key(134);
            t.says(login_reply(0, 4242, 3, 3));
            t.key(134);
            t.frames(3);
            t.key(134);
            assert_eq!(t.r.dialog.window.title.chars(), wide(&t.word(1275)));
            assert!(!t.r.dialog.prompt, "no prompt while the news waits");
            assert!(t.news.borrow().is_empty());
            t.frames(4);
            assert_eq!(*t.news.borrow(), vec![("192.0.2.1".to_string(), 7000, "/patch/web/eqoa/motd/news.html".to_string())]);
            t.key(134);
            assert_eq!(t.r.result, 0, "the control: X while the news is coming");
            *t.news_reply.borrow_mut() = Some(if ok { Ok(b"\t\n<p>Welcome</p>".to_vec()) } else { Err("refused".into()) });
            t.frames(6);
            if ok {
                assert_eq!(t.r.dialog.window.title.chars(), wide(&t.word(1804)));
                assert_eq!(t.dialog(), "<p>Welcome</p>");
            } else {
                assert_eq!(t.dialog(), t.word(1238));
            }
            assert!(!t.r.dialog.confirm);
            assert_eq!(t.r.result, 0);
            t.key(134);
            assert_eq!(t.r.result, 1);
        }
        crate::settings::testing::reset(false);
    }

    /// Triangle on the login page asks to disconnect; X on the question is
    /// the result 2. The control: Triangle on a second page goes back.
    #[test]
    fn triangle_on_the_first_page_disconnects() {
        let Some(d) = disc() else { return };
        crate::settings::testing::reset(false);
        let mut t = T::new(&d, ("", "", false));
        t.down(5);
        t.key(134);
        assert_eq!(t.page(), "data/ui/FrgtPass.xml");
        t.key(132);
        assert_eq!(t.page(), "data/ui/login.xml", "back, not a disconnect");
        assert!(!t.r.dialog.is_open());
        t.key(132);
        assert_eq!(t.r.dialog.window.title.chars(), wide(&t.word(1234)));
        assert_eq!(t.r.result, 0);
        t.key(134);
        assert_eq!(t.r.result, 2);
        crate::settings::testing::reset(false);
    }
}
