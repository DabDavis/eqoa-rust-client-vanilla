//! What a client derives itself, the same on every client and never the
//! password it comes from: a locked room's H (PROTOCOL.md's voice lobby: the
//! server keeps H while the room lives and never sees the password), and the
//! demo's group channel (eqoaclassic's voice server used from a server with no
//! voice of its own, `demo`). A name is folded as every client folds it:
//! trimmed, lower-cased (Unicode's simple lower case, `str::to_lowercase`).

use sha2::{Digest, Sha256};

/// SHA-256(`text`) in lowercase hex.
pub fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// A name as every client folds it.
pub fn fold(name: &str) -> String {
    name.trim().to_lowercase()
}

/// A locked room's H: SHA-256("eqoa-voice-room\n" + its name folded + "\n" +
/// its password as typed); an open room's (no password) is empty.
pub fn room_hash(name: &str, password: Option<&str>) -> String {
    match password {
        Some(p) if !p.is_empty() => sha256_hex(&format!("eqoa-voice-room\n{}\n{p}", fold(name))),
        _ => String::new(),
    }
}

/// The demo's group channel: "d:" + the first 32 hex digits of
/// SHA-256("eqoa-voice-demo group\n" + the server's name + "\n" + the group's
/// names (the player's own among them) folded, sorted, one a line).
pub fn demo_group<'a>(server: &str, names: impl IntoIterator<Item = &'a str>) -> String {
    let mut n: Vec<String> = names.into_iter().map(fold).collect();
    n.sort();
    let h = sha256_hex(&format!("eqoa-voice-demo group\n{server}\n{}", n.join("\n")));
    format!("d:{}", &h[..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHA-256 itself, against a known digest (the control the rest stand on).
    #[test]
    fn sha256_hex_is_sha256() {
        assert_eq!(sha256_hex("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_ne!(sha256_hex("abd"), sha256_hex("abc"));
    }

    /// A locked room's H is the same whatever the name's case and
    /// surrounding spaces; another password (its case too: as typed) or
    /// another name is another H (the controls); no password, none.
    #[test]
    fn a_rooms_hash_is_its_name_folded_and_its_password() {
        let h = room_hash("Raid Night", Some("hunter2"));
        assert!(h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()), "{h}");
        assert_eq!(room_hash("  raid NIGHT ", Some("hunter2")), h);
        assert_eq!(h, sha256_hex("eqoa-voice-room\nraid night\nhunter2"), "the formula");
        assert_ne!(room_hash("Raid Night", Some("hunter3")), h, "another password");
        assert_ne!(room_hash("Raid Night", Some("Hunter2")), h, "a password is as typed");
        assert_ne!(room_hash("Raid Nights", Some("hunter2")), h, "another name");
        assert_eq!(room_hash("Raid Night", None), "", "an open room");
        assert_eq!(room_hash("Raid Night", Some("")), "");
    }

    /// The demo's group is its members whatever their order and case, on one
    /// server: another server or another member is another channel (the
    /// controls).
    #[test]
    fn the_demos_group_is_the_members_and_the_server() {
        let g = demo_group("Sandstorm", ["Jibrish", "dabsha", "Kellina"]);
        assert!(g.starts_with("d:") && g.len() == 34, "{g}");
        assert_eq!(demo_group("Sandstorm", ["Kellina", "Dabsha", "JIBRISH"]), g, "the same members, another order and case");
        assert_eq!(g, format!("d:{}", &sha256_hex("eqoa-voice-demo group\nSandstorm\ndabsha\njibrish\nkellina")[..32]), "the formula");
        assert_ne!(demo_group("Sandstorm Test", ["Jibrish", "Dabsha", "Kellina"]), g, "another server");
        assert_ne!(demo_group("Sandstorm", ["Jibrish", "Dabsha"]), g, "another group");
    }
}
