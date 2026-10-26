//! The spell-visual preview in a page (`crate::fxview`; `web/fxview.html`,
//! which the GM panel frames). The panel hands over the disc's files it needs
//! (`fxview`, as Blobs out of its own copy of the GM's disc) or the page's
//! own picker a disc image (`fxview_disc`); then the visual to play and what
//! it lands on (`fxview_play`), the rest as the GM changes it.

use super::{WebDisc, canvas, gfx, js, status};
use crate::fxview::{self, Aim, Command, Orbit};
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// `opts.<name>` as a string or a number, if given.
fn opt_str(opts: &JsValue, name: &str) -> Option<String> {
    let v = js_sys::Reflect::get(opts, &name.into()).ok()?;
    v.as_string().or_else(|| v.as_f64().map(|n| (n as u64).to_string()))
}

fn opt_f64(opts: &JsValue, name: &str) -> Option<f64> {
    js_sys::Reflect::get(opts, &name.into()).ok()?.as_f64()
}

/// What `opts` asks to play first: `{effect, castMs, aim}`.
fn first(opts: &JsValue) -> (Option<u32>, i32, Option<Aim>) {
    let effect = opt_str(opts, "effect").and_then(|e| fxview::parse_visual(&e));
    let cast = opt_f64(opts, "castMs").map_or(fxview::CAST_MS, |ms| ms as i32);
    let aim = opt_str(opts, "aim").and_then(|a| Aim::parse(&a));
    (effect, cast, aim)
}

/// The stage on the page's canvas, its loop handed to the page.
async fn start(lib: esf::Library, opts: &JsValue) -> Result<(), String> {
    let (effect, cast, aim) = first(opts);
    if let (Some(model), size) = (opt_str(opts, "dummy").and_then(|d| fxview::parse_visual(&d)), opt_f64(opts, "dummySize")) {
        fxview::send(Command::Dummy(model as i32, size.unwrap_or(0.0) as f32));
    }
    let pending = gfx(canvas()?).await?;
    fxview::run(lib, effect, cast, aim, Some(pending))
}

/// The preview from the files the panel hands over: `files` is an array of
/// `[name, Blob]` (the names on the disc, `fxview::FILES`; ITEM.ESF may be
/// left out), `opts` `{effect, castMs, aim, dummy, dummySize}`.
#[wasm_bindgen]
pub async fn fxview(files: js_sys::Array, opts: JsValue) -> Result<(), JsValue> {
    let r: Result<(), String> = async {
        let mut got = HashMap::new();
        for pair in files.iter() {
            let pair: js_sys::Array = pair.dyn_into().map_err(|_| "each file is [name, Blob]")?;
            let name = pair.get(0).as_string().ok_or("a file's name")?;
            let blob: web_sys::Blob = pair.get(1).dyn_into().map_err(|_| format!("{name}: not a Blob"))?;
            status(&format!("reading {name}"));
            let buf = wasm_bindgen_futures::JsFuture::from(blob.array_buffer()).await.map_err(|_| format!("{name}: could not be read"))?;
            got.insert(name, js_sys::Uint8Array::new(&buf).to_vec());
        }
        let lib = fxview::library(got)?;
        start(lib, &opts).await
    }
    .await;
    r.map_err(js)
}

/// The preview off a disc image picked in the page itself (the preview
/// opened on its own, not framed).
#[wasm_bindgen]
pub async fn fxview_disc(file: web_sys::File, opts: JsValue) -> Result<(), JsValue> {
    let r: Result<(), String> = async {
        let disc = WebDisc::open(file).await?;
        let mut got = HashMap::new();
        for name in fxview::FILES {
            status(&format!("reading {name}"));
            match disc.read(name).await {
                Ok(b) => {
                    got.insert(name.to_string(), b);
                }
                Err(e) if *name != "ITEM.ESF" => return Err(e),
                Err(_) => {}
            }
        }
        let lib = fxview::library(got)?;
        start(lib, &opts).await
    }
    .await;
    r.map_err(js)
}

/// Plays `effect` (a DictID: `0x1DAEB5B6`, the eight digits, or a number)
/// cast for `cast_ms`, landing on `aim` (`caster`, `human`, `dummy` or `both`,
/// the caster and a second human at once; empty leaves it as it was).
#[wasm_bindgen]
pub fn fxview_play(effect: String, cast_ms: f64, aim: String) -> bool {
    let Some(e) = fxview::parse_visual(&effect) else { return false };
    fxview::send(Command::Play { effect: e, cast_ms: cast_ms as i32, aim: Aim::parse(&aim) });
    true
}

/// Casts the visual again now.
#[wasm_bindgen]
pub fn fxview_again() {
    fxview::send(Command::Again);
}

/// What the cast lands on: `caster`, `human`, `dummy` or `both`.
#[wasm_bindgen]
pub fn fxview_aim(aim: String) -> bool {
    let Some(a) = Aim::parse(&aim) else { return false };
    fxview::send(Command::Aim(a));
    true
}

/// How far in front of the caster the target stands.
#[wasm_bindgen]
pub fn fxview_distance(d: f64) {
    fxview::send(Command::Distance(d as f32));
}

/// The training dummy's body (a CHAR.ESF DictID) and size.
#[wasm_bindgen]
pub fn fxview_dummy(model: String, size: f64) -> bool {
    let Some(m) = fxview::parse_visual(&model) else { return false };
    fxview::send(Command::Dummy(m as i32, size as f32));
    true
}

/// The camera: yaw and pitch in radians, zoom a multiple of the framing.
#[wasm_bindgen]
pub fn fxview_camera(yaw: f64, pitch: f64, zoom: f64) {
    fxview::send(Command::Camera(Orbit { yaw: yaw as f32, pitch: pitch as f32, zoom: zoom as f32 }));
}

/// The preview's last status line (what plays, at what).
#[wasm_bindgen]
pub fn fxview_status() -> String {
    fxview::status()
}

/// Every visual on the disc, in its order, as `0x1DAEB5B6`: what the
/// community page (`web/spellfx.html`) lists to be tried; empty until the
/// stage has started.
#[wasm_bindgen]
pub fn fxview_visuals() -> js_sys::Array {
    fxview::visuals().iter().map(|v| JsValue::from_str(&format!("0x{v:08X}"))).collect()
}
