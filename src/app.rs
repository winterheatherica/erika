use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Pos2, Rect, Vec2};

pub(crate) const PROJECT_DIR: &str = "./project";
pub(crate) const PNG_EXPORT_DIR: &str = "./export/png";
pub(crate) const SVG_EXPORT_DIR: &str = "./export/svg";
pub(crate) const TEX_EXPORT_DIR: &str = "./export/tex";
pub(crate) const JS_EXPORT_DIR: &str = "./export/js";
pub(crate) const IMPORT_IMAGE_DIR: &str = "./import/image";
pub(crate) const TEX_TEMPLATE_PATH: &str = "./template/art.tex";

fn ensure_dir(p: impl AsRef<Path>) -> PathBuf {
    let pb = p.as_ref().to_path_buf();
    let _ = std::fs::create_dir_all(&pb);
    pb
}

fn copy_into_import(src: &Path) -> std::io::Result<PathBuf> {
    let dir = ensure_dir(IMPORT_IMAGE_DIR);
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("png");
    let mut dest = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while dest.exists() {
        dest = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    std::fs::copy(src, &dest)?;
    Ok(dest)
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

pub(crate) fn snap_angle(total: f32, snap: bool) -> f32 {
    if snap {
        let step = std::f32::consts::FRAC_PI_4;
        (total / step).round() * step
    } else {
        total
    }
}

use crate::model::curve::{Arr, CurveSet, Group, P, PALETTE};
use crate::model::image_ref::ReferenceImage;
use crate::model::persist::{CameraState, ExportSettings, Project};

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
pub(crate) enum AnimFormat {
    Svg,
    DesmosJs,
}

impl AnimFormat {
    pub(crate) fn label(self) -> &'static str {
        match self {
            AnimFormat::Svg => "SVG",
            AnimFormat::DesmosJs => "Desmos JS",
        }
    }
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
    pub(crate) multi_select: std::collections::BTreeSet<usize>,
    pub(crate) groups: Vec<Group>,
    pub(crate) next_group_id: u64,
    pub(crate) new_curve_group_id: Option<u64>,
    pub(crate) active_group_id: Option<u64>,

    pub(crate) center_x: f32,
    pub(crate) center_y: f32,
    pub(crate) scale: f32,

    pub(crate) dragging_handle: Option<HandleId>,
    pub(crate) dragging_curve_row: Option<usize>,
    pub(crate) dragging_image: bool,
    pub(crate) drag_image_offset: Vec2,
    pub(crate) dragging_selection: bool,
    pub(crate) dragging_rotation: bool,
    pub(crate) rotate_center: P,
    pub(crate) rotate_radius: f32,
    pub(crate) rotate_prev_raw: f32,
    pub(crate) rotate_total: f32,
    pub(crate) rotate_applied: f32,
    pub(crate) panning: bool,
    pub(crate) link_continuity: bool,

    pub(crate) samples_per_segment: usize,
    pub(crate) show_grid: bool,
    pub(crate) show_axes: bool,
    pub(crate) show_handles_all: bool,
    pub(crate) show_left_panel: bool,
    pub(crate) show_top_bar: bool,
    pub(crate) background: Option<[u8; 3]>,

    pub(crate) reference_images: Vec<ReferenceImage>,
    pub(crate) selected_image: usize,
    pub(crate) image_drag_enabled: bool,
    pub(crate) pending_fit_view: bool,

    pub(crate) show_gallery: bool,
    pub(crate) gallery_search: String,
    pub(crate) thumb_cache: std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>,

    pub(crate) show_trace: bool,
    pub(crate) trace_image: Option<PathBuf>,
    pub(crate) trace_threshold: u8,
    pub(crate) trace_invert: bool,
    pub(crate) trace_min_area: usize,
    pub(crate) trace_simplify: f32,
    pub(crate) trace_corner_deg: f32,
    pub(crate) trace_fill: bool,

    pub(crate) show_animate: bool,
    pub(crate) anim_format: AnimFormat,
    pub(crate) anim_from: Option<PathBuf>,
    pub(crate) anim_to: Option<PathBuf>,
    pub(crate) anim_default_dur: f32,
    pub(crate) anim_diff: Option<crate::export::anim::AnimDiff>,
    pub(crate) anim_js_files: Vec<Option<PathBuf>>,
    pub(crate) anim_js_durs: Vec<f32>,
    pub(crate) anim_js_normalize: bool,
    pub(crate) anim_js_points: usize,
    pub(crate) anim_js_diff: Option<crate::export::anim_js::JsDiff>,

    pub(crate) color_pick_target: Option<ColorPickTarget>,
    pub(crate) last_picked_color: Option<[u8; 4]>,

    pub(crate) export_name: String,
    pub(crate) export_w: u32,
    pub(crate) export_h: u32,
    pub(crate) export_transparent: bool,
    pub(crate) export_samples: usize,
    pub(crate) export_frame: Option<[f32; 4]>,
    pub(crate) export_frame_lock: bool,
    pub(crate) js_timelapse: bool,
    pub(crate) js_grouped: bool,
    pub(crate) last_msg: Option<String>,

    pub(crate) new_curve_name: String,
    pub(crate) new_curve_kind: crate::model::curve::CurveKind,
    pub(crate) current_project_path: Option<PathBuf>,

    pub(crate) undo_stack: Vec<(Vec<CurveSet>, Vec<Group>)>,
    pub(crate) redo_stack: Vec<(Vec<CurveSet>, Vec<Group>)>,
    pub(crate) last_committed_curves: Vec<CurveSet>,
    pub(crate) last_committed_groups: Vec<Group>,

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
    pub(crate) edit_hide_all_strokes: bool,
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
            multi_select: std::collections::BTreeSet::new(),
            groups: vec![default_group.clone()],
            next_group_id: 2,
            new_curve_group_id: Some(default_group.id),
            active_group_id: Some(default_group.id),
            center_x: 0.0,
            center_y: 0.0,
            scale: 70.0,
            dragging_handle: None,
            dragging_curve_row: None,
            dragging_image: false,
            drag_image_offset: Vec2::ZERO,
            dragging_selection: false,
            dragging_rotation: false,
            rotate_center: P::new(0.0, 0.0),
            rotate_radius: 0.0,
            rotate_prev_raw: 0.0,
            rotate_total: 0.0,
            rotate_applied: 0.0,
            panning: false,
            link_continuity: true,
            samples_per_segment: 32,
            show_grid: true,
            show_axes: true,
            show_handles_all: true,
            show_left_panel: true,
            show_top_bar: true,
            background: None,
            reference_images: Vec::new(),
            selected_image: 0,
            image_drag_enabled: false,
            pending_fit_view: false,
            show_gallery: false,
            gallery_search: String::new(),
            thumb_cache: std::collections::HashMap::new(),
            show_trace: false,
            trace_image: None,
            trace_threshold: 128,
            trace_invert: false,
            trace_min_area: 64,
            trace_simplify: 1.5,
            trace_corner_deg: 60.0,
            trace_fill: false,
            show_animate: false,
            anim_format: AnimFormat::Svg,
            anim_from: None,
            anim_to: None,
            anim_default_dur: 2.0,
            anim_diff: None,
            anim_js_files: vec![None, None],
            anim_js_durs: vec![2.0],
            anim_js_normalize: false,
            anim_js_points: 48,
            anim_js_diff: None,
            color_pick_target: None,
            last_picked_color: None,
            export_name: "output".to_string(),
            export_w: 1024,
            export_h: 1024,
            export_transparent: false,
            export_samples: 64,
            export_frame: None,
            export_frame_lock: false,
            js_timelapse: false,
            js_grouped: true,
            last_msg: None,
            new_curve_name: String::new(),
            new_curve_kind: crate::model::curve::CurveKind::Bezier,
            current_project_path: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_committed_curves: initial,
            last_committed_groups: vec![default_group],
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
            edit_hide_all_strokes: false,
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
        if self.curves != self.last_committed_curves || self.groups != self.last_committed_groups {
            let prev_c = std::mem::replace(&mut self.last_committed_curves, self.curves.clone());
            let prev_g = std::mem::replace(&mut self.last_committed_groups, self.groups.clone());
            self.undo_stack.push((prev_c, prev_g));
            self.redo_stack.clear();
            if self.undo_stack.len() > 200 {
                self.undo_stack.remove(0);
            }
        }
    }

    pub(crate) fn undo(&mut self) {
        self.commit_if_changed();
        if let Some((prev_c, prev_g)) = self.undo_stack.pop() {
            let now_c = std::mem::replace(&mut self.curves, prev_c.clone());
            let now_g = std::mem::replace(&mut self.groups, prev_g.clone());
            self.redo_stack.push((now_c, now_g));
            self.last_committed_curves = prev_c;
            self.last_committed_groups = prev_g;
            self.cancel_interactions();
            self.clamp_selection();
            self.reconcile_group_refs();
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some((next_c, next_g)) = self.redo_stack.pop() {
            let now_c = std::mem::replace(&mut self.curves, next_c.clone());
            let now_g = std::mem::replace(&mut self.groups, next_g.clone());
            self.undo_stack.push((now_c, now_g));
            self.last_committed_curves = next_c;
            self.last_committed_groups = next_g;
            self.cancel_interactions();
            self.clamp_selection();
            self.reconcile_group_refs();
        }
    }

    fn reconcile_group_refs(&mut self) {
        let first = self.groups.first().map(|g| g.id);
        let exists = |id: u64, groups: &[Group]| groups.iter().any(|g| g.id == id);
        if let Some(id) = self.active_group_id {
            if !exists(id, &self.groups) {
                self.active_group_id = first;
            }
        }
        match self.new_curve_group_id {
            Some(id) if exists(id, &self.groups) => {}
            _ => self.new_curve_group_id = first,
        }
        let max_id = self.groups.iter().map(|g| g.id).max().unwrap_or(0);
        if self.next_group_id <= max_id {
            self.next_group_id = max_id + 1;
        }
    }

    fn cancel_interactions(&mut self) {
        self.dragging_handle = None;
        self.dragging_curve_row = None;
        self.dragging_image = false;
        self.dragging_selection = false;
        self.dragging_rotation = false;
        self.panning = false;
    }

    fn clamp_selection(&mut self) {
        self.multi_select.clear();
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
        use crate::model::curve::CurveKind;
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
        for img in &self.reference_images {
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

    pub(crate) fn capture_export_frame(&mut self) {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        for c in &self.curves {
            if !c.visible {
                continue;
            }
            for p in c.sampled_path(32) {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
                any = true;
            }
        }
        if any {
            self.export_frame = Some([min_x, min_y, max_x, max_y]);
            self.export_frame_lock = true;
        }
    }

    fn make_project(&self) -> Project {
        Project {
            version: 1,
            curves: self.curves.clone(),
            reference_image: None,
            reference_images: self
                .reference_images
                .iter()
                .map(|i| {
                    let mut clone = i.clone();
                    clone.texture = None;
                    clone.load_error = None;
                    clone.raw_rgba = None;
                    clone
                })
                .collect(),
            camera: CameraState {
                center_x: self.center_x,
                center_y: self.center_y,
                scale: self.scale,
            },
            samples_per_segment: self.samples_per_segment,
            background: self.background,
            groups: self.groups.clone(),
            export: ExportSettings {
                name: self.export_name.clone(),
                width: self.export_w,
                height: self.export_h,
                transparent: self.export_transparent,
                samples: self.export_samples,
                timelapse: self.js_timelapse,
                grouped: self.js_grouped,
                frame: self.export_frame,
                frame_lock: self.export_frame_lock,
            },
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
        self.multi_select.clear();
        self.new_curve_group_id = Some(default_id);
        self.active_group_id = Some(default_id);
        let mut images = p.reference_images;
        if let Some(legacy) = p.reference_image {
            images.insert(0, legacy);
        }
        for i in &mut images {
            i.texture = None;
            i.load_error = None;
            i.raw_rgba = None;
        }
        self.reference_images = images;
        self.selected_image = 0;
        self.center_x = p.camera.center_x;
        self.center_y = p.camera.center_y;
        self.scale = p.camera.scale;
        self.samples_per_segment = p.samples_per_segment;
        self.background = p.background;
        self.export_name = p.export.name;
        self.export_w = p.export.width;
        self.export_h = p.export.height;
        self.export_transparent = p.export.transparent;
        self.export_samples = p.export.samples;
        self.export_frame = p.export.frame;
        self.export_frame_lock = p.export.frame_lock;
        self.js_timelapse = p.export.timelapse;
        self.js_grouped = p.export.grouped;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.last_committed_curves = self.curves.clone();
        self.last_committed_groups = self.groups.clone();
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
        self.curves.retain(|c| c.group_id != Some(group_id));
        if self.curves.is_empty() {
            let mut c = CurveSet::empty("Curve 1", PALETTE[0]);
            c.group_id = Some(fallback_id);
            self.curves.push(c);
        }
        self.clamp_selection();
        if self.new_curve_group_id == Some(group_id) {
            self.new_curve_group_id = Some(fallback_id);
        }
        if self.active_group_id == Some(group_id) {
            self.set_active_group(fallback_id);
        }
    }

    pub(crate) fn set_show_all_group(&mut self) {
        self.active_group_id = None;
    }

    pub(crate) fn set_active_group(&mut self, id: u64) {
        self.active_group_id = Some(id);
        self.new_curve_group_id = Some(id);
        let sel_in_group = self
            .curves
            .get(self.selected)
            .map_or(false, |c| c.group_id == Some(id));
        if !sel_in_group {
            if let Some(idx) = self.curves.iter().position(|c| c.group_id == Some(id)) {
                self.selected = idx;
            }
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

    pub(crate) fn move_curve_to(&mut self, from: usize, to: usize) {
        if from == to || from >= self.curves.len() || to >= self.curves.len() {
            return;
        }
        let c = self.curves.remove(from);
        self.curves.insert(to, c);
        self.remap_selected_after_move(from, to);
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

    pub(crate) fn duplicate_curve(&mut self, i: usize) {
        let Some(src) = self.curves.get(i) else {
            return;
        };
        let mut copy = src.clone();
        copy.name = format!("{}-copy", src.name);
        copy.created_at.clear();
        self.curves.insert(i + 1, copy);
        self.selected = i + 1;
    }

    pub(crate) fn duplicate_curves(&mut self, indices: &[usize]) {
        let mut idx: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| i < self.curves.len())
            .collect();
        idx.sort_unstable();
        idx.dedup();
        if idx.is_empty() {
            return;
        }
        let insert_at = idx[idx.len() - 1] + 1;
        let copies: Vec<CurveSet> = idx
            .iter()
            .map(|&i| {
                let mut copy = self.curves[i].clone();
                copy.name = format!("{}-copy", copy.name);
                copy.created_at.clear();
                copy
            })
            .collect();
        let n = copies.len();
        for (offset, copy) in copies.into_iter().enumerate() {
            self.curves.insert(insert_at + offset, copy);
        }
        self.selected = insert_at;
        self.multi_select = (insert_at..insert_at + n).collect();
    }

    pub(crate) fn translate_selection(&mut self, dx: f32, dy: f32) {
        let idx: Vec<usize> = self.multi_select.iter().copied().collect();
        for i in idx {
            if let Some(c) = self.curves.get_mut(i) {
                c.translate(dx, dy);
            }
        }
    }

    pub(crate) fn flip_selection(&mut self, horizontal: bool) {
        let Some(center) = self.selection_center() else {
            return;
        };
        let idx: Vec<usize> = self.multi_select.iter().copied().collect();
        for i in idx {
            if let Some(c) = self.curves.get_mut(i) {
                if horizontal {
                    c.flip_h(center.x);
                } else {
                    c.flip_v(center.y);
                }
            }
        }
    }

    pub(crate) fn rotate_selection(&mut self, center: P, angle: f32) {
        let idx: Vec<usize> = self.multi_select.iter().copied().collect();
        for i in idx {
            if let Some(c) = self.curves.get_mut(i) {
                c.rotate(center.x, center.y, angle);
            }
        }
    }

    pub(crate) fn selection_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        for &i in &self.multi_select {
            let Some(c) = self.curves.get(i) else {
                continue;
            };
            for p in c.sampled_path(16) {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
                any = true;
            }
        }
        any.then_some((min_x, min_y, max_x, max_y))
    }

    pub(crate) fn selection_center(&self) -> Option<P> {
        self.selection_bounds()
            .map(|(min_x, min_y, max_x, max_y)| {
                P::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5)
            })
    }

    pub(crate) fn handle_curve_click(
        &mut self,
        i: usize,
        ctrl: bool,
        shift: bool,
        active_group: Option<u64>,
    ) {
        if ctrl {
            if !self.multi_select.remove(&i) {
                self.multi_select.insert(i);
            }
        } else if shift {
            let anchor = self.selected;
            let active: Vec<usize> = self
                .curves
                .iter()
                .enumerate()
                .filter(|(_, c)| active_group.is_none() || c.group_id == active_group)
                .map(|(idx, _)| idx)
                .collect();
            match (
                active.iter().position(|&x| x == anchor),
                active.iter().position(|&x| x == i),
            ) {
                (Some(pa), Some(pi)) => {
                    let (lo, hi) = if pa <= pi { (pa, pi) } else { (pi, pa) };
                    for &gi in &active[lo..=hi] {
                        self.multi_select.insert(gi);
                    }
                }
                _ => {
                    self.multi_select.insert(i);
                }
            }
        }
        self.selected = i;
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
        self.thumb_cache.clear();
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
            self.thumb_cache.clear();
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
            self.load_project(path);
        }
    }

    pub(crate) fn load_project(&mut self, path: PathBuf) {
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

    pub(crate) fn saved_projects(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(PROJECT_DIR)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        out.sort();
        out
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
            let (local, msg) = match copy_into_import(&path) {
                Ok(dest) => {
                    let m = format!("Image loaded → {}", dest.display());
                    (dest, m)
                }
                Err(e) => (
                    path.clone(),
                    format!("Copy to import failed ({e}); using original location"),
                ),
            };
            self.reference_images.push(ReferenceImage::new(local));
            self.selected_image = self.reference_images.len() - 1;
            self.pending_fit_view = true;
            self.last_msg = Some(msg);
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
                self.background = Some([rgba[0], rgba[1], rgba[2]]);
            }
        }
        self.last_picked_color = Some(rgba);
    }

    pub(crate) fn selected_image_ref(&self) -> Option<&ReferenceImage> {
        self.reference_images.get(self.selected_image)
    }

    pub(crate) fn any_image_ready(&self) -> bool {
        self.reference_images.iter().any(|i| i.is_ready())
    }

    pub(crate) fn sample_image_at(&self, wx: f32, wy: f32) -> Option<[u8; 4]> {
        self.reference_images.iter().rev().find_map(|img| {
            if img.visible {
                img.sample_at_world(wx, wy)
            } else {
                None
            }
        })
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::light());

        for img in &mut self.reference_images {
            img.ensure_loaded(ctx, 10.0);
        }

        self.sync_created_at();
        self.tick_playback(ctx);

        if self.pending_fit_view {
            let ready = self.selected_image_ref().map_or(false, |i| i.is_ready());
            if ready {
                let size = ctx.screen_rect().size();
                self.fit_to_curves(size);
                self.pending_fit_view = false;
            }
        }

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| self.top_bar(ui));
        if self.show_left_panel {
            egui::SidePanel::left("left_panel")
                .default_width(400.0)
                .min_width(320.0)
                .show(ctx, |ui| self.left_panel(ui));
        }
        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ui));

        if self.show_gallery {
            self.gallery_window(ctx);
        }

        if self.show_animate {
            self.animate_window(ctx);
        }

        if self.show_trace {
            self.trace_window(ctx);
        }

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
        let toggle_panel = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::B,
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
        if toggle_panel {
            self.show_left_panel = !self.show_left_panel;
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

    #[test]
    fn duplicate_curve_inserts_copy_after_original() {
        let mut app = super::App::new();
        app.curves[0].name = "Eye".to_string();
        let group = app.curves[0].group_id;
        let n = app.curves.len();

        app.duplicate_curve(0);

        assert_eq!(app.curves.len(), n + 1);
        assert_eq!(app.curves[1].name, "Eye-copy");
        assert_eq!(app.curves[1].group_id, group, "copy stays in the same folder");
        assert_eq!(app.selected, 1, "the copy becomes selected");
    }

    #[test]
    fn duplicate_curves_inserts_block_after_last_selected() {
        use crate::model::curve::{CurveSet, PALETTE};
        let mut app = super::App::new();
        app.curves[0].name = "a".to_string();
        let group = app.curves[0].group_id;
        for name in ["b", "c"] {
            let mut extra = CurveSet::empty(name, PALETTE[1]);
            extra.group_id = group;
            app.curves.push(extra);
        }

        app.duplicate_curves(&[2, 0]);

        assert_eq!(app.curves.len(), 5, "two copies added");
        assert_eq!(app.curves[3].name, "a-copy", "sorted by index, a first");
        assert_eq!(app.curves[4].name, "c-copy");
        assert_eq!(app.curves[3].group_id, group, "copies keep the folder");
        assert_eq!(app.selected, 3, "first copy becomes selected");
        let expected: std::collections::BTreeSet<usize> = [3, 4].into_iter().collect();
        assert_eq!(app.multi_select, expected, "copies become the new selection");
    }

    #[test]
    fn duplicate_curves_ignores_empty_and_out_of_range() {
        let mut app = super::App::new();
        let before = app.curves.len();
        app.duplicate_curves(&[]);
        app.duplicate_curves(&[99]);
        assert_eq!(app.curves.len(), before, "no-op on empty or bad indices");
    }

    #[test]
    fn remove_group_deletes_its_curves() {
        use crate::model::curve::{CurveSet, PALETTE};
        let mut app = super::App::new();
        let g2 = app.add_group();
        let mut c = CurveSet::empty("in-g2", PALETTE[1]);
        c.group_id = Some(g2);
        app.curves.push(c);
        let before = app.curves.len();

        app.remove_group(g2);

        assert_eq!(app.curves.len(), before - 1);
        assert!(app.curves.iter().all(|c| c.group_id != Some(g2)));
        assert!(app.groups.iter().all(|g| g.id != g2));
    }

    #[test]
    fn remove_group_holding_every_curve_leaves_a_default_curve() {
        let mut app = super::App::new();
        let g1 = app.groups[0].id;
        let g2 = app.add_group();

        app.remove_group(g1);

        assert_eq!(app.curves.len(), 1, "a fresh default curve is created");
        assert_eq!(app.curves[0].group_id, Some(g2));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn undo_restores_deleted_group_and_its_curves() {
        use crate::model::curve::{CurveSet, PALETTE};
        let mut app = super::App::new();
        let g2 = app.add_group();
        let mut c = CurveSet::empty("in-g2", PALETTE[1]);
        c.group_id = Some(g2);
        app.curves.push(c);
        app.commit_if_changed();

        app.remove_group(g2);
        app.commit_if_changed();
        app.undo();

        assert!(app.groups.iter().any(|g| g.id == g2), "group restored");
        assert!(
            app.curves.iter().any(|c| c.group_id == Some(g2)),
            "curves restored into their group"
        );

        app.redo();
        assert!(app.groups.iter().all(|g| g.id != g2), "redo removes it again");
        assert!(app.curves.iter().all(|c| c.group_id != Some(g2)));
        assert!(
            app.active_group_id.is_none()
                || app
                    .groups
                    .iter()
                    .any(|g| Some(g.id) == app.active_group_id),
            "active group stays valid"
        );
    }

    #[test]
    fn move_curve_to_places_curve_at_target_index() {
        use crate::model::curve::{CurveSet, PALETTE};
        let mut app = super::App::new();
        app.curves[0].name = "a".to_string();
        for name in ["b", "c", "d"] {
            app.curves.push(CurveSet::empty(name, PALETTE[1]));
        }
        app.selected = 0;

        app.move_curve_to(0, 2);
        let names: Vec<&str> = app.curves.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["b", "c", "a", "d"]);
        assert_eq!(app.selected, 2, "selection follows the moved curve");

        app.move_curve_to(3, 0);
        let names: Vec<&str> = app.curves.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["d", "b", "c", "a"]);

        app.move_curve_to(9, 0);
        let names: Vec<&str> = app.curves.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["d", "b", "c", "a"], "out of range is a no-op");
    }

    #[test]
    fn translate_selection_moves_only_selected_curves() {
        use crate::model::curve::{CurveSet, P, PALETTE};
        let mut app = super::App::new();
        app.curves[0].append_segment();
        for name in ["b", "c"] {
            let mut extra = CurveSet::empty(name, PALETTE[1]);
            extra.append_segment();
            app.curves.push(extra);
        }
        app.multi_select = [0, 2].into_iter().collect();

        app.translate_selection(10.0, -5.0);

        assert_eq!(app.curves[0].s1[0], P::new(10.0, -5.0));
        assert_eq!(app.curves[0].s3[0], P::new(12.0, -5.0));
        assert_eq!(app.curves[2].s1[0], P::new(10.0, -5.0));
        assert_eq!(app.curves[1].s1[0], P::new(0.0, 0.0), "unselected stays put");
    }

    #[test]
    fn flip_selection_mirrors_horizontally_around_center() {
        let mut app = super::App::new();
        app.curves[0].append_segment();
        app.multi_select = [0].into_iter().collect();

        app.flip_selection(true);

        assert_eq!(app.curves[0].s1[0].x, 2.0, "x=0 mirrors to 2 around center x=1");
        assert_eq!(app.curves[0].s3[0].x, 0.0, "x=2 mirrors to 0");
        assert_eq!(app.curves[0].s1[0].y, 0.0, "y unchanged on horizontal flip");
    }

    #[test]
    fn rotate_selection_rotates_points_around_center() {
        let mut app = super::App::new();
        app.curves[0].append_segment();
        app.multi_select = [0].into_iter().collect();
        let center = app.selection_center().unwrap();

        app.rotate_selection(center, std::f32::consts::FRAC_PI_2);

        let p = app.curves[0].s1[0];
        assert!((p.x - 1.25).abs() < 1e-4, "x={}", p.x);
        assert!((p.y + 0.75).abs() < 1e-4, "y={}", p.y);
    }

    #[test]
    fn snap_angle_clicks_to_45_degrees_when_enabled() {
        use std::f32::consts::FRAC_PI_4;
        let almost = FRAC_PI_4 * 0.9;
        assert!((super::snap_angle(almost, true) - FRAC_PI_4).abs() < 1e-5);
        assert!((super::snap_angle(almost, false) - almost).abs() < 1e-5);
        let wide = FRAC_PI_4 * 2.4;
        assert!((super::snap_angle(wide, true) - FRAC_PI_4 * 2.0).abs() < 1e-5);
    }
}
