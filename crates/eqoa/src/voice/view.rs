//! What the voice window (`voicewnd`; the user: "add voice chat gui in
//! addition to the chat commands, so it is easy") reads and asks of voice,
//! and nothing more. It reads
//! a `VoiceView` (`Play::voice_view`) and asks through `Play::voice_set_mode`,
//! `voice_refresh_lobby`, `voice_join`, `voice_leave`, `voice_set_volume` and
//! `voice_set_muted`, the same functions the `/voice` commands call.

/// The voice the player chose: off, the server's voice by push-to-talk or
/// open mic, or the demo (eqoaclassic's voice server, where the server
/// offers none: Sandstorm).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceMode {
    #[default]
    Off,
    PushToTalk,
    OpenMic,
    Demo,
}

/// One the player may hear: a groupmate, one in its room, or both.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoiceMember {
    pub peer: u32,
    pub name: String,
    pub group: bool,
    pub room: bool,
    /// its voice's level now, 0..1
    pub level: f32,
    /// its own volume, 0..2 (1 its own level)
    pub volume: f32,
    pub muted: bool,
}

/// A room of the lobby.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VoiceRoomInfo {
    pub name: String,
    pub members: u16,
    pub locked: bool,
}

/// Voice as the window shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoiceView {
    /// the server offers voice, or the demo can be used
    pub available: bool,
    /// only the demo (a server that offers none)
    pub demo_only: bool,
    pub mode: VoiceMode,
    pub connected: bool,
    /// 0..1
    pub mic_level: f32,
    /// bit 0 the group, bit 1 the room
    pub transmitting: u8,
    /// the room the player is in: its name, locked, unlisted
    pub room: Option<(String, bool, bool)>,
    /// the group's and the room's, each once
    pub members: Vec<VoiceMember>,
    /// the last list asked for
    pub lobby: Vec<VoiceRoomInfo>,
    /// the last voice line ("Voice: in room X", "Voice: wrong password for X")
    pub status: String,
}
