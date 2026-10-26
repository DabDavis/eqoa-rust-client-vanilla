//! The page's voice: its engine is web/voice.js (`window.eqoaVoice`, the
//! browser's WebRTC: one RTCPeerConnection to the forwarding server, the
//! microphone through getUserMedia, a gain a slot), driven from here through
//! `crate::voice::VoiceEngine`; and the voice demo's WebSocket (`socket`),
//! the browser's own. What crosses to the page is JSON text and numbers.

use crate::voice::demo::{Socket, SocketState};
use crate::voice::{IceServer, SignalKind, VoiceEngine, VoiceEvent};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, WebSocket};

/// `window.eqoaVoice.<name>(args)`, if the page has it.
fn call(name: &str, args: &[JsValue]) -> Option<JsValue> {
    let w = web_sys::window()?;
    let v = js_sys::Reflect::get(&w, &"eqoaVoice".into()).ok().filter(|v| v.is_object())?;
    let f = js_sys::Reflect::get(&v, &name.into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    let a = js_sys::Array::new();
    for x in args {
        a.push(x);
    }
    f.apply(&v, &a).ok()
}

/// A JSON text the page answered, parsed.
fn json_of(v: Option<JsValue>) -> Value {
    v.and_then(|v| v.as_string()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

/// The page's engine (web/voice.js).
#[derive(Default)]
pub struct WebEngine {
    told_missing: bool,
    events: Vec<VoiceEvent>,
}

impl VoiceEngine for WebEngine {
    fn start(&mut self, ice: &[IceServer]) {
        let ice: Vec<Value> = ice.iter().map(|i| json!({ "urls": i.url, "username": i.username, "credential": i.credential })).collect();
        if call("start", &[Value::Array(ice).to_string().into()]).is_none() && !self.told_missing {
            self.told_missing = true;
            self.events.push(VoiceEvent::Note("Voice chat: this page has no voice; load it again.".to_string()));
        }
    }

    fn signal(&mut self, kind: SignalKind, text: &str) {
        call("signal", &[JsValue::from(kind.byte()), text.into()]);
    }

    fn take_outgoing(&mut self) -> Vec<(SignalKind, String)> {
        let Value::Array(a) = json_of(call("takeOutgoing", &[])) else { return Vec::new() };
        a.iter()
            .filter_map(|m| {
                let kind = SignalKind::of(m.get(0)?.as_u64()? as u8)?;
                Some((kind, m.get(1)?.as_str()?.to_string()))
            })
            .collect()
    }

    fn set_send(&mut self, channels: u8) {
        call("setSend", &[JsValue::from(channels)]);
    }

    fn set_peer_volume(&mut self, peer: u32, volume: f32) {
        call("setPeerVolume", &[JsValue::from(peer), JsValue::from(volume)]);
    }

    fn levels(&self) -> Vec<(u32, f32)> {
        let Value::Array(a) = json_of(call("levels", &[])) else { return Vec::new() };
        a.iter().filter_map(|m| Some((u32::try_from(m.get(0)?.as_u64()?).ok()?, m.get(1)?.as_f64()? as f32))).collect()
    }

    fn mic_level(&self) -> f32 {
        call("micLevel", &[]).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32
    }

    fn stop(&mut self) {
        call("stop", &[]);
    }

    fn take_events(&mut self) -> Vec<VoiceEvent> {
        let mut out = std::mem::take(&mut self.events);
        if let Value::Array(a) = json_of(call("takeEvents", &[])) {
            for e in a {
                match (e.get(0).and_then(Value::as_str), e.get(1).and_then(Value::as_str)) {
                    (Some("connected"), _) => out.push(VoiceEvent::Connected),
                    (Some("failed"), _) => out.push(VoiceEvent::Failed),
                    (Some("mic-denied"), _) => out.push(VoiceEvent::MicDenied),
                    (Some("note"), Some(t)) => out.push(VoiceEvent::Note(t.to_string())),
                    _ => {}
                }
            }
        }
        out
    }
}

/// The voice demo's socket: the browser's WebSocket.
struct PageSocket {
    ws: WebSocket,
    inbox: Rc<RefCell<VecDeque<String>>>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
}

impl Socket for PageSocket {
    fn send(&mut self, text: &str) {
        let _ = self.ws.send_with_str(text);
    }

    fn take(&mut self) -> Vec<String> {
        self.inbox.borrow_mut().drain(..).collect()
    }

    fn state(&self) -> SocketState {
        match self.ws.ready_state() {
            WebSocket::CONNECTING => SocketState::Connecting,
            WebSocket::OPEN => SocketState::Open,
            _ => SocketState::Closed,
        }
    }
}

impl Drop for PageSocket {
    fn drop(&mut self) {
        self.ws.set_onmessage(None);
        let _ = self.ws.close();
    }
}

/// A socket that never opened.
struct Shut;

impl Socket for Shut {
    fn send(&mut self, _text: &str) {}
    fn take(&mut self) -> Vec<String> {
        Vec::new()
    }
    fn state(&self) -> SocketState {
        SocketState::Closed
    }
}

pub fn socket(url: &str) -> Box<dyn Socket> {
    let Ok(ws) = WebSocket::new(url) else {
        super::log(&format!("voice demo: {url} would not open"));
        return Box::new(Shut);
    };
    let inbox = Rc::new(RefCell::new(VecDeque::new()));
    let q = inbox.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        if let Some(t) = e.data().as_string() {
            let mut q = q.borrow_mut();
            if q.len() < 512 {
                q.push_back(t);
            }
        }
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    Box::new(PageSocket { ws, inbox, _on_message: on_message })
}
