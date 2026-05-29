use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Pos2, Rect, Vec2};

pub(crate) const PROJECT_DIR: &str = "./project";
pub(crate) const PNG_EXPORT_DIR: &str = "./export/png";
pub(crate) const SVG_EXPORT_DIR: &str = "./export/svg";
pub(crate) const TEX_EXPORT_DIR: &str = "./export/tex";
pub(crate) const JS_EXPORT_DIR: &str = "./export/js";
pub(crate) const TEX_TEMPLATE_PATH: &str = "./template/art.tex";

fn ensure_dir(p: impl AsRef<Path>) -> PathBuf {
    let pb = p.as_ref().to_path_buf();
    let _ = std::fs::create_dir_all(&pb);
    pb
}

fn bijective_base26(mut n: usize) -> String {
    let mut chars = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        chars.push((b'A' + rem as u8) as char);
        n = (n - 1) / 26;
    }
    chars.iter().rev().collect()
}

use crate::curve::{Arr, CurveSet, Group, P, PALETTE};
use crate::image_ref::ReferenceImage;
use crate::persist::{CameraState, Project};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HandleId {
    Bezier(usize, Arr, usize),
    EllipseCenter(usize),
    EllipseRx(usize),
    EllipseRy(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorPickTarget {
    Stroke(usize),
    Fill(usize),
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderMode {
    Real,
    StrokeOnly,
    FillOnly,
    Both,
}

impl RenderMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            RenderMode::Real => "Normal",
            RenderMode::StrokeOnly => "Outline",
            RenderMode::FillOnly => "Fill",
            RenderMode::Both => "Outline + fill",
        }
    }

    pub(crate) fn effective_stroke(self, curve_stroke: bool) -> bool {
        match self {
            RenderMode::Real => curve_stroke,
            RenderMode::StrokeOnly | RenderMode::Both => true,
            RenderMode::FillOnly => false,
        }
    }

    pub(crate) fn effective_fill(self, curve_fill: bool) -> bool {
        match self {
            RenderMode::Real | RenderMode::FillOnly | RenderMode::Both => curve_fill,
            RenderMode::StrokeOnly => false,
        }
    }
}

pub struct App {
    pub(crate) curves: Vec<CurveSet>,
    pub(crate) selected: usize,
    pub(crate) groups: Vec<Group>,
    pub(crate) next_group_id: u64,
    pub(crate) new_curve_group_id: Option<u64>,

    pub(crate) center_x: f32,
    pub(crate) center_y: f32,
    pub(crate) scale: f32,

    pub(crate) dragging_handle: Option<HandleId>,
    pub(crate) dragging_image: bool,
    pub(crate) drag_image_offset: Vec2,
    pub(crate) panning: bool,
    pub(crate) link_continuity: bool,

    pub(crate) samples_per_segment: usize,
    pub(crate) show_grid: bool,
    pub(crate) show_axes: bool,
    pub(crate) show_handles_all: bool,
    pub(crate) background: [u8; 3],

    pub(crate) reference_image: Option<ReferenceImage>,
    pub(crate) image_drag_enabled: bool,
    pub(crate) pending_fit_view: bool,

    pub(crate) color_pick_target: Option<ColorPickTarget>,
    pub(crate) last_picked_color: Option<[u8; 4]>,

    pub(crate) export_path: String,
    pub(crate) export_w: u32,
    pub(crate) export_h: u32,
    pub(crate) export_transparent: bool,
    pub(crate) export_samples: usize,
    pub(crate) svg_export_path: String,
    pub(crate) tex_export_path: String,
    pub(crate) js_export_path: String,
    pub(crate) last_msg: Option<String>,

    pub(crate) new_curve_name: String,
    pub(crate) new_curve_kind: crate::curve::CurveKind,
    pub(crate) current_project_path: Option<PathBuf>,

    pub(crate) undo_stack: Vec<Vec<CurveSet>>,
    pub(crate) redo_stack: Vec<Vec<CurveSet>>,
    pub(crate) last_committed_curves: Vec<CurveSet>,

    pub(crate) playback_active: bool,
    pub(crate) playback_progress: f32,
    pub(crate) playback_duration_secs: f32,
    pub(crate) playback_loop: bool,
    pub(crate) playback_last_tick: Option<std::time::Instant>,

    pub(crate) next_created: u64,

    pub(crate) render_mode: RenderMode,

    pub(crate) edit_hide_handles: bool,
    pub(crate) edit_hide_control_polygon: bool,
    pub(crate) edit_show_all_strokes: bool,
    pub(crate) edit_show_all_fills: bool,
    pub(crate) edit_hide_all_fills: bool,
}

impl App {
    pub fn new() -> Self {
        let default_group = Group::new(1, "A", "A");
        let mut first_curve = CurveSet::empty("Curve 1", PALETTE[0]);
        first_curve.group_id = Some(default_group.id);
        let initial = vec![first_curve];
        Self {
            curves: initial.clone(),
            selected: 0,
            groups: vec![default_group.clone()],
            next_group_id: 2,
            new_curve_group_id: Some(default_group.id),
            center_x: 0.0,
            center_y: 0.0,
            scale: 70.0,
            dragging_handle: None,
            dragging_image: false,
            drag_image_offset: Vec2::ZERO,
            panning: false,
            link_continuity: true,
            samples_per_segment: 32,
            show_grid: true,
            show_axes: true,
            show_handles_all: true,
            background: [245, 245, 250],
            reference_image: None,
            image_drag_enabled: false,
            pending_fit_view: false,
            color_pick_target: None,
            last_picked_color: None,
            export_path: format!("{}/output.png", PNG_EXPORT_DIR),
            export_w: 1024,
            export_h: 1024,
            export_transparent: false,
            export_samples: 64,
            svg_export_path: format!("{}/output.svg", SVG_EXPORT_DIR),
            tex_export_path: format!("{}/output.tex", TEX_EXPORT_DIR),
            js_export_path: format!("{}/output.js", JS_EXPORT_DIR),
            last_msg: None,
            new_curve_name: String::new(),
            new_curve_kind: crate::curve::CurveKind::Bezier,
            current_project_path: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_committed_curves: initial,
            playback_active: false,
            playback_progress: 1.0,
            playback_duration_secs: 5.0,
            playback_loop: false,
            playback_last_tick: None,
            next_created: 0,
            render_mode: RenderMode::Real,
            edit_hide_handles: false,
            edit_hide_control_polygon: false,
            edit_show_all_strokes: false,
            edit_show_all_fills: false,
            edit_hide_all_fills: false,
        }
    }

    pub(crate) fn sync_created_at(&mut self) {
        let max_existing = self
            .curves
            .iter()
            .flat_map(|c| c.created_at.iter().copied())
            .max();
        if let Some(m) = max_existing {
            if self.next_created <= m {
                self.next_created = m + 1;
            }
        }
        for c in &mut self.curves {
            let want = c.draw_units();
            if c.created_at.len() > want {
                c.created_at.truncate(want);
            }
            while c.created_at.len() < want {
                c.created_at.push(self.next_created);
                self.next_created += 1;
            }
        }
    }

    pub(crate) fn total_draw_units(&self) -> usize {
        self.curves
            .iter()
            .filter(|c| c.visible)
            .map(|c| c.draw_units())
            .sum()
    }

    pub(crate) fn tick_playback(&mut self, ctx: &egui::Context) {
        if !self.playback_active {
            self.playback_last_tick = None;
            return;
        }
        let now = std::time::Instant::now();
        if let Some(prev) = self.playback_last_tick {
            let dt = (now - prev).as_secs_f32();
            if self.playback_duration_secs > 0.001 {
                self.playback_progress += dt / self.playback_duration_secs;
            }
            if self.playback_progress >= 1.0 {
                if self.playback_loop {
                    self.playback_progress = 0.0;
                } else {
                    self.playback_progress = 1.0;
                    self.playback_active = false;
                    self.playback_last_tick = None;
                    return;
                }
            }
        }
        self.playback_last_tick = Some(now);
        ctx.request_repaint();
    }

    pub(crate) fn playback_play(&mut self) {
        if self.playback_progress >= 1.0 {
            self.playback_progress = 0.0;
        }
        self.playback_active = true;
        self.playback_last_tick = None;
    }

    pub(crate) fn playback_pause(&mut self) {
        self.playback_active = false;
        self.playback_last_tick = None;
    }

    pub(crate) fn playback_stop(&mut self) {
        self.playback_active = false;
        self.playback_progress = 0.0;
        self.playback_last_tick = None;
    }

    pub(crate) fn playback_seek_end(&mut self) {
        self.playback_active = false;
        self.playback_progress = 1.0;
        self.playback_last_tick = None;
    }

    pub(crate) fn commit_if_changed(&mut self) {
        if self.curves != self.last_committed_curves {
            let prev = std::mem::replace(&mut self.last_committed_curves, self.curves.clone());
            self.undo_stack.push(prev);
            self.redo_stack.clear();
            if self.undo_stack.len() > 200 {
                self.undo_stack.remove(0);
            }
        }
    }

    pub(crate) fn undo(&mut self) {
        self.commit_if_changed();
        if let Some(prev) = self.undo_stack.pop() {
            let now = std::mem::replace(&mut self.curves, prev.clone());
            self.redo_stack.push(now);
            self.last_committed_curves = prev;
            self.cancel_interactions();
            self.clamp_selection();
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            let now = std::mem::replace(&mut self.curves, next.clone());
            self.undo_stack.push(now);
            self.last_committed_curves = next;
            self.cancel_interactions();
            self.clamp_selection();
        }
    }

    fn cancel_interactions(&mut self) {
        self.dragging_handle = None;
        self.dragging_image = false;
        self.panning = false;
    }

    fn clamp_selection(&mut self) {
        if self.curves.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.curves.len() {
            self.selected = self.curves.len() - 1;
        }
    }

    pub(crate) fn w2s(&self, rect: Rect, p: P) -> Pos2 {
        Pos2::new(
            rect.center().x + (p.x - self.center_x) * self.scale,
            rect.center().y - (p.y - self.center_y) * self.scale,
        )
    }

    pub(crate) fn s2w(&self, rect: Rect, s: Pos2) -> P {
        P::new(
            self.center_x + (s.x - rect.center().x) / self.scale,
            self.center_y - (s.y - rect.center().y) / self.scale,
        )
    }

    pub(crate) fn ellipse_rx_endpoint(c: &CurveSet) -> P {
        let r = c.ellipse_rot_deg.to_radians();
        P::new(
            c.ellipse_cx + c.ellipse_rx * r.cos(),
            c.ellipse_cy + c.ellipse_rx * r.sin(),
        )
    }

    pub(crate) fn ellipse_ry_endpoint(c: &CurveSet) -> P {
        let r = (c.ellipse_rot_deg + 90.0).to_radians();
        P::new(
            c.ellipse_cx + c.ellipse_ry * r.cos(),
            c.ellipse_cy + c.ellipse_ry * r.sin(),
        )
    }

    pub(crate) fn find_handle(&self, rect: Rect, pos: Pos2) -> Option<HandleId> {
        use crate::curve::CurveKind;
        let ci = self.selected;
        let c = self.curves.get(ci)?;
        if !c.visible || !c.show_handles {
            return None;
        }
        let threshold_sq = 30.0 * 30.0;
        let mut best: Option<(HandleId, f32)> = None;
        let mut consider = |h: HandleId, p: P| {
            let sp = self.w2s(rect, p);
            let d2 = (sp - pos).length_sq();
            if d2 < threshold_sq && best.map_or(true, |(_, bd)| d2 <= bd) {
                best = Some((h, d2));
            }
        };

        match c.kind {
            CurveKind::Bezier => {
                let n = c.n();
                if n == 0 {
                    return None;
                }
                let active = c.active_segment.min(n - 1);
                for a in Arr::all() {
                    let arr = c.get(a);
                    if active < arr.len() {
                        consider(HandleId::Bezier(ci, a, active), arr[active]);
                    }
                }
            }
            CurveKind::Ellipse => {
                consider(
                    HandleId::EllipseCenter(ci),
                    P::new(c.ellipse_cx, c.ellipse_cy),
                );
                consider(HandleId::EllipseRx(ci), Self::ellipse_rx_endpoint(c));
                consider(HandleId::EllipseRy(ci), Self::ellipse_ry_endpoint(c));
            }
        }
        best.map(|(h, _)| h)
    }

    fn apply_continuity_after_drag(&mut self, ci: usize, a: Arr, pi: usize) {
        if !self.link_continuity {
            return;
        }
        let c = &mut self.curves[ci];
        match a {
            Arr::S3 if pi + 1 < c.s1.len() => {
                c.s1[pi + 1] = c.s3[pi];
            }
            Arr::S1 if pi > 0 && pi - 1 < c.s3.len() => {
                c.s3[pi - 1] = c.s1[pi];
            }
            _ => {}
        }
    }

    pub(crate) fn update_dragged_handle(&mut self, world: P) {
        let Some(handle) = self.dragging_handle else {
            return;
        };
        match handle {
            HandleId::Bezier(ci, a, pi) => {
                let arr = self.curves[ci].get_mut(a);
                if pi < arr.len() {
                    arr[pi] = world;
                    self.apply_continuity_after_drag(ci, a, pi);
                }
            }
            HandleId::EllipseCenter(ci) => {
                let c = &mut self.curves[ci];
                c.ellipse_cx = world.x;
                c.ellipse_cy = world.y;
            }
            HandleId::EllipseRx(ci) => {
                let c = &mut self.curves[ci];
                let dx = world.x - c.ellipse_cx;
                let dy = world.y - c.ellipse_cy;
                let r = (dx * dx + dy * dy).sqrt();
                if r > 1e-4 {
                    c.ellipse_rx = r;
                    c.ellipse_rot_deg = dy.atan2(dx).to_degrees();
                }
            }
            HandleId::EllipseRy(ci) => {
                let c = &mut self.curves[ci];
                let dx = world.x - c.ellipse_cx;
                let dy = world.y - c.ellipse_cy;
                let r = (dx * dx + dy * dy).sqrt();
                if r > 1e-4 {
                    c.ellipse_ry = r;
                    c.ellipse_rot_deg = dy.atan2(dx).to_degrees() - 90.0;
                }
            }
        }
    }

    pub(crate) fn fit_to_curves(&mut self, viewport_px: Vec2) {
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        for c in &self.curves {
            if !c.visible {
                continue;
            }
            for p in c.sampled_path(32) {
                min_x = min_x.min(p.x);
                max_x = max_x.max(p.x);
                min_y = min_y.min(p.y);
                max_y = max_y.max(p.y);
                any = true;
            }
        }
        if let Some(img) = &self.reference_image {
            if img.visible && img.world_w > 0.0 && img.world_h > 0.0 {
                min_x = min_x.min(img.world_x);
                max_x = max_x.max(img.world_x + img.world_w);
                min_y = min_y.min(img.world_y);
                max_y = max_y.max(img.world_y + img.world_h);
                any = true;
            }
        }
        if any {
            self.center_x = (min_x + max_x) * 0.5;
            self.center_y = (min_y + max_y) * 0.5;
            let span_x = (max_x - min_x).max(0.01);
            let span_y = (max_y - min_y).max(0.01);
            let target_w = viewport_px.x * 0.85;
            let target_h = viewport_px.y * 0.85;
            self.scale = (target_w / span_x).min(target_h / span_y).max(2.0);
        }
    }

    fn make_project(&self) -> Project {
        Project {
            version: 1,
            curves: self.curves.clone(),
            reference_image: self.reference_image.as_ref().map(|i| {
                let mut clone = i.clone();
                clone.texture = None;
                clone.load_error = None;
                clone.raw_rgba = None;
                clone
            }),
            camera: CameraState {
                center_x: self.center_x,
                center_y: self.center_y,
                scale: self.scale,
            },
            samples_per_segment: self.samples_per_segment,
            background: self.background,
            groups: self.groups.clone(),
        }
    }

    fn apply_project(&mut self, p: Project) {
        self.curves = p.curves;
        self.groups = p.groups;
        if self.groups.is_empty() {
            self.groups.push(Group::new(1, "A", "A"));
        }
        self.next_group_id = self.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        let default_id = self.groups[0].id;
        for c in &mut self.curves {
            let needs_reassign = match c.group_id {
                None => true,
                Some(gid) => !self.groups.iter().any(|g| g.id == gid),
            };
            if needs_reassign {
                c.group_id = Some(default_id);
            }
        }
        if self.curves.is_empty() {
            let mut c = CurveSet::empty("Curve 1", PALETTE[0]);
            c.group_id = Some(default_id);
            self.curves.push(c);
        }
        for c in &mut self.curves {
            c.clamp_active_segment();
        }
        self.selected = 0;
        self.new_curve_group_id = Some(default_id);
        self.reference_image = p.reference_image.map(|mut i| {
            i.texture = None;
            i.load_error = None;
            i.raw_rgba = None;
            i
        });
        self.center_x = p.camera.center_x;
        self.center_y = p.camera.center_y;
        self.scale = p.camera.scale;
        self.samples_per_segment = p.samples_per_segment;
        self.background = p.background;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.last_committed_curves = self.curves.clone();
        self.sync_created_at();
    }

    pub(crate) fn add_group(&mut self) -> u64 {
        let name = self.next_default_folder_name();
        let id = self.next_group_id;
        self.next_group_id += 1;
        let g = Group::new(id, name.clone(), name);
        self.groups.push(g);
        id
    }

    fn next_default_folder_name(&self) -> String {
        let mut n = 1usize;
        loop {
            let candidate = bijective_base26(n);
            if !self.groups.iter().any(|g| g.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    pub(crate) fn remove_group(&mut self, group_id: u64) {
        if self.groups.len() <= 1 {
            return;
        }
        let Some(pos) = self.groups.iter().position(|g| g.id == group_id) else {
            return;
        };
        self.groups.remove(pos);
        let fallback_id = self.groups[0].id;
        for c in &mut self.curves {
            if c.group_id == Some(group_id) {
                c.group_id = Some(fallback_id);
            }
        }
        if self.new_curve_group_id == Some(group_id) {
            self.new_curve_group_id = Some(fallback_id);
        }
    }

    pub(crate) fn move_curve_forward(&mut self, i: usize) {
        if i + 1 < self.curves.len() {
            self.curves.swap(i, i + 1);
            self.remap_selected_after_swap(i, i + 1);
        }
    }

    pub(crate) fn move_curve_backward(&mut self, i: usize) {
        if i > 0 {
            self.curves.swap(i, i - 1);
            self.remap_selected_after_swap(i, i - 1);
        }
    }

    pub(crate) fn move_curve_to_front(&mut self, i: usize) {
        let last = self.curves.len().saturating_sub(1);
        if i >= last {
            return;
        }
        let c = self.curves.remove(i);
        self.curves.push(c);
        self.remap_selected_after_move(i, last);
    }

    pub(crate) fn move_curve_to_back(&mut self, i: usize) {
        if i == 0 || i >= self.curves.len() {
            return;
        }
        let c = self.curves.remove(i);
        self.curves.insert(0, c);
        self.remap_selected_after_move(i, 0);
    }

    fn remap_selected_after_swap(&mut self, a: usize, b: usize) {
        if self.selected == a {
            self.selected = b;
        } else if self.selected == b {
            self.selected = a;
        }
    }

    fn remap_selected_after_move(&mut self, from: usize, to: usize) {
        if self.selected == from {
            self.selected = to;
        } else if from < to && self.selected > from && self.selected <= to {
            self.selected -= 1;
        } else if from > to && self.selected >= to && self.selected < from {
            self.selected += 1;
        }
    }

    pub(crate) fn save_current(&mut self) {
        let Some(path) = self.current_project_path.clone() else {
            self.save_dialog();
            return;
        };
        let proj = self.make_project();
        self.last_msg = Some(match proj.save_to(&path) {
            Ok(()) => format!("Saved project → {}", path.display()),
            Err(e) => format!("Save error: {e}"),
        });
    }

    pub(crate) fn save_dialog(&mut self) {
        let initial = self
            .current_project_path
            .clone()
            .unwrap_or_else(|| ensure_dir(PROJECT_DIR).join("project.json"));
        let mut dlg = rfd::FileDialog::new().add_filter("project", &["json"]);
        if let Some(name) = initial.file_name().and_then(|s| s.to_str()) {
            dlg = dlg.set_file_name(name);
        }
        if let Some(dir) = initial.parent() {
            if dir.as_os_str().len() > 0 {
                dlg = dlg.set_directory(dir);
            }
        }
        if let Some(path) = dlg.save_file() {
            let proj = self.make_project();
            self.last_msg = Some(match proj.save_to(&path) {
                Ok(()) => {
                    self.current_project_path = Some(path.clone());
                    format!("Saved project → {}", path.display())
                }
                Err(e) => format!("Save error: {e}"),
            });
        }
    }

    pub(crate) fn load_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("project", &["json"]);
        let default_dir = self
            .current_project_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| ensure_dir(PROJECT_DIR));
        dlg = dlg.set_directory(&default_dir);
        if let Some(path) = dlg.pick_file() {
            match Project::load_from(&path) {
                Ok(proj) => {
                    self.apply_project(proj);
                    self.current_project_path = Some(path.clone());
                    self.last_msg = Some(format!("Loaded project ← {}", path.display()));
                }
                Err(e) => {
                    self.last_msg = Some(format!("Load error: {e}"));
                }
            }
        }
    }

    pub(crate) fn load_image_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter(
            "Image",
            &["png", "jpg", "jpeg", "bmp", "webp"],
        );
        if let Some(p) = &self.current_project_path {
            if let Some(dir) = p.parent() {
                if dir.as_os_str().len() > 0 {
                    dlg = dlg.set_directory(dir);
                }
            }
        }
        if let Some(path) = dlg.pick_file() {
            self.reference_image = Some(ReferenceImage::new(path.clone()));
            self.pending_fit_view = true;
            self.last_msg = Some(format!("Image queued ← {}", path.display()));
        }
    }

    pub(crate) fn apply_picked_color(&mut self, target: ColorPickTarget, rgba: [u8; 4]) {
        match target {
            ColorPickTarget::Stroke(ci) => {
                if let Some(c) = self.curves.get_mut(ci) {
                    c.color = [rgba[0], rgba[1], rgba[2]];
                }
            }
            ColorPickTarget::Fill(ci) => {
                if let Some(c) = self.curves.get_mut(ci) {
                    c.fill_color = rgba;
                }
            }
            ColorPickTarget::Background => {
                self.background = [rgba[0], rgba[1], rgba[2]];
            }
        }
        self.last_picked_color = Some(rgba);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::light());

        if let Some(img) = &mut self.reference_image {
            img.ensure_loaded(ctx, 10.0);
        }

        self.sync_created_at();
        self.tick_playback(ctx);

        if self.pending_fit_view {
            let ready = self
                .reference_image
                .as_ref()
                .map_or(false, |i| i.is_ready());
            if ready {
                let size = ctx.screen_rect().size();
                self.fit_to_curves(size);
                self.pending_fit_view = false;
            }
        }

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| self.top_bar(ui));
        egui::SidePanel::left("left_panel")
            .default_width(400.0)
            .min_width(320.0)
            .show(ctx, |ui| self.left_panel(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ui));

        let undo = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Z,
            ))
        });
        let redo = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Y,
            )) || i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            ))
        });
        let save = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::S,
            ))
        });
        let enter =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let escape =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));

        if escape {
            self.color_pick_target = None;
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if save {
            self.save_current();
        }
        if enter && self.color_pick_target.is_none() && !self.curves.is_empty() {
            let sel = self.selected.min(self.curves.len() - 1);
            if self.curves[sel].is_bezier() {
                self.curves[sel].append_segment();
            }
        }

        let interacting = ctx.input(|i| {
            i.pointer.primary_down()
                || i.pointer.secondary_down()
                || i.pointer.middle_down()
        });
        if !interacting {
            self.commit_if_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::bijective_base26;

    #[test]
    fn base26_single_letters() {
        assert_eq!(bijective_base26(1), "A");
        assert_eq!(bijective_base26(2), "B");
        assert_eq!(bijective_base26(26), "Z");
    }

    #[test]
    fn base26_double_letters() {
        assert_eq!(bijective_base26(27), "AA");
        assert_eq!(bijective_base26(28), "AB");
        assert_eq!(bijective_base26(52), "AZ");
        assert_eq!(bijective_base26(53), "BA");
        assert_eq!(bijective_base26(702), "ZZ");
    }

    #[test]
    fn base26_triple_letters() {
        assert_eq!(bijective_base26(703), "AAA");
    }
}
