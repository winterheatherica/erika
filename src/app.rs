use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::curve::{Arr, CurveKind, CurveSet, LineStyle, P, PALETTE};
use crate::export::{ExportConfig, export_png};
use crate::image_ref::ReferenceImage;
use crate::persist::{CameraState, Project};
use crate::svg_export::{SvgConfig, export_svg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandleId {
    Bezier(usize, Arr, usize),
    EllipseCenter(usize),
    EllipseRx(usize),
    EllipseRy(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorPickTarget {
    Stroke(usize),
    Fill(usize),
    Background,
}

pub struct App {
    pub curves: Vec<CurveSet>,
    pub selected: usize,

    pub center_x: f32,
    pub center_y: f32,
    pub scale: f32,

    dragging_handle: Option<HandleId>,
    dragging_image: bool,
    drag_image_offset: Vec2,
    panning: bool,
    link_continuity: bool,

    samples_per_segment: usize,
    show_grid: bool,
    show_axes: bool,
    show_handles_all: bool,
    background: [u8; 3],

    reference_image: Option<ReferenceImage>,
    image_drag_enabled: bool,
    pending_fit_view: bool,

    color_pick_target: Option<ColorPickTarget>,
    last_picked_color: Option<[u8; 4]>,

    export_path: String,
    export_w: u32,
    export_h: u32,
    export_transparent: bool,
    export_samples: usize,
    svg_export_path: String,
    last_msg: Option<String>,

    new_curve_name: String,
    new_curve_kind: CurveKind,
    current_project_path: Option<PathBuf>,

    undo_stack: Vec<Vec<CurveSet>>,
    redo_stack: Vec<Vec<CurveSet>>,
    last_committed_curves: Vec<CurveSet>,
}

impl App {
    pub fn new() -> Self {
        let initial = vec![CurveSet::empty("Curve 1", PALETTE[0])];
        Self {
            curves: initial.clone(),
            selected: 0,
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
            export_path: "output.png".into(),
            export_w: 1024,
            export_h: 1024,
            export_transparent: false,
            export_samples: 64,
            svg_export_path: "output.svg".into(),
            last_msg: None,
            new_curve_name: String::new(),
            new_curve_kind: CurveKind::Bezier,
            current_project_path: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_committed_curves: initial,
        }
    }

    fn commit_if_changed(&mut self) {
        if self.curves != self.last_committed_curves {
            let prev = std::mem::replace(&mut self.last_committed_curves, self.curves.clone());
            self.undo_stack.push(prev);
            self.redo_stack.clear();
            if self.undo_stack.len() > 200 {
                self.undo_stack.remove(0);
            }
        }
    }

    fn undo(&mut self) {
        self.commit_if_changed();
        if let Some(prev) = self.undo_stack.pop() {
            let now = std::mem::replace(&mut self.curves, prev.clone());
            self.redo_stack.push(now);
            self.last_committed_curves = prev;
            self.cancel_interactions();
            self.clamp_selection();
        }
    }

    fn redo(&mut self) {
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

    fn w2s(&self, rect: Rect, p: P) -> Pos2 {
        Pos2::new(
            rect.center().x + (p.x - self.center_x) * self.scale,
            rect.center().y - (p.y - self.center_y) * self.scale,
        )
    }

    fn s2w(&self, rect: Rect, s: Pos2) -> P {
        P::new(
            self.center_x + (s.x - rect.center().x) / self.scale,
            self.center_y - (s.y - rect.center().y) / self.scale,
        )
    }

    fn ellipse_rx_endpoint(c: &CurveSet) -> P {
        let r = c.ellipse_rot_deg.to_radians();
        P::new(
            c.ellipse_cx + c.ellipse_rx * r.cos(),
            c.ellipse_cy + c.ellipse_rx * r.sin(),
        )
    }

    fn ellipse_ry_endpoint(c: &CurveSet) -> P {
        let r = (c.ellipse_rot_deg + 90.0).to_radians();
        P::new(
            c.ellipse_cx + c.ellipse_ry * r.cos(),
            c.ellipse_cy + c.ellipse_ry * r.sin(),
        )
    }

    fn find_handle(&self, rect: Rect, pos: Pos2) -> Option<HandleId> {
        let ci = self.selected;
        let c = self.curves.get(ci)?;
        if !c.visible || !c.show_handles {
            return None;
        }
        let threshold_sq = 14.0 * 14.0;
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

    fn update_dragged_handle(&mut self, world: P) {
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

    fn fit_to_curves(&mut self, viewport_px: Vec2) {
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
        }
    }

    fn apply_project(&mut self, p: Project) {
        self.curves = p.curves;
        if self.curves.is_empty() {
            self.curves.push(CurveSet::empty("Curve 1", PALETTE[0]));
        }
        for c in &mut self.curves {
            c.clamp_active_segment();
        }
        self.selected = 0;
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
    }

    fn save_dialog(&mut self) {
        let initial = self
            .current_project_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("project.json"));
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

    fn load_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("project", &["json"]);
        if let Some(p) = &self.current_project_path {
            if let Some(dir) = p.parent() {
                if dir.as_os_str().len() > 0 {
                    dlg = dlg.set_directory(dir);
                }
            }
        }
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

    fn load_image_dialog(&mut self) {
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

    fn apply_picked_color(&mut self, target: ColorPickTarget, rgba: [u8; 4]) {
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

impl App {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Erika").strong().size(16.0));
            ui.separator();
            if ui.button("💾 Save").clicked() {
                self.save_dialog();
            }
            if ui.button("📂 Load").clicked() {
                self.load_dialog();
            }
            ui.separator();
            let undo_enabled =
                !self.undo_stack.is_empty() || self.curves != self.last_committed_curves;
            let redo_enabled = !self.redo_stack.is_empty();
            if ui
                .add_enabled(undo_enabled, egui::Button::new("↶ Undo"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(redo_enabled, egui::Button::new("↷ Redo"))
                .on_hover_text("Ctrl+Y / Ctrl+Shift+Z")
                .clicked()
            {
                self.redo();
            }
            ui.separator();
            ui.checkbox(&mut self.show_grid, "Grid");
            ui.checkbox(&mut self.show_axes, "Axes");
            ui.checkbox(&mut self.show_handles_all, "Handles");
            ui.checkbox(&mut self.link_continuity, "Link S3[i]=S1[i+1]");
            ui.separator();
            ui.label("Samples/seg:");
            ui.add(egui::DragValue::new(&mut self.samples_per_segment).range(4..=512));
            ui.separator();
            if ui.button("Fit view").clicked() {
                let size = ui.ctx().screen_rect().size();
                self.fit_to_curves(size);
            }
            ui.separator();
            ui.label("BG:");
            let _ = ui.color_edit_button_srgb(&mut self.background);
            let img_ready = self
                .reference_image
                .as_ref()
                .map_or(false, |i| i.is_ready());
            pick_color_button_widget(
                ui,
                ColorPickTarget::Background,
                &mut self.color_pick_target,
                img_ready,
            );
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("PNG:");
            ui.add(
                egui::TextEdit::singleline(&mut self.export_path)
                    .desired_width(160.0)
                    .hint_text("output.png"),
            );
            ui.label("W");
            ui.add(egui::DragValue::new(&mut self.export_w).range(64..=8192));
            ui.label("H");
            ui.add(egui::DragValue::new(&mut self.export_h).range(64..=8192));
            ui.checkbox(&mut self.export_transparent, "Transparent");
            ui.label("samples:");
            ui.add(egui::DragValue::new(&mut self.export_samples).range(8..=512));
            if ui.button("Export PNG").clicked() {
                let path = PathBuf::from(&self.export_path);
                let bg = if self.export_transparent {
                    None
                } else {
                    Some(self.background)
                };
                let cfg = ExportConfig {
                    width: self.export_w,
                    height: self.export_h,
                    samples_per_segment: self.export_samples,
                    background: bg,
                    padding_fraction: 0.05,
                    path: &path,
                };
                self.last_msg = Some(match export_png(&self.curves, &cfg) {
                    Ok(()) => format!("Exported PNG → {}", path.display()),
                    Err(e) => format!("Error: {e}"),
                });
            }
            ui.separator();
            ui.label("SVG:");
            ui.add(
                egui::TextEdit::singleline(&mut self.svg_export_path)
                    .desired_width(160.0)
                    .hint_text("output.svg"),
            );
            if ui.button("Export SVG").clicked() {
                let path = PathBuf::from(&self.svg_export_path);
                let bg = if self.export_transparent {
                    None
                } else {
                    Some(self.background)
                };
                let cfg = SvgConfig {
                    width: self.export_w,
                    height: self.export_h,
                    background: bg,
                    padding_fraction: 0.05,
                    path: &path,
                };
                self.last_msg = Some(match export_svg(&self.curves, &cfg) {
                    Ok(()) => format!("Exported SVG → {}", path.display()),
                    Err(e) => format!("Error: {e}"),
                });
            }
            if let Some(msg) = &self.last_msg {
                ui.label(egui::RichText::new(msg).color(Color32::from_rgb(60, 100, 60)));
            }
        });
    }


    fn left_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("left_scroll").show(ui, |ui| {
            self.curves_section(ui);
            ui.separator();
            self.image_section(ui);
            ui.separator();
            self.shape_editor_section(ui);
        });
    }

    fn curves_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Curves");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_curve_name)
                    .hint_text("Name (optional)")
                    .desired_width(140.0),
            );
            egui::ComboBox::from_id_salt("new_curve_kind")
                .selected_text(self.new_curve_kind.label())
                .show_ui(ui, |ui| {
                    for k in CurveKind::all() {
                        ui.selectable_value(&mut self.new_curve_kind, k, k.label());
                    }
                });
            if ui.button("+ Add").clicked() {
                let name = if self.new_curve_name.trim().is_empty() {
                    format!("Curve {}", self.curves.len() + 1)
                } else {
                    self.new_curve_name.trim().to_string()
                };
                let color = PALETTE[self.curves.len() % PALETTE.len()];
                let new_curve = match self.new_curve_kind {
                    CurveKind::Bezier => CurveSet::empty(name, color),
                    CurveKind::Ellipse => {
                        CurveSet::new_ellipse(name, color, self.center_x, self.center_y)
                    }
                };
                self.curves.push(new_curve);
                self.selected = self.curves.len() - 1;
                self.new_curve_name.clear();
            }
        });

        let mut to_delete: Option<usize> = None;
        let mut select: Option<usize> = None;
        for (i, c) in self.curves.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let _ = ui.color_edit_button_srgb(&mut c.color);
                ui.checkbox(&mut c.visible, "");
                let kind_tag = match c.kind {
                    CurveKind::Bezier => "B",
                    CurveKind::Ellipse => "E",
                };
                let label = format!("[{}] {}", kind_tag, c.name);
                if ui.selectable_label(self.selected == i, label).clicked() {
                    select = Some(i);
                }
                if ui.small_button("🗑").clicked() {
                    to_delete = Some(i);
                }
            });
        }
        if let Some(i) = select {
            self.selected = i;
        }
        if let Some(i) = to_delete {
            if self.curves.len() > 1 {
                self.curves.remove(i);
                if self.selected >= self.curves.len() {
                    self.selected = self.curves.len() - 1;
                }
            } else {
                self.curves[i] =
                    CurveSet::empty(format!("Curve {}", i + 1), PALETTE[i % PALETTE.len()]);
            }
        }

        ui.separator();
        if self.curves.is_empty() {
            return;
        }
        let sel = self.selected.min(self.curves.len() - 1);
        self.selected = sel;
        let c = &mut self.curves[sel];

        ui.label(egui::RichText::new(format!("Edit: {}", c.name)).strong());
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut c.name);
        });
        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("kind_{}", sel))
                .selected_text(c.kind.label())
                .show_ui(ui, |ui| {
                    for k in CurveKind::all() {
                        ui.selectable_value(&mut c.kind, k, k.label());
                    }
                });
        });

        ui.label("Stroke");
        let img_ready = self
            .reference_image
            .as_ref()
            .map_or(false, |i| i.is_ready());
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.stroke_visible, "Show");
            let _ = ui.color_edit_button_srgb(&mut c.color);
        });
        ui.horizontal(|ui| {
            pick_color_button_widget(
                ui,
                ColorPickTarget::Stroke(sel),
                &mut self.color_pick_target,
                img_ready,
            );
            ui.label("← pick stroke from image");
        });
        let c = &mut self.curves[sel];
        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut c.thickness, 0.5..=10.0)
                    .text("thickness")
                    .show_value(true),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Style:");
            egui::ComboBox::from_id_salt(format!("style_{}", sel))
                .selected_text(c.line_style.label())
                .show_ui(ui, |ui| {
                    for style in LineStyle::all() {
                        ui.selectable_value(&mut c.line_style, style, style.label());
                    }
                });
        });

        ui.label("Fill");
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.fill_enabled, "Enabled");
            let _ = ui.color_edit_button_srgba_unmultiplied(&mut c.fill_color);
        });
        ui.horizontal(|ui| {
            pick_color_button_widget(
                ui,
                ColorPickTarget::Fill(sel),
                &mut self.color_pick_target,
                img_ready,
            );
            ui.label("← pick fill from image");
        });
        let c = &mut self.curves[sel];

        ui.label("Handles & control polygon");
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.show_handles, "Handles");
            if c.is_bezier() {
                ui.checkbox(&mut c.show_control_poly, "Control polygon");
            }
        });
        if c.is_bezier() && c.show_control_poly {
            ui.horizontal(|ui| {
                ui.label("CP thickness:");
                ui.add(egui::Slider::new(&mut c.control_poly_thickness, 0.5..=6.0));
            });
        }
    }

    fn image_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Reference image");
        ui.horizontal(|ui| {
            if ui.button("📷 Load image").clicked() {
                self.load_image_dialog();
            }
            if self.reference_image.is_some() && ui.button("Remove").clicked() {
                self.reference_image = None;
            }
        });

        if let Some(picked) = self.last_picked_color {
            ui.horizontal(|ui| {
                ui.label("Last picked:");
                let preview =
                    Color32::from_rgba_unmultiplied(picked[0], picked[1], picked[2], picked[3]);
                let (rect_r, _) =
                    ui.allocate_exact_size(egui::vec2(20.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect_r, 2.0, preview);
                ui.label(format!(
                    "#{:02x}{:02x}{:02x}{:02x}",
                    picked[0], picked[1], picked[2], picked[3]
                ));
            });
        }

        let Some(img) = &mut self.reference_image else {
            ui.label(egui::RichText::new("No image loaded").italics().weak());
            return;
        };

        ui.label(
            egui::RichText::new(format!("📁 {}", img.path.display()))
                .small()
                .weak(),
        );
        if let Some(err) = &img.load_error {
            ui.label(egui::RichText::new(format!("⚠ {}", err)).color(Color32::RED));
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut img.visible, "Visible");
            ui.checkbox(&mut img.locked, "Lock");
            ui.checkbox(&mut self.image_drag_enabled, "Drag mode");
        });
        ui.horizontal(|ui| {
            ui.label("Opacity:");
            ui.add(egui::Slider::new(&mut img.opacity, 0.0..=1.0));
        });
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut img.world_x).speed(0.05));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut img.world_y).speed(0.05));
        });
        ui.horizontal(|ui| {
            ui.label("W:");
            ui.add(
                egui::DragValue::new(&mut img.world_w)
                    .speed(0.05)
                    .range(0.01..=10000.0),
            );
            ui.label("H:");
            ui.add(
                egui::DragValue::new(&mut img.world_h)
                    .speed(0.05)
                    .range(0.01..=10000.0),
            );
            if ui.button("Fit aspect").clicked() {
                img.fix_aspect();
            }
        });
    }

    fn shape_editor_section(&mut self, ui: &mut egui::Ui) {
        if self.curves.is_empty() {
            return;
        }
        let sel = self.selected.min(self.curves.len() - 1);
        match self.curves[sel].kind {
            CurveKind::Bezier => self.bezier_points_section(ui, sel),
            CurveKind::Ellipse => self.ellipse_params_section(ui, sel),
        }
    }

    fn ellipse_params_section(&mut self, ui: &mut egui::Ui, sel: usize) {
        ui.heading("Ellipse parameters");
        let c = &mut self.curves[sel];
        ui.horizontal(|ui| {
            ui.label("Center X:");
            ui.add(egui::DragValue::new(&mut c.ellipse_cx).speed(0.02).fixed_decimals(3));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut c.ellipse_cy).speed(0.02).fixed_decimals(3));
        });
        ui.horizontal(|ui| {
            ui.label("Radius X:");
            ui.add(
                egui::DragValue::new(&mut c.ellipse_rx)
                    .speed(0.02)
                    .range(0.001..=10000.0)
                    .fixed_decimals(3),
            );
            ui.label("Y:");
            ui.add(
                egui::DragValue::new(&mut c.ellipse_ry)
                    .speed(0.02)
                    .range(0.001..=10000.0)
                    .fixed_decimals(3),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Rotation:");
            ui.add(
                egui::DragValue::new(&mut c.ellipse_rot_deg)
                    .speed(0.5)
                    .suffix("°"),
            );
            if ui.small_button("Make circle").clicked() {
                let r = (c.ellipse_rx + c.ellipse_ry) * 0.5;
                c.ellipse_rx = r;
                c.ellipse_ry = r;
            }
        });
        ui.label(
            egui::RichText::new("Drag center on canvas to move, rx/ry handles to resize+rotate.")
                .small()
                .weak(),
        );
    }

    fn bezier_points_section(&mut self, ui: &mut egui::Ui, sel: usize) {
        ui.heading("Points");
        let c = &self.curves[sel];
        ui.label(format!(
            "n = {} (|S1|={}, |S2|={}, |S3|={})",
            c.n(),
            c.s1.len(),
            c.s2.len(),
            c.s3.len(),
        ));

        let n = c.n();
        if n > 0 {
            let active = c.active_segment.min(n - 1);
            ui.horizontal(|ui| {
                ui.label("Active segment:");
                if ui
                    .add_enabled(active > 0, egui::Button::new("◀"))
                    .clicked()
                {
                    self.curves[sel].active_segment = active.saturating_sub(1);
                }
                ui.label(
                    egui::RichText::new(format!("{} / {}", active + 1, n))
                        .monospace()
                        .strong(),
                );
                if ui
                    .add_enabled(active + 1 < n, egui::Button::new("▶"))
                    .clicked()
                {
                    self.curves[sel].active_segment = active + 1;
                }
                ui.separator();
                if ui.small_button("First").clicked() {
                    self.curves[sel].active_segment = 0;
                }
                if ui.small_button("Last").clicked() {
                    self.curves[sel].active_segment = n - 1;
                }
            });
            ui.label(
                egui::RichText::new("Only handles of the active segment can be dragged on canvas.")
                    .small()
                    .weak(),
            );
        }

        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("+ Add segment").strong().size(14.0))
                        .min_size(egui::vec2(140.0, 0.0)),
                )
                .on_hover_text("Append 1 segment (3 points across S1/S2/S3).\nShortcut: Enter")
                .clicked()
            {
                self.curves[sel].append_segment();
            }
            if ui.button("− Remove last").clicked() {
                self.curves[sel].pop_segment();
            }
        });

        for a in Arr::all() {
            let header = format!("{} ({} pts)", a.label(), self.curves[sel].get(a).len());
            egui::CollapsingHeader::new(header)
                .id_salt(format!("hdr_{}", a.label()))
                .default_open(true)
                .show(ui, |ui| {
                    self.points_array_editor(ui, sel, a);
                });
        }
    }

    fn points_array_editor(&mut self, ui: &mut egui::Ui, ci: usize, a: Arr) {
        let mut remove_idx: Option<usize> = None;
        let mut insert_after: Option<usize> = None;
        let mut do_auto_merge = false;
        let len = self.curves[ci].get(a).len();
        let s1_first = self.curves[ci].s1.first().copied();

        for i in 0..len {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{:>2}", i + 1)).monospace());
                let arr = self.curves[ci].get_mut(a);
                ui.add(
                    egui::DragValue::new(&mut arr[i].x)
                        .speed(0.02)
                        .fixed_decimals(3)
                        .prefix("x="),
                );
                ui.add(
                    egui::DragValue::new(&mut arr[i].y)
                        .speed(0.02)
                        .fixed_decimals(3)
                        .prefix("y="),
                );
                if ui.small_button("+").on_hover_text("Insert after").clicked() {
                    insert_after = Some(i);
                }
                if ui.small_button("🗑").clicked() {
                    remove_idx = Some(i);
                }
                if a == Arr::S3 && i + 1 == len && s1_first.is_some() {
                    if ui
                        .small_button("⤴ auto merge")
                        .on_hover_text("Set this S3 = S1[0] (close the loop)")
                        .clicked()
                    {
                        do_auto_merge = true;
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.button("+ append").clicked() {
                let arr = self.curves[ci].get_mut(a);
                let p = arr.last().copied().unwrap_or(P::new(0.0, 0.0));
                arr.push(p);
            }
            if ui.button("clear").clicked() {
                self.curves[ci].get_mut(a).clear();
            }
        });
        if let Some(i) = insert_after {
            let arr = self.curves[ci].get_mut(a);
            let p = arr.get(i).copied().unwrap_or(P::new(0.0, 0.0));
            arr.insert(i + 1, p);
        }
        if let Some(i) = remove_idx {
            self.curves[ci].get_mut(a).remove(i);
        }
        if do_auto_merge {
            if let Some(first) = s1_first {
                let s3 = &mut self.curves[ci].s3;
                if let Some(last) = s3.last_mut() {
                    *last = first;
                }
            }
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        painter.rect_filled(
            rect,
            0.0,
            Color32::from_rgb(self.background[0], self.background[1], self.background[2]),
        );

        self.draw_reference_image(&painter, rect);

        if self.show_grid {
            self.draw_grid(&painter, rect);
        }
        if self.show_axes {
            self.draw_axes(&painter, rect);
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 {
                let mouse = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
                let world_before = self.s2w(rect, mouse);
                let factor = (scroll * 0.005).exp();
                self.scale = (self.scale * factor).clamp(2.0, 5000.0);
                let world_after = self.s2w(rect, mouse);
                self.center_x += world_before.x - world_after.x;
                self.center_y += world_before.y - world_after.y;
            }
        }

        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let d = response.drag_delta();
            self.center_x -= d.x / self.scale;
            self.center_y += d.y / self.scale;
        }

        let picking = self.color_pick_target.is_some();
        let mut consumed_by_pick = false;
        if picking {
            if response.clicked() || response.drag_started_by(egui::PointerButton::Primary) {
                if let Some(pos) = response.interact_pointer_pos() {
                    let w = self.s2w(rect, pos);
                    if let Some(img) = &self.reference_image {
                        if let Some(rgba) = img.sample_at_world(w.x, w.y) {
                            if let Some(target) = self.color_pick_target.take() {
                                self.apply_picked_color(target, rgba);
                            }
                        } else {
                            self.color_pick_target = None;
                        }
                    } else {
                        self.color_pick_target = None;
                    }
                }
                consumed_by_pick = true;
            }
        }

        if !consumed_by_pick && response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let mut started = false;
                if self.image_drag_enabled {
                    if let Some(img) = &self.reference_image {
                        if img.visible && !img.locked && self.image_hit(rect, pos) {
                            let img_screen_pos =
                                self.w2s(rect, P::new(img.world_x, img.world_y));
                            self.drag_image_offset = pos - img_screen_pos;
                            self.dragging_image = true;
                            started = true;
                        }
                    }
                }
                if !started {
                    if let Some(h) = self.find_handle(rect, pos) {
                        self.dragging_handle = Some(h);
                    } else {
                        self.panning = true;
                    }
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            if self.dragging_image {
                if let Some(pos) = response.interact_pointer_pos() {
                    let new_screen = pos - self.drag_image_offset;
                    let top_left_world = self.s2w(rect, new_screen);
                    if let Some(img) = self.reference_image.as_mut() {
                        img.world_x = top_left_world.x;
                        img.world_y = top_left_world.y - img.world_h;
                    }
                }
            } else if self.dragging_handle.is_some() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let w = self.s2w(rect, pos);
                    self.update_dragged_handle(w);
                }
            } else if self.panning {
                let d = response.drag_delta();
                self.center_x -= d.x / self.scale;
                self.center_y += d.y / self.scale;
            }
        }
        if response.drag_stopped() {
            self.dragging_handle = None;
            self.dragging_image = false;
            self.panning = false;
        }

        for c in &self.curves {
            if !c.visible {
                continue;
            }
            self.draw_curve(&painter, rect, c);
        }

        if self.show_handles_all {
            for (ci, c) in self.curves.iter().enumerate() {
                if !c.visible || !c.show_handles {
                    continue;
                }
                let is_sel = ci == self.selected;
                self.draw_handles(&painter, rect, ci, c, is_sel);
            }
        }

        if let Some(pos) = response.hover_pos() {
            let w = self.s2w(rect, pos);
            let mut text = format!("({:.3}, {:.3})  scale={:.1} px/u", w.x, w.y, self.scale);
            if picking {
                if let Some(img) = &self.reference_image {
                    if let Some(rgba) = img.sample_at_world(w.x, w.y) {
                        text.push_str(&format!(
                            "   pick: #{:02x}{:02x}{:02x}",
                            rgba[0], rgba[1], rgba[2]
                        ));
                        let swatch = Color32::from_rgb(rgba[0], rgba[1], rgba[2]);
                        let r = Rect::from_min_size(
                            rect.left_top() + Vec2::new(8.0, 24.0),
                            egui::vec2(18.0, 14.0),
                        );
                        painter.rect_filled(r, 2.0, swatch);
                        painter.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::BLACK));
                    }
                }
            }
            painter.text(
                rect.left_top() + Vec2::new(8.0, 6.0),
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::monospace(12.0),
                Color32::from_gray(60),
            );
        }

        let hint = if picking {
            "color-pick mode • click on the reference image to sample • Esc to cancel"
        } else if self.image_drag_enabled {
            "image drag ON • drag handle = move • right/middle = pan • scroll = zoom • Enter = add segment • Ctrl+Z undo"
        } else {
            "drag handle = move • right/middle = pan • scroll = zoom • Enter = add segment • Ctrl+Z undo"
        };
        painter.text(
            rect.right_bottom() + Vec2::new(-8.0, -6.0),
            egui::Align2::RIGHT_BOTTOM,
            hint,
            egui::FontId::proportional(11.0),
            Color32::from_gray(140),
        );
    }

    fn draw_curve(&self, painter: &egui::Painter, rect: Rect, c: &CurveSet) {
        let stroke_samples = self.samples_per_segment.max(4);
        let fill_samples = stroke_samples.max(64);

        let stroke_world = c.sampled_path(stroke_samples);
        let stroke_pts: Vec<Pos2> = stroke_world.iter().map(|p| self.w2s(rect, *p)).collect();

        if c.fill_enabled {
            let fill_world = if fill_samples == stroke_samples {
                stroke_world.clone()
            } else {
                c.sampled_path(fill_samples)
            };
            let fill_pts: Vec<Pos2> = fill_world.iter().map(|p| self.w2s(rect, *p)).collect();
            if fill_pts.len() >= 3 {
                let fill_color = Color32::from_rgba_unmultiplied(
                    c.fill_color[0],
                    c.fill_color[1],
                    c.fill_color[2],
                    c.fill_color[3],
                );
                draw_filled_polygon(painter, &fill_pts, fill_color);
            }
        }

        if c.stroke_visible && stroke_pts.len() >= 2 {
            let color = Color32::from_rgb(c.color[0], c.color[1], c.color[2]);
            let stroke = Stroke::new(c.thickness, color);
            let pattern_base = c.line_style.pattern();
            if pattern_base.is_empty() {
                painter.add(egui::Shape::line(stroke_pts.clone(), stroke));
            } else {
                let pattern: Vec<f32> = pattern_base
                    .iter()
                    .map(|v| v * c.thickness.max(0.5))
                    .collect();
                for dash in arc_length_dashes(&stroke_pts, &pattern) {
                    if dash.len() >= 2 {
                        painter.add(egui::Shape::line(dash, stroke));
                    }
                }
            }
        }

        if c.is_bezier() && c.show_control_poly {
            let cp_color =
                Color32::from_rgba_unmultiplied(c.color[0], c.color[1], c.color[2], 140);
            let cp_stroke = Stroke::new(c.control_poly_thickness, cp_color);
            let n = c.n();
            for i in 0..n {
                let cp = vec![
                    self.w2s(rect, c.s1[i]),
                    self.w2s(rect, c.s2[i]),
                    self.w2s(rect, c.s3[i]),
                ];
                painter.add(egui::Shape::line(cp, cp_stroke));
            }
        }
    }

    fn draw_handles(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        ci: usize,
        c: &CurveSet,
        is_sel: bool,
    ) {
        let base = c.color;
        match c.kind {
            CurveKind::Bezier => {
                let n = c.n();
                let active = if n > 0 { c.active_segment.min(n - 1) } else { 0 };
                for a in Arr::all() {
                    for (pi, p) in c.get(a).iter().enumerate() {
                        let sp = self.w2s(rect, *p);
                        let is_drag = self.dragging_handle == Some(HandleId::Bezier(ci, a, pi));
                        let is_active = is_sel && pi == active;
                        let base_r = match a {
                            Arr::S1 | Arr::S3 => 5.0,
                            Arr::S2 => 4.0,
                        };
                        let r = if is_active { base_r } else { base_r * 0.65 };
                        let alpha: u8 = if is_active { 255 } else { 70 };
                        let color =
                            Color32::from_rgba_unmultiplied(base[0], base[1], base[2], alpha);
                        let fill = if matches!(a, Arr::S2) {
                            Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
                        } else {
                            color
                        };
                        let stroke = Stroke::new(if is_drag { 2.5 } else { 1.5 }, color);
                        painter.circle(sp, r, fill, stroke);
                    }
                }
            }
            CurveKind::Ellipse => {
                let alpha: u8 = if is_sel { 255 } else { 90 };
                let color = Color32::from_rgba_unmultiplied(base[0], base[1], base[2], alpha);
                let white = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
                let center = P::new(c.ellipse_cx, c.ellipse_cy);
                let rx_end = Self::ellipse_rx_endpoint(c);
                let ry_end = Self::ellipse_ry_endpoint(c);

                let sp_center = self.w2s(rect, center);
                let sp_rx = self.w2s(rect, rx_end);
                let sp_ry = self.w2s(rect, ry_end);

                painter.line_segment(
                    [sp_center, sp_rx],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(base[0], base[1], base[2], 70)),
                );
                painter.line_segment(
                    [sp_center, sp_ry],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(base[0], base[1], base[2], 70)),
                );

                let is_center_drag =
                    self.dragging_handle == Some(HandleId::EllipseCenter(ci));
                let is_rx_drag = self.dragging_handle == Some(HandleId::EllipseRx(ci));
                let is_ry_drag = self.dragging_handle == Some(HandleId::EllipseRy(ci));
                painter.circle(
                    sp_center,
                    if is_sel { 6.0 } else { 4.0 },
                    color,
                    Stroke::new(if is_center_drag { 2.5 } else { 1.5 }, color),
                );
                painter.circle(
                    sp_rx,
                    if is_sel { 5.0 } else { 3.5 },
                    white,
                    Stroke::new(if is_rx_drag { 2.5 } else { 1.5 }, color),
                );
                painter.circle(
                    sp_ry,
                    if is_sel { 5.0 } else { 3.5 },
                    white,
                    Stroke::new(if is_ry_drag { 2.5 } else { 1.5 }, color),
                );
            }
        }
    }

    fn image_hit(&self, rect: Rect, screen_pos: Pos2) -> bool {
        let Some(img) = &self.reference_image else {
            return false;
        };
        if img.world_w <= 0.0 || img.world_h <= 0.0 {
            return false;
        }
        let tl = self.w2s(rect, P::new(img.world_x, img.world_y + img.world_h));
        let br = self.w2s(rect, P::new(img.world_x + img.world_w, img.world_y));
        let r = Rect::from_min_max(tl, br);
        r.contains(screen_pos)
    }

    fn draw_reference_image(&self, painter: &egui::Painter, rect: Rect) {
        let Some(img) = &self.reference_image else {
            return;
        };
        if !img.visible {
            return;
        }
        let Some(tex) = &img.texture else {
            return;
        };
        if img.world_w <= 0.0 || img.world_h <= 0.0 {
            return;
        }
        let tl = self.w2s(rect, P::new(img.world_x, img.world_y + img.world_h));
        let br = self.w2s(rect, P::new(img.world_x + img.world_w, img.world_y));
        let r = Rect::from_min_max(tl, br);
        let alpha = (img.opacity.clamp(0.0, 1.0) * 255.0) as u8;
        let tint = Color32::from_white_alpha(alpha);
        painter.image(
            tex.id(),
            r,
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
            tint,
        );
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let target_px = 60.0;
        let raw = target_px / self.scale;
        if !raw.is_finite() || raw <= 0.0 {
            return;
        }
        let mag = 10f32.powf(raw.log10().floor());
        let step = if raw / mag < 2.0 {
            mag
        } else if raw / mag < 5.0 {
            mag * 2.0
        } else {
            mag * 5.0
        };

        let min = self.s2w(rect, rect.left_bottom());
        let max = self.s2w(rect, rect.right_top());
        let color = Color32::from_gray(225);

        let x0 = (min.x / step).floor() * step;
        let mut x = x0;
        while x <= max.x {
            let sx = self.w2s(rect, P::new(x, 0.0)).x;
            painter.line_segment(
                [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
                Stroke::new(1.0, color),
            );
            x += step;
        }
        let y0 = (min.y / step).floor() * step;
        let mut y = y0;
        while y <= max.y {
            let sy = self.w2s(rect, P::new(0.0, y)).y;
            painter.line_segment(
                [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
                Stroke::new(1.0, color),
            );
            y += step;
        }

        let label_color = Color32::from_gray(150);
        let fmt = |v: f32| -> String {
            if step >= 1.0 {
                format!("{:.0}", v)
            } else if step >= 0.1 {
                format!("{:.1}", v)
            } else {
                format!("{:.2}", v)
            }
        };
        let mut x = x0;
        while x <= max.x {
            let sx = self.w2s(rect, P::new(x, 0.0)).x;
            painter.text(
                Pos2::new(sx + 2.0, rect.bottom() - 2.0),
                egui::Align2::LEFT_BOTTOM,
                fmt(x),
                egui::FontId::monospace(10.0),
                label_color,
            );
            x += step;
        }
        let mut y = y0;
        while y <= max.y {
            let sy = self.w2s(rect, P::new(0.0, y)).y;
            painter.text(
                Pos2::new(rect.left() + 2.0, sy - 1.0),
                egui::Align2::LEFT_BOTTOM,
                fmt(y),
                egui::FontId::monospace(10.0),
                label_color,
            );
            y += step;
        }
    }

    fn draw_axes(&self, painter: &egui::Painter, rect: Rect) {
        let axis_color = Color32::from_gray(150);
        let origin = self.w2s(rect, P::new(0.0, 0.0));
        let y_clamped = origin.y.clamp(rect.top(), rect.bottom());
        painter.line_segment(
            [
                Pos2::new(rect.left(), y_clamped),
                Pos2::new(rect.right(), y_clamped),
            ],
            Stroke::new(1.5, axis_color),
        );
        let x_clamped = origin.x.clamp(rect.left(), rect.right());
        painter.line_segment(
            [
                Pos2::new(x_clamped, rect.top()),
                Pos2::new(x_clamped, rect.bottom()),
            ],
            Stroke::new(1.5, axis_color),
        );
    }
}

fn pick_color_button_widget(
    ui: &mut egui::Ui,
    target: ColorPickTarget,
    current_target: &mut Option<ColorPickTarget>,
    img_ready: bool,
) {
    let active = *current_target == Some(target);
    let label = if active { "🎨..." } else { "🎨" };
    let btn = ui.add_enabled(
        img_ready,
        egui::Button::new(label).small().fill(if active {
            Color32::from_rgb(255, 220, 100)
        } else {
            Color32::TRANSPARENT
        }),
    );
    let btn = btn.on_hover_text(if img_ready {
        "Pick this color from the reference image (click image after)"
    } else {
        "Load a reference image first to enable color picking"
    });
    if btn.clicked() {
        *current_target = if active { None } else { Some(target) };
    }
}

fn draw_filled_polygon(painter: &egui::Painter, pts: &[Pos2], color: Color32) {
    if pts.len() < 3 {
        return;
    }
    let flat: Vec<f64> = pts
        .iter()
        .flat_map(|p| [p.x as f64, p.y as f64])
        .collect();
    let Ok(indices) = earcutr::earcut(&flat, &[], 2) else {
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: pts.to_vec(),
            closed: true,
            fill: color,
            stroke: egui::Stroke::NONE.into(),
        }));
        return;
    };
    let mut mesh = egui::epaint::Mesh::default();
    for p in pts {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    for i in &indices {
        mesh.indices.push(*i as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn arc_length_dashes(pts: &[Pos2], pattern: &[f32]) -> Vec<Vec<Pos2>> {
    let mut result = Vec::new();
    if pts.len() < 2 || pattern.is_empty() {
        return result;
    }
    let mut current = vec![pts[0]];
    let mut pattern_idx = 0;
    let mut remaining = pattern[0];
    let mut drawing = true;

    for i in 1..pts.len() {
        let mut p0 = pts[i - 1];
        let p1 = pts[i];
        loop {
            let dx = p1.x - p0.x;
            let dy = p1.y - p0.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-6 {
                break;
            }
            if dist <= remaining {
                if drawing {
                    current.push(p1);
                }
                remaining -= dist;
                break;
            } else {
                let frac = remaining / dist;
                let mid = Pos2::new(p0.x + dx * frac, p0.y + dy * frac);
                if drawing {
                    current.push(mid);
                    if current.len() >= 2 {
                        result.push(std::mem::take(&mut current));
                    }
                } else {
                    current = vec![mid];
                }
                drawing = !drawing;
                pattern_idx = (pattern_idx + 1) % pattern.len();
                remaining = pattern[pattern_idx];
                p0 = mid;
            }
        }
    }
    if drawing && current.len() >= 2 {
        result.push(current);
    }
    result
}
