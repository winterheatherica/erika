use std::path::PathBuf;

use eframe::egui;
use egui::Color32;

use crate::app::{
    App, ColorPickTarget, PNG_EXPORT_DIR, SVG_EXPORT_DIR, TEX_EXPORT_DIR, TEX_TEMPLATE_PATH,
};
use crate::export::{ExportConfig, export_png};
use crate::export_path::build_dynamic_path;
use crate::svg_export::{SvgConfig, export_svg};
use crate::tex_export::{TexConfig, export_tex};

impl App {
    pub(crate) fn top_bar(&mut self, ui: &mut egui::Ui) {
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
                let path = build_dynamic_path(&self.export_path, PNG_EXPORT_DIR, "png");
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
                let path = build_dynamic_path(&self.svg_export_path, SVG_EXPORT_DIR, "svg");
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
            ui.separator();
            ui.label("TEX:");
            ui.add(
                egui::TextEdit::singleline(&mut self.tex_export_path)
                    .desired_width(160.0)
                    .hint_text("output.tex"),
            );
            if ui.button("Export TEX").clicked() {
                let path = build_dynamic_path(&self.tex_export_path, TEX_EXPORT_DIR, "tex");
                let template_path = PathBuf::from(TEX_TEMPLATE_PATH);
                let cfg = TexConfig {
                    path: &path,
                    template_path: &template_path,
                };
                self.last_msg = Some(match export_tex(&self.curves, &self.groups, &cfg) {
                    Ok(()) => format!("Exported TEX → {}", path.display()),
                    Err(e) => format!("Error: {e}"),
                });
            }
            if let Some(msg) = &self.last_msg {
                ui.label(egui::RichText::new(msg).color(Color32::from_rgb(60, 100, 60)));
            }
        });
    }
}

pub(crate) fn pick_color_button_widget(
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
