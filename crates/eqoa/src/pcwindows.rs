//! PC modal window frames. The disc's controls, text and hit targets share a
//! uniformly zoomed coordinate system inside a separately resizable viewport.
use crate::ui::UiRaster;
use render::ui::{Quad, Ui};
use std::collections::BTreeMap;

const TITLE: f32 = 19.0;
const GUTTER: f32 = 10.0;
const OVERFLOW_EPSILON: f32 = 0.01;
const MIN_SIZE: [f32; 2] = [212.0, 80.0];
const BODY: [f32; 4] = [0.07, 0.065, 0.05, 0.90];
const METAL: [f32; 4] = [0.48, 0.43, 0.29, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect([f32; 4]);

impl Rect {
    fn size(self, i: usize) -> f32 { self.0[i + 2] - self.0[i] }
    fn has(self, p: [f32; 2]) -> bool {
        (0..2).all(|i| self.0[i] <= p[i] && p[i] < self.0[i + 2])
    }
    fn union(self, b: Self) -> Self {
        Self([self.0[0].min(b.0[0]), self.0[1].min(b.0[1]), self.0[2].max(b.0[2]), self.0[3].max(b.0[3])])
    }
    fn intersection(self, b: Self) -> Option<Self> {
        let r = Self([self.0[0].max(b.0[0]), self.0[1].max(b.0[1]), self.0[2].min(b.0[2]), self.0[3].min(b.0[3])]);
        (r.size(0) > 0.0 && r.size(1) > 0.0).then_some(r)
    }
}

fn quad_bounds(q: &Quad) -> Option<Rect> {
    if q.color[3] <= 0.0 { return None; }
    let mut r = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for [x, y] in q.pos {
        r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
    }
    Rect(r).intersection(Rect(q.clip.unwrap_or([0, 0, 640, 448]).map(|v| v as f32)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    // Fraction of the available travel. Windows at the right/bottom stay
    // there when the display changes size or aspect ratio.
    position: Option<[f32; 2]>,
    size: Option<[f32; 2]>,
    // Only an axis changed by the resize grip can require a scrollbar.
    // An older saved frame kept its size on pin and zoom as well.
    resized: Option<[bool; 2]>,
    zoom: u16,
    pinned: bool,
}

impl Default for Layout {
    fn default() -> Self { Self { position: None, size: None, resized: Some([false; 2]), zoom: 100, pinned: false } }
}

impl Layout {
    fn resized_axes(self, source: Rect, screen: Rect) -> [bool; 2] {
        self.resized.unwrap_or_else(|| {
            let Some(size) = self.size else { return [false; 2] };
            // Version one reserved a gutter on both axes, even at rest.
            std::array::from_fn(|i| {
                let old_natural = source.size(i) + GUTTER + if i == 1 { TITLE } else { 0.0 };
                (size[i] - old_natural.clamp(MIN_SIZE[i], screen.size(i))).abs() > 0.5
            })
        })
    }
    fn keep_frame(&mut self, view: View) {
        let size = [view.frame.size(0), view.frame.size(1)];
        self.size = Some(size);
        self.position = Some(view.position([view.frame.0[0], view.frame.0[1]], size));
    }
}

fn layouts(text: &str) -> BTreeMap<i32, Layout> {
    let pair = |a: &str, b: &str, min: [f32; 2], max: f32| -> Option<Option<[f32; 2]>> {
        let p: [f32; 2] = [a.parse().ok()?, b.parse().ok()?];
        if p == [-1.0; 2] { return Some(None); }
        (0..2).all(|i| (min[i]..=max).contains(&p[i])).then_some(Some(p))
    };
    text.lines().filter_map(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        let [id, x, y, w, h, zoom, pin, extra @ ..] = fields.as_slice() else { return None };
        let resized = match extra {
            [] => None,
            [axes] => {
                let bits = axes.parse::<u8>().ok().filter(|&n| n <= 3)?;
                Some([bits & 1 != 0, bits & 2 != 0])
            }
            _ => return None,
        };
        let zoom = zoom.parse().ok().filter(|z| (50..=200).contains(z))?;
        let pinned = match *pin { "1" => true, "0" => false, _ => return None };
        Some((id.parse().ok()?, Layout { position: pair(x, y, [0.0; 2], 1.0)?, size: pair(w, h, MIN_SIZE, 100_000.0)?, resized, zoom, pinned }))
    }).collect()
}

#[derive(Clone, Copy, Debug)]
struct View {
    key: i32,
    screen: Rect,
    source: Rect,
    frame: Rect,
    body: Rect,
    zoom: f32,
    max_zoom: f32,
    scroll: [bool; 2],
    pan: [f32; 2],
}

impl View {
    fn new(key: i32, screen: Rect, source: Rect, layout: Layout, pan: [f32; 2]) -> Self {
        let manual = layout.resized_axes(source, screen);
        let header = [0.0, TITLE];
        let limits: [f32; 2] = std::array::from_fn(|i| {
            let size = if manual[i] || layout.pinned { layout.size.map(|s| s[i]) } else { None };
            size.unwrap_or(screen.size(i)).clamp(MIN_SIZE[i], screen.size(i))
        });
        let requested = f32::from(layout.zoom) / 100.0;
        let room = |i: usize, bars: [bool; 2]| limits[i] - header[i] - if bars[1-i] { GUTTER } else { 0.0 };
        // Free axes grow with the contents, up to the display edge. At that
        // edge a new window fits uniformly instead of opening already clipped.
        let fit = |bars: [bool; 2]| (0..2).filter(|&i| !manual[i])
            .fold(2.0_f32, |scale, i| scale.min(room(i, bars) / source.size(i)));
        let needed = |scale: f32, bars| std::array::from_fn(|i| {
            manual[i] && source.size(i) * scale > room(i, bars) + OVERFLOW_EPSILON
        });
        let first = needed(requested.min(fit([false; 2])), [false; 2]);
        let max_zoom = fit(first);
        let zoom = requested.min(max_zoom);
        let scroll = needed(zoom, first);
        let mut frame = screen;
        for i in 0..2 {
            let natural = source.size(i) * zoom + header[i] + if scroll[1-i] { GUTTER } else { 0.0 };
            let span = if manual[i] || layout.pinned { limits[i] } else { natural.clamp(MIN_SIZE[i], limits[i]) };
            let travel = screen.size(i) - span;
            let origin = layout.position.map_or_else(
                || (source.0[i] - if i == 1 { TITLE } else { 0.0 }).clamp(screen.0[i], screen.0[i] + travel),
                |p| screen.0[i] + p[i] * travel,
            );
            frame.0[i] = origin;
            frame.0[i + 2] = origin + span;
        }
        let [l, t, r, b] = frame.0;
        let body = Rect([l, t + TITLE, r - if scroll[1] { GUTTER } else { 0.0 }, b - if scroll[0] { GUTTER } else { 0.0 }]);
        let pan = std::array::from_fn(|i| if scroll[i] { pan[i] } else { 0.0 });
        Self { key, screen, source, frame, body, zoom, max_zoom, scroll, pan }
    }
    fn overflow(self, i: usize) -> f32 { (self.source.size(i) * self.zoom - self.body.size(i)).max(0.0) }
    fn origin(self, i: usize) -> f32 { self.body.0[i] - self.pan[i] * self.overflow(i) }
    fn place(self, p: [f32; 2]) -> [f32; 2] {
        std::array::from_fn(|i| self.origin(i) + (p[i] - self.source.0[i]) * self.zoom)
    }
    fn local(self, p: [f32; 2]) -> [f32; 2] {
        std::array::from_fn(|i| self.source.0[i] + (p[i] - self.origin(i)) / self.zoom)
    }
    fn position(self, at: [f32; 2], size: [f32; 2]) -> [f32; 2] {
        std::array::from_fn(|i| ((at[i] - self.screen.0[i]) / (self.screen.size(i) - size[i]).max(0.01)).clamp(0.0, 1.0))
    }
    fn transform(self, q: &mut Quad) {
        let clip = q.clip.unwrap_or([0, 0, 640, 448]).map(|v| v as f32);
        let clip = Rect(clip).intersection(self.source).and_then(|r| {
            let [x, y] = self.place([r.0[0], r.0[1]]);
            let [right, bottom] = self.place([r.0[2], r.0[3]]);
            Rect([x, y, right, bottom]).intersection(self.body)
        });
        q.clip = Some(clip.map_or([0; 4], |r| r.0.map(|v| v.round() as i32)));
        q.pos = q.pos.map(|p| self.place(p));
    }
    fn track(self, i: usize) -> Rect {
        let [l, t, r, b] = self.body.0;
        if i == 0 { Rect([l, b, self.frame.0[2] - GUTTER, b + GUTTER]) }
        else { Rect([r, t, r + GUTTER, self.frame.0[3] - GUTTER]) }
    }
    fn thumb(self, i: usize) -> Rect {
        let mut thumb = self.track(i);
        let length = thumb.size(i);
        let visible = (length * self.body.size(i) / (self.body.size(i) + self.overflow(i))).clamp(GUTTER, length);
        thumb.0[i] += self.pan[i] * (length - visible);
        thumb.0[i + 2] = thumb.0[i] + visible;
        thumb
    }
    fn buttons(self) -> [(Rect, Action); 4] {
        let [_, t, r, _] = self.frame.0;
        [(137.,117.,Action::Smaller),(81.,61.,Action::Larger),(59.,32.,Action::Pin),(30.,1.,Action::Reset)]
            .map(|(a, b, k)| (Rect([r-a, t+1., r-b, t+TITLE-1.]), k))
    }
    fn grip(self) -> Rect {
        let [_, _, r, b] = self.frame.0;
        Rect([r - GUTTER, b - GUTTER, r, b])
    }
}

#[derive(Clone, Copy, Debug)]
enum Action { Move, Resize, Scroll(usize), Smaller, Larger, Pin, Reset }

#[derive(Clone, Copy)]
struct Drag { action: Action, from: [f32; 2], view: View, resized: [bool; 2] }

#[derive(Default)]
pub(crate) struct Windows {
    cache: Option<(usize, bool)>,
    saved: BTreeMap<i32, Layout>,
    view: Option<View>,
    drag: Option<Drag>,
}

/// Reserve the backdrop in draw order before the original game's controls.
/// Its bounds are known only after those controls have been laid out.
pub(crate) struct Layer { start: usize, backdrop: Option<usize>, previous_scrollbars: bool }

pub(crate) fn begin(r: &mut UiRaster) -> Layer {
    let backdrop = r.assets.white.map(|tex| {
        let index = r.ui.quads.len();
        r.ui.push_whole(Quad { tex, blend: 3, pos: [[0.0; 2]; 4], uv: [[0.5; 2]; 4], color: [0.045, 0.041, 0.032, 0.49], clip: None });
        index
    });
    let previous_scrollbars = r.native_list_scrollbars;
    if backdrop.is_some() && r.assets.font.is_some() { r.native_list_scrollbars = false; }
    Layer { start: r.ui.quads.len(), backdrop, previous_scrollbars }
}

impl Windows {
    fn save(&self, key: i32) {
        use std::fmt::Write;
        // Inventory and two simultaneous item comparisons can each hold a
        // frame. Keep the most recent choices made by the other open frames.
        let mut saved = layouts(&crate::settings::pc_window_layout());
        if saved.get(&key) == self.saved.get(&key) { return; }
        if let Some(&layout) = self.saved.get(&key) { saved.insert(key, layout); }
        else { saved.remove(&key); }
        let mut text = String::new();
        for (key, layout) in saved {
            let [x, y] = layout.position.unwrap_or([-1.0; 2]);
            let [w, h] = layout.size.unwrap_or([-1.0; 2]);
            write!(text, "{key} {x} {y} {w} {h} {} {}", layout.zoom, u8::from(layout.pinned)).unwrap();
            if let Some([width, height]) = layout.resized {
                write!(text, " {}", u8::from(width) + 2 * u8::from(height)).unwrap();
            }
            text.push('\n');
        }
        crate::settings::save_pc_window_layout(&text);
    }
    pub fn clear(&mut self) {
        self.release();
        self.view = None;
    }
    pub fn show(&mut self, r: &mut UiRaster, layer: Layer, window: Option<(i32, &str)>) {
        r.native_list_scrollbars = layer.previous_scrollbars;
        let Some((key, title)) = window else { self.clear(); return };
        let start = layer.start;
        if layer.backdrop.is_none() || r.assets.font.is_none() {
            self.clear();
            return;
        }
        let cache = crate::settings::hud_cache_key();
        if self.cache != Some(cache) {
            self.saved = layouts(&crate::settings::pc_window_layout());
            self.view = None;
            self.drag = None;
            self.cache = Some(cache);
        }
        let Some(mut source) = r.ui.quads[start..].iter().filter_map(quad_bounds).reduce(Rect::union) else {
            self.clear();
            return;
        };
        let previous = self.view.filter(|v| v.key == key);
        if previous.is_none() { self.release(); }
        // Animated glyphs and nested menus can change the drawn extent.
        // Keep the frame steady when previously visible content disappears.
        let layout = self.saved.get(&key).copied().unwrap_or_default();
        let screen = Rect([-r.ui.side(), -r.ui.top(), 640.0+r.ui.side(), 448.0+r.ui.top()]);
        if !layout.resized_axes(source, screen).contains(&true) && let Some(v) = previous {
            source = source.union(v.source);
        }
        let view = View::new(key, screen, source, layout, previous.map_or([0.0; 2], |v| v.pan));
        if let Some(index) = layer.backdrop {
            let [l, t, right, b] = view.body.0;
            r.ui.quads[index].pos = [[l,t], [right,t], [right,b], [l,b]];
        }
        for q in &mut r.ui.quads[start..] { view.transform(q); }
        self.view = Some(view);
        draw_frame(r, view, title, layout);
        r.ui.finish_floating_layer(start);
    }
    pub fn input(&self, key: Option<i32>, at: [f32; 2]) -> Option<[f32; 2]> {
        let Some(view) = self.view.filter(|v| Some(v.key) == key) else { return Some(at) };
        let local = view.local(at);
        (view.body.has(at) && view.source.has(local)).then_some(local)
    }
    pub fn covers(&self, key: i32, at: [f32; 2]) -> bool {
        self.view.is_some_and(|v| v.key == key && v.frame.has(at))
    }
    pub fn frame_body(&self) -> Option<[f32; 4]> { self.view.map(|v| v.body.0) }
    pub fn press(&mut self, key: Option<i32>, at: [f32; 2]) -> bool {
        let Some(v) = self.view.filter(|v| Some(v.key) == key && v.frame.has(at)) else { return false };
        let layout = self.saved.get(&v.key).copied().unwrap_or_default();
        let action = if at[1] < v.body.0[1] {
            v.buttons().into_iter().find(|(b, _)| b.has(at)).map_or(Action::Move, |(_, k)| k)
        } else if !layout.pinned && v.grip().has(at) { Action::Resize }
        else if let Some(axis) = (0..2).find(|&i| v.scroll[i] && v.track(i).has(at)) { Action::Scroll(axis) }
        else { return false };
        let mut drag_view = v;
        let resized = layout.resized_axes(v.source, v.screen);
        match action {
            Action::Pin => {
                let layout = self.saved.entry(v.key).or_default();
                layout.resized = Some(resized);
                if !layout.pinned { layout.keep_frame(v); }
                layout.pinned = !layout.pinned;
            }
            Action::Scroll(axis) => {
                if v.overflow(axis) > 0.1 && !v.thumb(axis).has(at) {
                    let thumb = v.thumb(axis).size(axis);
                    let track = v.track(axis);
                    drag_view.pan[axis] = ((at[axis] - track.0[axis] - thumb/2.) / (track.size(axis)-thumb).max(0.01)).clamp(0., 1.);
                    self.view = Some(drag_view);
                }
            }
            _ if layout.pinned => {}
            Action::Reset => { self.saved.remove(&v.key); drag_view.pan = [0.; 2]; self.view = Some(drag_view); }
            Action::Smaller | Action::Larger => {
                let pct = (v.zoom * 100.).round() as u16;
                let layout = self.saved.entry(v.key).or_default();
                layout.resized = Some(resized);
                if matches!(action, Action::Smaller) {
                    layout.zoom = (pct.saturating_sub(1) / 10 * 10).max(50);
                } else if v.zoom < v.max_zoom - 0.001 {
                    layout.zoom = ((pct / 10 + 1) * 10).min(200);
                }
            }
            _ => {}
        }
        self.drag = Some(Drag { action, from: at, view: drag_view, resized });
        true
    }
    pub fn drag(&mut self, key: Option<i32>, at: [f32; 2]) -> bool {
        let Some(drag) = self.drag else { return false };
        let v = drag.view;
        if Some(v.key) != key { self.clear(); return false; }
        let delta = [at[0] - drag.from[0], at[1] - drag.from[1]];
        if let Action::Scroll(i) = drag.action {
            let travel = v.track(i).size(i) - v.thumb(i).size(i);
            if travel > 0.01 && let Some(current) = self.view.as_mut() {
                current.pan[i] = (v.pan[i] + delta[i]/travel).clamp(0., 1.);
            }
        } else if matches!(drag.action, Action::Move | Action::Resize) && !self.saved.get(&v.key).is_some_and(|p| p.pinned) {
            let layout = self.saved.entry(v.key).or_default();
            let mut size = [v.frame.size(0), v.frame.size(1)];
            let mut pos = [v.frame.0[0], v.frame.0[1]];
            for i in 0..2 {
                if matches!(drag.action, Action::Resize) {
                    size[i] = (size[i] + delta[i]).clamp(MIN_SIZE[i], (v.screen.0[i+2] - pos[i]).max(MIN_SIZE[i]));
                } else { pos[i] += delta[i]; }
            }
            if matches!(drag.action, Action::Resize) {
                layout.size = Some(size);
                layout.resized = Some(std::array::from_fn(|i| drag.resized[i] || (size[i] - v.frame.size(i)).abs() > OVERFLOW_EPSILON));
            }
            layout.position = Some(v.position(pos, size));
        }
        true
    }
    pub fn release(&mut self) -> bool {
        let Some(drag) = self.drag.take() else { return false };
        if !matches!(drag.action, Action::Scroll(_)) && self.cache == Some(crate::settings::hud_cache_key()) { self.save(drag.view.key); }
        true
    }
    #[cfg(test)]
    pub fn geometry(&self) -> Option<(i32, [f32;4], [f32;4], u16)> {
        self.view.map(|v| (v.key, v.frame.0, v.source.0, (v.zoom*100.).round() as u16))
    }
    #[cfg(test)]
    pub fn project(&self, p: [f32;2]) -> [f32;2] { self.view.unwrap().place(p) }
    #[cfg(test)]
    pub fn scrollbars(&self) -> [bool; 2] { self.view.unwrap().scroll }
}

fn draw_frame(r: &mut UiRaster, view: View, title: &str, layout: Layout) {
    let (Some(white), Some(font)) = (r.assets.white, r.assets.font) else { return };
    let ui = &mut *r.ui;
    let (anchor, vertical, viewport, shift, hud_scale) = (ui.anchor, ui.vertical, ui.viewport.take(), ui.shift, ui.hud_scale);
    ui.anchor = render::ui::Anchor::Middle;
    ui.vertical = render::ui::Vertical::Middle;
    ui.shift = [0.0; 2];
    ui.hud_scale = render::ui::HudScale::IDENTITY;
    let fill = |ui: &mut Ui, rect: Rect, color| {
        let [l, t, r, b] = rect.0;
        ui.push_whole(Quad { tex: white, blend: 3, pos: [[l,t],[r,t],[r,b],[l,b]], uv: [[0.5;2];4], color, clip: None });
    };
    let scale = 13.0 / ui.fonts[font].height.max(1) as f32;
    let ink = [1.0, 0.96, 0.81, 1.0];
    let label = |ui: &mut Ui, text: &str, x: f32, y: f32, size: f32, clip: Rect, color| {
        let first = ui.quads.len();
        ui.text_scaled(font, x, y, size, color, &crate::ui::wide(text));
        for q in &mut ui.quads[first..] { q.clip = Some(clip.0.map(|n| n.round() as i32)); }
    };
    let [l, t, right, b] = view.frame.0;
    let header = Rect([l,t,right,t+TITLE]);
    fill(ui, header, BODY);
    fill(ui, Rect([l,t+TITLE-1.,right,t+TITLE]), METAL);
    label(ui, title, l + 5., t + 2., scale, Rect([l, t, right - 138., t + TITLE]), ink);
    for (btn, action) in view.buttons() {
        let enabled = match action {
            Action::Pin => true,
            Action::Smaller => !layout.pinned && view.zoom > 0.501,
            Action::Larger => !layout.pinned && view.zoom < view.max_zoom - 0.001,
            _ => !layout.pinned,
        };
        fill(ui, btn, if enabled { [0.26, 0.23, 0.16, 1.0] } else { BODY });
        let color = if enabled { ink } else { [0.55, 0.53, 0.46, 1.0] };
        if matches!(action, Action::Smaller | Action::Larger) {
            // The tiny disc-font hyphen can fall between samples at 640x480.
            let (x, y) = ((btn.0[0] + btn.0[2]) / 2., (btn.0[1] + btn.0[3]) / 2.);
            fill(ui, Rect([x - 4., y - 1., x + 4., y + 1.]), color);
            if matches!(action, Action::Larger) { fill(ui, Rect([x - 1., y - 4., x + 1., y + 4.]), color); }
            continue;
        }
        let text = match action {
            Action::Pin if layout.pinned => "Unpin",
            Action::Pin => "Pin",
            _ => "Reset",
        };
        let width = ui.fonts[font].string_width(&crate::ui::wide(text)).max(1) as f32;
        let fit = scale.min((btn.size(0) - 3.) / width);
        label(ui, text, btn.0[0] + (btn.size(0) - width * fit) / 2., t + 2., fit, btn, color);
    }
    label(ui, &format!("{:.0}%", view.zoom * 100.), right - 116., t + 2., scale, Rect([right - 117., t, right - 81., t + TITLE]), ink);
    for i in 0..2 {
        if !view.scroll[i] { continue; }
        fill(ui, view.track(i), BODY);
        fill(ui, view.thumb(i), METAL);
    }
    if !layout.pinned {
        fill(ui, view.grip(), BODY);
        for inset in [5., 9.] {
            ui.push_whole(Quad {
                tex: white, blend: 3, color: ink, uv: [[0.5; 2]; 4], clip: None,
                pos: [[right-inset,b-1.], [right-1.,b-inset], [right-1.,b-inset+1.5], [right-inset+1.5,b-1.]],
            });
        }
    }
    (ui.anchor, ui.vertical, ui.viewport, ui.shift, ui.hud_scale) = (anchor, vertical, viewport, shift, hud_scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_follow_the_resized_axes_and_give_space_and_input_back_when_the_content_fits() {
        let screen = Rect([0., 0., 640., 448.]);
        let source = Rect([0., 0., 640., 350.]);
        let make = |size, axes, zoom| View::new(5, screen, source, Layout {
            size, resized: Some(axes), zoom, ..Layout::default()
        }, [1.; 2]);
        let full = make(None, [false; 2], 100);
        assert_eq!(full.scroll, [false; 2]);
        assert_eq!([full.body.size(0), full.body.size(1)], [640., 350.]);
        assert_eq!(full.pan, [0.; 2]);
        let mut windows = Windows { view: Some(full), ..Default::default() };
        for at in [[638., 200.], [280., 367.]] {
            assert!(windows.input(Some(5), at).is_some());
            assert!(!windows.press(Some(5), at), "a hidden track must not intercept {at:?}");
        }
        assert_eq!(make(None, [false; 2], 200).scroll, [false; 2], "zoom alone fits the display");
        for (size, axes, visible) in [
            ([480., 369.], [true, false], [true, false]),
            ([640., 240.], [false, true], [false, true]),
            ([480., 240.], [true; 2], [true; 2]),
            ([640., 369.], [true; 2], [false; 2]),
        ] {
            let v = make(Some(size), axes, 100);
            assert_eq!(v.scroll, visible);
            for i in 0..2 {
                assert_eq!(v.pan[i] != 0., visible[i]);
                if !visible[i] { assert!(v.overflow(i) <= OVERFLOW_EPSILON, "unreachable content on axis {i}"); }
            }
        }
    }

    #[test]
    fn old_pinned_or_zoomed_frames_at_the_original_size_are_not_manual_resizes() {
        let saved = layouts("5 -1 -1 650 379 120 0\n13 0 0 650 379 100 1\n1013 0 0 410 379 100 0\n");
        let screen = Rect([-100., 0., 740., 448.]);
        let source = Rect([0., 0., 640., 350.]);
        for key in [5, 13] {
            let v = View::new(key, screen, source, saved[&key], [1.; 2]);
            assert_eq!(v.scroll, [false; 2]);
        }
        assert_eq!(View::new(1013, screen, source, saved[&1013], [0.; 2]).scroll, [true, false]);
    }

    #[test]
    fn a_resized_zoomed_window_can_reach_both_far_edges_without_stretching_its_contents() {
        let layout = Layout { size: Some([260., 150.]), resized: Some([true; 2]), zoom: 150, ..Layout::default() };
        let view = View::new(5, Rect([0., 0., 640., 448.]), Rect([10., 20., 620., 380.]), layout, [0.; 2]);
        let [x, y] = view.place([40., 50.]);
        assert_eq!(view.place([50., 60.]), [x + 15., y + 15.]);
        let mut windows = Windows { view: Some(view), ..Default::default() };
        for i in 0..2 {
            let thumb = windows.view.unwrap().thumb(i).0;
            let mut at = [(thumb[0] + thumb[2]) / 2., (thumb[1] + thumb[3]) / 2.];
            assert!(windows.press(Some(5), at));
            at[i] = view.track(i).0[i + 2];
            assert!(windows.drag(Some(5), at));
            assert!(windows.release());
        }
        let view = windows.view.unwrap();
        let last = [view.source.0[2] - 1., view.source.0[3] - 1.];
        assert_eq!(windows.input(Some(5), view.place(last)), Some(last));
        assert_eq!(windows.input(Some(5), [view.frame.0[0], view.frame.0[1]]), None);
    }

    #[test]
    fn pinned_window_keeps_its_size_when_a_submenu_grows_and_another_window_saves() {
        crate::settings::testing::reset(false);
        crate::settings::set_ui_mode(true).unwrap();
        let screen = Rect([-100., 0., 740., 448.]);
        let source = Rect([20., 80., 320., 300.]);
        let mut windows = Vec::new();
        for key in [5, 1013] {
            let view = View::new(key, screen, source, Layout::default(), [0.; 2]);
            let mut frame = Windows { cache: Some(crate::settings::hud_cache_key()), view: Some(view), ..Default::default() };
            assert!(frame.press(Some(key), [view.frame.0[2] - 45., view.frame.0[1] + 9.]));
            windows.push(frame);
        }
        for mut window in windows {
            assert!(window.release());
            let v = window.view.unwrap();
            let grown = View::new(v.key, screen, Rect([0., 20., 615., 395.]), window.saved[&v.key], [0.; 2]);
            assert_eq!(grown.scroll, [false; 2], "pinning an untouched window does not create scrollbars");
            for (before, after) in v.frame.0.into_iter().zip(grown.frame.0) {
                assert!((before - after).abs() < 0.001, "pin held the frame edge at {before}, got {after}");
            }
        }
        let saved = layouts(&crate::settings::pc_window_layout());
        assert!(saved[&5].pinned && saved[&1013].pinned);
    }
}
