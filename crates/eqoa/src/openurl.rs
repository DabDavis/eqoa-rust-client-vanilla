//! A web page opened from the game: Station's Forgot Password on eqoaclassic
//! (the owner's of 2026-11-11), which has no Station API behind it but a
//! page, go-eqoa-server's `auth.password_url`. Natively the system's browser,
//! in a page a new tab (`window.eqoaOpenLink`, web/notices.js: a notice with
//! the link when the browser held the tab back), on Android a VIEW intent.

/// eqoaclassic's password page: a forgotten password asked for there (a GM
/// approves it), a known one changed at its `#change`.
pub const PASSWORD_URL: &str = "https://eqoaclassic.com/eqoa/password";

/// eqoaclassic's address, for a server named by it rather than its name.
const OUR_ADDRESS: &str = "104.238.144.159";

/// The password page for `server` (the host Station logs in at, as Network
/// Setup or the page names it: a name or an address, a port or a scheme
/// perhaps): eqoaclassic's on eqoaclassic, none on any other server
/// (Sandstorm's, a private one), whose Forgot Password stays the game's.
pub fn password_url(server: &str) -> Option<&'static str> {
    let s = server.trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let host = s.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next().unwrap_or("").trim_end_matches('.').to_ascii_lowercase();
    let ours = host == "eqoaclassic.com" || host.ends_with(".eqoaclassic.com") || host == OUR_ADDRESS;
    ours.then_some(PASSWORD_URL)
}

/// Opens `url` outside the game: false when it could not be handed on (the
/// game then shows the address to type).
pub fn open(url: &str) -> bool {
    crate::say(&format!("opening {url}"));
    #[cfg(test)]
    {
        testing::OPENED.with(|o| o.borrow_mut().push(url.to_string()));
        return true;
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::open_link(url);
        true
    }
    #[cfg(target_os = "android")]
    {
        crate::android::open_link(url);
        true
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[allow(unreachable_code)]
    {
        native(url)
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn native(url: &str) -> bool {
    use std::process::{Command, Stdio};
    let mut c = if cfg!(windows) {
        // the URL protocol's handler, as Explorer opens a link: no console
        // window, no quoting of `start`'s
        let mut c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(url);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match c.spawn() {
        // reaped by a thread of its own, so that no zombie is left
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
            true
        }
        Err(e) => {
            crate::say(&format!("{url} was not opened: {e}"));
            false
        }
    }
}

/// What the tests opened (a test opens no browser).
#[cfg(test)]
pub mod testing {
    thread_local! {
        pub static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// The URLs opened on this thread since the last call.
    pub fn take() -> Vec<String> {
        OPENED.with(|o| std::mem::take(&mut *o.borrow_mut()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// eqoaclassic's page for eqoaclassic, by name, address, port or
    /// scheme; none for Sandstorm, a private server, the local one, none.
    #[test]
    fn the_password_page_by_host() {
        for s in ["eqoaclassic.com", "EQOAClassic.com", " eqoaclassic.com ", "eqoaclassic.com:9735", "www.eqoaclassic.com", "eqoaclassic.com.", "104.238.144.159", "104.238.144.159:10070", "https://eqoaclassic.com/eqoa/play/", "wss://eqoaclassic.com/eqoa/relay"] {
            assert_eq!(password_url(s), Some(PASSWORD_URL), "{s}");
        }
        for s in ["", "sandstorm.example", "127.0.0.1", "localhost", "198.51.100.20:9735", "noteqoaclassic.com", "eqoaclassic.com.evil.example", "104.238.144.15", "1104.238.144.159"] {
            assert_eq!(password_url(s), None, "{s}");
        }
    }
}
