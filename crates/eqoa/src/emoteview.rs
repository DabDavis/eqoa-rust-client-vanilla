//! `eqoa emotes <body> <out.png> [pose ...] [cols=N] [size=WxH]`: one body
//! off the disc playing actions, a row each, a few moments of each across,
//! from the front a little to its left, under the GM preview's daylight
//! (`dressview`'s), with no window. For choosing by eye what a race's body
//! plays for each emote (`raceemote::TABLE`), and for its contact sheets.
//! Each action is set over the idle a moment after it starts, as a received
//! emote is.
//!
//! The poses, as words: `wave`, `bow`, `point`, `cheer` (the emote as this
//! client plays it for a race's player in that body: `raceemote`), `N` (the
//! body's own action N, as `SetAction` plays it), `human:N` (the human's
//! action N carried onto the body's skeleton, `raceemote::retarget`, from the
//! two bind poses), `idle:N` (the same from the poses the two idles stand
//! in, `raceemote::Base::Idle`). None: the four emotes. `cols=N` moments of each (default 3, at 1/4, 1/2 and 3/4
//! of the action), `size=WxH` one view's, `yaw=R` the view's turn round the
//! body (radians from its front; 0.6 unless said), `tight` framed on the body
//! standing rather than on every moment (for one that flies about). Every
//! view of a sheet is framed alike. The rows and their moments are printed,
//! in order.

use crate::raceemote;
use esf::Library;
use glam::{Mat4, Vec3};
use render::{Renderer, View, wgpu};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::{AnimScene, ONCE};
use vi::vect::Vect3;

/// The files a body needs, in the order the game registers them.
pub const FILES: &[&str] = &["CHARCUST.CSF", "CHARFACE.CSF", "CHAR.ESF"];

/// One row of a sheet.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// An emote (0 wave .. 3 cheer) as a race's player plays it.
    Emote(usize),
    /// The body's own action.
    Slot(i32),
    /// The human's action carried onto the body, measured from the bind
    /// poses or (`idle:`) the idles' stances.
    Human(i32, raceemote::Base),
}

impl Row {
    pub fn parse(w: &str) -> Option<Row> {
        if let Some(k) = raceemote::NAMES.iter().position(|n| *n == w) {
            return Some(Row::Emote(k));
        }
        if let Some(n) = w.strip_prefix("human:") {
            return n.parse().ok().filter(|n| (0..66).contains(n)).map(|n| Row::Human(n, raceemote::Base::Bind));
        }
        if let Some(n) = w.strip_prefix("idle:") {
            return n.parse().ok().filter(|n| (0..66).contains(n)).map(|n| Row::Human(n, raceemote::Base::Idle));
        }
        w.parse().ok().filter(|n| (0..66).contains(n)).map(Row::Slot)
    }

    pub fn label(&self) -> String {
        match self {
            Row::Emote(k) => raceemote::NAMES[*k].to_string(),
            Row::Slot(n) => format!("{n}"),
            Row::Human(n, raceemote::Base::Bind) => format!("human:{n}"),
            Row::Human(n, raceemote::Base::Idle) => format!("idle:{n}"),
        }
    }
}

/// The words: the rows, the moments of each and one view's size.
pub struct Words {
    pub rows: Vec<Row>,
    pub cols: usize,
    pub size: (u32, u32),
    /// Framed on the body standing (its idle, with room round it), not on
    /// every moment drawn: for a body that flies about (a pixie).
    pub tight: bool,
    /// The view's turn round the body, radians from its front (its front a
    /// little to its left, 0.6, unless said).
    pub yaw: f32,
}

pub fn parse(words: &[String]) -> Result<Words, String> {
    let (mut rows, mut cols, mut size, mut tight, mut yaw) = (Vec::new(), 3usize, (220u32, 300u32), false, 0.6f32);
    for w in words {
        if w == "tight" {
            tight = true;
        } else if let Some(v) = w.strip_prefix("yaw=") {
            yaw = v.parse().map_err(|e| format!("{w}: {e}"))?;
        } else if let Some(v) = w.strip_prefix("cols=") {
            cols = v.parse().map_err(|e| format!("{w}: {e}"))?;
        } else if let Some(v) = w.strip_prefix("size=") {
            let (a, b) = v.split_once('x').ok_or_else(|| format!("{w}: WxH"))?;
            size = (a.parse().map_err(|e| format!("{w}: {e}"))?, b.parse().map_err(|e| format!("{w}: {e}"))?);
        } else {
            rows.push(Row::parse(w).ok_or_else(|| format!("{w}: wave, bow, point, cheer, a slot 0-65, human:<slot>, idle:<slot> or tight"))?);
        }
    }
    if rows.is_empty() {
        rows = (0..4).map(Row::Emote).collect();
    }
    Ok(Words { rows, cols: cols.clamp(1, 12), size, tight, yaw })
}

/// The body made as a race's player's, `row` put on it: the sprite, its
/// clips and the action to play.
fn sprite_for(lib: &Library, model: i32, row: &Row) -> Result<(CSprite<Ee>, AnimScene<Ee>, i32), String> {
    let r = lib.lookup(model).ok_or_else(|| format!("no body {:#010x} in CHAR.ESF", model as u32))?;
    let mut anims = AnimScene::<Ee>::default();
    let mut cs = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut Rand(1)).ok_or("not a character")?;
    let action = match row {
        Row::Emote(k) => {
            if let Some(p) = raceemote::poses(model as u32) {
                raceemote::put(lib, &mut cs, &mut anims, p);
            }
            raceemote::EMOTES[*k]
        }
        Row::Slot(n) => *n,
        Row::Human(n, base) => {
            // the human's action n carried onto the body, played as an emote
            let human = raceemote::Human::load(lib).ok_or("no human")?;
            let ([a, b], weight) = human.carry(lib, *n, &cs, &anims, *base).ok_or_else(|| format!("the human has no action {n}"))?;
            anims.anims.push(a);
            let ia = anims.anims.len() as i32 - 1;
            anims.anims.push(b);
            let ib = anims.anims.len() as i32 - 1;
            let e = &mut cs.entries[56];
            e.animation = [ia, if cs.skel_type == 1 { ib } else { -1 }];
            e.weight = weight;
            e.sound = [0, 0];
            e.sync = ONCE;
            56
        }
    };
    Ok((cs, anims, action))
}

/// One posed moment: the centre transform and the skin palette.
struct Posed {
    centre: Matrix44<Ee>,
    palette: Vec<[[f32; 4]; 4]>,
}

/// `row` played from its start (the action set over the idle, as a
/// received emote is): its length (ms, to where the action is complete or
/// 4 s), and the moments `cols` across it.
fn play(lib: &Library, model: i32, row: &Row, cols: usize) -> Result<(i64, Posed, Vec<(i64, Posed)>), String> {
    let step = 16;
    // its length
    let (mut cs, anims, action) = sprite_for(lib, model, row)?;
    let mut sc = anims;
    let mut rand = Rand(1);
    // a moment of idle first, as a player stands before an emote
    let lead = 480;
    let mut t = 0;
    let mut handle = -1;
    let mut end = 4000 + lead;
    while t <= 4000 + lead {
        sc.time = t;
        if t == lead {
            handle = cs.set_action(action, &sc, &mut rand);
        } else {
            cs.set_locomotion(0, &sc, &mut rand);
        }
        cs.h.process(0, &sc);
        if t > lead && cs.is_action_complete(handle) {
            end = t;
            break;
        }
        t += step;
    }
    let len = end - lead;
    let times: Vec<i64> = (0..cols).map(|k| lead + len * (k as i64 + 1) / (cols as i64 + 1)).collect();
    // again, posed at each moment
    let (mut cs, mut sc, _) = sprite_for(lib, model, row)?;
    let mut rand = Rand(1);
    let mut out = Vec::new();
    let mut standing = None;
    let mut t = 0;
    let last = *times.last().unwrap_or(&0);
    let pose = |cs: &mut CSprite<Ee>, sc: &AnimScene<Ee>| {
        let centre = cs.h.calc_trans_about_center(&Matrix44::identity(), sc);
        cs.h.set_blend_matrices();
        Posed { centre, palette: cs.h.blend.iter().map(|m| m.to_cols()).collect() }
    };
    while t <= last {
        sc.time = t;
        if t == lead {
            cs.set_action(action, &sc, &mut rand);
        } else {
            cs.set_locomotion(0, &sc, &mut rand);
        }
        cs.h.process(0, &sc);
        if t == lead - step {
            standing = Some(pose(&mut cs, &sc));
        }
        if times.iter().any(|&m| m / step * step == t) {
            out.push((t - lead, pose(&mut cs, &sc)));
        }
        t += step;
    }
    Ok((len, standing.ok_or("no standing pose")?, out))
}

/// The sheet: `words`' rows down, their moments across, into `out`.
#[cfg(not(target_arch = "wasm32"))]
pub fn sheet(lib: &Library, model: i32, words: &Words, out: &str) -> Result<(), String> {
    let (w, h) = words.size;
    let r = lib.lookup(model).ok_or_else(|| format!("no body {:#010x} in CHAR.ESF", model as u32))?;
    let mesh = lib.build_model(r).ok_or("no mesh")?;
    let mut rows = Vec::new();
    let mut standing = Vec::new();
    for row in &words.rows {
        let (len, stand, posed) = play(lib, model, row, words.cols)?;
        eprintln!("{:#010x} {:>9}: {len} ms; at {:?}", model as u32, row.label(), posed.iter().map(|(t, _)| *t).collect::<Vec<_>>());
        rows.push(posed);
        standing.push(stand);
    }
    // framed alike: the bounds of every posed moment (`tight`: of the body
    // standing, with room round it)
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let framed: Vec<&Posed> = if words.tight { standing.iter().take(1).collect() } else { rows.iter().flatten().map(|(_, p)| p).collect() };
    for p in framed {
        let world = Mat4::from_cols_array_2d(&p.centre.to_cols());
        for (i, v) in mesh.pos.iter().enumerate() {
            let (b, wt) = (mesh.bones[i], mesh.weights[i]);
            let (mut q, mut sum) = (Vec3::ZERO, 0.0);
            for k in 0..4 {
                if b[k] != 255 && (b[k] as usize) < p.palette.len() && wt[k] > 0 {
                    q += Mat4::from_cols_array_2d(&p.palette[b[k] as usize]).transform_point3(Vec3::from(*v)) * wt[k] as f32;
                    sum += wt[k] as f32;
                }
            }
            let q = world.transform_point3(if sum > 0.0 { q / sum } else { Vec3::from(*v) });
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    if words.tight {
        let (m, half) = ((lo + hi) * 0.5, (hi - lo) * 0.5 * 1.45);
        (lo, hi) = (m - half, m + half);
    }
    let mid = (lo + hi) * 0.5;
    let fov = 30f32.to_radians();
    let reach = ((hi.y - lo.y) * 0.5 / 0.9).max((hi.x - lo.x).max(hi.z - lo.z) * 0.5 / 0.8 / (w as f32 / h as f32)) / (fov * 0.5).tan();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).map_err(|e| e.to_string())?;
    let mut rr = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let textures = render::library_textures(lib);
    let mut gpu = rr.upload(&device, &queue, &mesh, &[render::IDENTITY], &textures);
    let cols = words.cols;
    let (sw, sh) = (w * cols as u32, h * rows.len() as u32);
    let mut sheet = vec![0u8; (sw * sh * 4) as usize];
    let yaw = words.yaw;
    let dir = Vec3::new(yaw.sin() * 0.97, 0.24, yaw.cos() * 0.97).normalize();
    let eye = mid + dir * reach;
    let v = glam::camera::rh::view::look_at_mat4(eye, mid, Vec3::Y);
    let p = glam::camera::rh::proj::directx::perspective(fov, w as f32 / h as f32, reach * 0.05, reach * 4.0);
    let mut view = View::daylight(p * v);
    view.eye = eye;
    view.sun_dir = (Vec3::new(-dir.x, 0.0, -dir.z) * 0.6 + Vec3::new(dir.z, 0.0, -dir.x) * 0.25 + Vec3::new(0.0, -0.75, 0.0)).normalize();
    let at_eye = Vect3::new(F::new(eye.x), F::new(eye.y), F::new(eye.z));
    for (ri, posed) in rows.iter().enumerate() {
        for (ci, (_, p)) in posed.iter().enumerate() {
            rr.set_instances(&queue, &gpu, &[p.centre.to_cols()]);
            if !p.palette.is_empty() {
                rr.set_palette(&queue, &gpu, &p.palette);
            }
            crate::lod::place(&mut gpu, &p.centre, &at_eye);
            let rgba = render::capture(&device, &queue, &mut rr, w, h, &view, &[render::Draw::Model(&gpu)], None);
            for y in 0..h as usize {
                let src = &rgba[y * w as usize * 4..(y + 1) * w as usize * 4];
                let o = ((ri * h as usize + y) * sw as usize + ci * w as usize) * 4;
                sheet[o..o + w as usize * 4].copy_from_slice(src);
            }
        }
    }
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| format!("{out}: {e}"))?), sw, sh);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&sheet)).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words make the rows, moments, size, framing and turn they say;
    /// none is the four emotes; a stray word or a slot past the play list's
    /// 66 is refused (the control).
    #[test]
    fn rows_from_words() {
        let words = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        let w = parse(&words("wave 37 human:57 idle:61 cols=5 size=100x120 tight yaw=-0.3")).unwrap();
        assert_eq!(w.rows, vec![Row::Emote(0), Row::Slot(37), Row::Human(57, raceemote::Base::Bind), Row::Human(61, raceemote::Base::Idle)]);
        assert_eq!((w.cols, w.size, w.tight, w.yaw), (5, (100, 120), true, -0.3));
        assert_eq!(w.rows.iter().map(Row::label).collect::<Vec<_>>(), ["wave", "37", "human:57", "idle:61"]);
        let none = parse(&[]).unwrap();
        assert_eq!((none.rows, none.cols, none.tight), ((0..4).map(Row::Emote).collect::<Vec<_>>(), 3, false));
        assert!(parse(&words("wav")).is_err());
        assert!(parse(&words("66")).is_err());
        assert!(parse(&words("human:66")).is_err());
    }
}
