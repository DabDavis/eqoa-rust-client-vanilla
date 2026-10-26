//! `/voice`, the client's own chat command (never sent as chat): `/voice
//! rooms` (the lobby), `/voice join <room> [password]` (joins, or makes the
//! room, listed), `/voice unlisted <room> [password]` (the same, a room it
//! makes unlisted: only those who know its name find it), `/voice leave`,
//! `/voice who`. A room's name with spaces goes in quotes; the password is the
//! rest of the line, its ends trimmed. The password goes no further than the
//! room's H (`ids::room_hash`): it is never shown, logged or kept, and a
//! report scrubs it (`report::secret`).

/// A room's name: 3 to 24 printable characters once trimmed (the server
/// refuses others, and an obscene one).
pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 24;

/// A `/voice` line's meaning.
#[derive(Clone, PartialEq, Eq)]
pub enum Command {
    Rooms,
    Join { name: String, password: Option<String>, unlisted: bool },
    Leave,
    Who,
    /// a room's name that cannot be one
    BadName(String),
    /// anything else: the command's own words
    Usage,
}

/// Its password never printed, even in a test's failure.
impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Rooms => write!(f, "Rooms"),
            Command::Join { name, password, unlisted } => {
                write!(f, "Join {{ name: {name:?}, password: {}, unlisted: {unlisted} }}", if password.is_some() { "<hidden>" } else { "none" })
            }
            Command::Leave => write!(f, "Leave"),
            Command::Who => write!(f, "Who"),
            Command::BadName(n) => write!(f, "BadName({n:?})"),
            Command::Usage => write!(f, "Usage"),
        }
    }
}

/// The command's words, for a line that asks for them.
pub const USAGE: &str = "Voice: /voice rooms, /voice join <room> [password], /voice unlisted <room> [password], /voice leave, /voice who (a room's name with spaces in quotes).";

/// Whether `name` (trimmed) can be a room's.
pub fn good_name(name: &str) -> bool {
    let n = name.trim();
    (NAME_MIN..=NAME_MAX).contains(&n.chars().count()) && !n.chars().any(char::is_control)
}

/// What follows `/voice`.
pub fn parse(rest: &str) -> Command {
    let rest = rest.trim_start();
    let (word, after) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let word = word.to_ascii_lowercase();
    match word.as_str() {
        "join" | "unlisted" => {
            let after = after.trim_start();
            let (name, password) = if let Some(q) = after.strip_prefix('"') {
                match q.split_once('"') {
                    Some((n, p)) => (n, p),
                    None => return Command::Usage,
                }
            } else {
                after.split_once(char::is_whitespace).unwrap_or((after, ""))
            };
            let (name, password) = (name.trim(), password.trim());
            if name.is_empty() {
                Command::Usage
            } else if !good_name(name) {
                Command::BadName(name.to_string())
            } else {
                Command::Join { name: name.to_string(), password: (!password.is_empty()).then(|| password.to_string()), unlisted: word == "unlisted" }
            }
        }
        "rooms" | "leave" | "who" if !after.trim().is_empty() => Command::Usage,
        "rooms" => Command::Rooms,
        "leave" => Command::Leave,
        "who" => Command::Who,
        _ => Command::Usage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn join(name: &str, password: Option<&str>, unlisted: bool) -> Command {
        Command::Join { name: name.into(), password: password.map(str::to_string), unlisted }
    }

    /// The commands, a quoted name with spaces, the password the rest of the
    /// line and optional, unlisted beside join; the controls: no name, a name
    /// too short or too long, a closing quote missing, another word, words
    /// after rooms, leave or who.
    #[test]
    fn the_commands_parse() {
        assert_eq!(parse("join Raid hunter2"), join("Raid", Some("hunter2"), false));
        assert_eq!(parse("join Raid"), join("Raid", None, false), "an open room");
        assert_eq!(parse("  JOIN   Raid   hunter2  "), join("Raid", Some("hunter2"), false));
        assert_eq!(parse("join \"Raid Night\" hunter two"), join("Raid Night", Some("hunter two"), false), "quoted; the password the rest");
        assert_eq!(parse("join \" Raid Night \"hunter2"), join("Raid Night", Some("hunter2"), false));
        assert_eq!(parse("unlisted Hideout"), join("Hideout", None, true), "unlisted");
        assert_eq!(parse("Unlisted \"Our Hideout\" s3cret"), join("Our Hideout", Some("s3cret"), true));
        assert_eq!(parse("rooms"), Command::Rooms);
        assert_eq!(parse("leave"), Command::Leave);
        assert_eq!(parse("Who"), Command::Who);
        assert_eq!(parse("join"), Command::Usage, "no name");
        assert_eq!(parse("unlisted"), Command::Usage);
        assert_eq!(parse("join ab"), Command::BadName("ab".into()), "too short");
        assert_eq!(parse(&format!("join {}", "x".repeat(25))), Command::BadName("x".repeat(25)), "too long");
        assert_eq!(parse(&format!("join {}", "x".repeat(24))), join(&"x".repeat(24), None, false), "24 is a name");
        assert_eq!(parse("join \"Raid Night hunter2"), Command::Usage, "no closing quote");
        assert_eq!(parse(""), Command::Usage);
        assert_eq!(parse("jion Raid hunter2"), Command::Usage);
        assert_eq!(parse("rooms please"), Command::Usage);
        assert_eq!(parse("leave now"), Command::Usage);
        assert_eq!(parse("who is there"), Command::Usage);
    }

    /// A join's password is not in its Debug (a log line or a failed test's
    /// message); the control: its name is.
    #[test]
    fn a_password_is_never_printed() {
        let s = format!("{:?}", parse("join Raid Tulip-7781"));
        assert!(!s.contains("Tulip-7781"), "{s}");
        assert!(s.contains("Raid") && s.contains("<hidden>"), "{s}");
    }
}
