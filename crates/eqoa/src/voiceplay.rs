//! Play's half of voice chat (`voice`): each frame the player's choices (the
//! Extended page's Voice Chat, Voice Demo and Voice Volume, the push-to-talk
//! keys), who the player is to the demo, the manager's frame and its lines in
//! the chat; the `/voice` commands; and the voice window's interface
//! (PROTOCOL.md's "The voice window's interface", lane voicegui18's
//! `voicewnd`), which the commands go through too.

use super::Play;
use crate::settings::VoiceChat;
use crate::voice::{VoiceMode, VoiceView, Wants};

/// Voice's lines in the chat: a pale blue, the client's own.
const LINE: [f32; 4] = [0.62, 0.84, 1.0, 1.0];

impl Play {
    /// The player's choices now.
    fn voice_wants(&self) -> Wants {
        let c = crate::settings::client();
        Wants { chat: c.voice, demo: c.voice_demo, keys: self.voice.keys, volume: c.voice_volume }
    }

    /// A frame's voice (`Play::frame`): the demo told who the player is (its
    /// name, the shard's, its group's names), then the manager's frame on the
    /// game's link or the demo's, its lines in the chat.
    pub(super) fn voice_frame(&mut self, now: i64) {
        let w = self.voice_wants();
        let group = self.group.members[..self.group.count()].iter().map(|m| m.name.clone()).collect();
        let who = crate::voice::demo::Who { name: self.dump.player.name.clone(), server: crate::settings::home_shard().unwrap_or_default(), group };
        let crate::voice::PlayVoice { voice, demo, .. } = &mut self.voice;
        demo.prepare(who, now);
        voice.frame(&mut self.session, demo, w, now);
        for l in self.voice.voice.take_lines() {
            self.add_chat(l, LINE);
        }
    }

    /// `/voice ...`: the lobby's commands, never sent as chat (the password
    /// goes no further than the room's H).
    pub(super) fn voice_command(&mut self, rest: &str) {
        let on = crate::settings::client().voice != VoiceChat::Off;
        self.voice.voice.command(rest, on);
        for l in self.voice.voice.take_lines() {
            self.add_chat(l, LINE);
        }
    }

    /// Voice as the voice window shows it.
    pub fn voice_view(&self) -> VoiceView {
        self.voice.voice.view(self.voice_wants(), self.session.now() as i64)
    }

    /// The voice the window chose, kept at once as the Extended rows are:
    /// Demo turns Voice Demo on (and push-to-talk when Voice Chat was off).
    pub fn voice_set_mode(&mut self, mode: VoiceMode) {
        let c = crate::settings::client();
        let (chat, demo) = match mode {
            VoiceMode::Off => (VoiceChat::Off, c.voice_demo),
            VoiceMode::PushToTalk => (VoiceChat::PushToTalk, c.voice_demo),
            VoiceMode::OpenMic => (VoiceChat::OpenMic, c.voice_demo),
            VoiceMode::Demo => (if c.voice == VoiceChat::Off { VoiceChat::PushToTalk } else { c.voice }, true),
        };
        if let Err(e) = crate::settings::set_voice(chat, demo) {
            crate::say(&format!("the voice choice was not kept: {e}"));
        }
    }

    /// The lobby asked for again, for the window to show (not said in the
    /// chat: `/voice rooms` says it).
    pub fn voice_refresh_lobby(&mut self) {
        self.voice.voice.refresh_lobby(false);
    }

    /// A room joined, made when it is not there: locked with `password` (""
    /// none), unlisted (only when this join makes it).
    pub fn voice_join(&mut self, name: &str, password: &str, unlisted: bool) {
        self.voice.voice.join(name, password, unlisted);
    }

    pub fn voice_leave(&mut self) {
        self.voice.voice.leave();
    }

    /// A member's own volume, 0..2.
    pub fn voice_set_volume(&mut self, peer: u32, volume: f32) {
        self.voice.voice.set_volume(peer, volume);
    }

    pub fn voice_set_muted(&mut self, peer: u32, muted: bool) {
        self.voice.voice.set_muted(peer, muted);
    }

    /// A click on a voice mark (`hud::Pick::VoiceMark`): that one muted, or
    /// heard again, and said.
    pub(super) fn voice_toggle_mute(&mut self, peer: u32) {
        let Some(name) = self.voice.voice.marks().into_iter().find(|m| m.peer == peer).map(|m| m.name) else { return };
        let muted = self.voice.voice.toggle_mute(peer);
        self.add_chat(format!("Voice: {name} {}.", if muted { "muted (click again to hear them)" } else { "heard again" }), LINE);
    }
}
