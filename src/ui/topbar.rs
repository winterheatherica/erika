use std::path::PathBuf;

use eframe::egui;
use egui::Color32;

use crate::app::{
    App, ColorPickTarget, JS_EXPORT_DIR, PNG_EXPORT_DIR, SVG_EXPORT_DIR, TEX_EXPORT_DIR,
    TEX_TEMPLATE_PATH,
};
use crate::export::js::{JsConfig, export_js};
use crate::export::path::build_named_path;
use crate::export::png::{ExportConfig, export_png};
use crate::export::svg::{SvgConfig, export_svg};
use crate::export::tex::{TexConfig, export_tex};

impl App {
    pub(crate) fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            if !self.show_top_bar {
                if ui
                    .button("▾ Toolbar")
                    .on_hover_text("Show the toolbar")
                    .clicked()
                {
                    self.show_top_bar = true;
                }
                return;
            }

            if ui
                .button("▴")
                .on_hover_text("Hide the toolbar")
                .clicked()
            {
                self.show_top_bar = false;
            }
            ui.label(egui::RichText::new("Erika").strong().size(16.0));
            ui.separator();

            self.file_menu(ui);
            self.edit_menu(ui);
            self.view_menu(ui);
            self.export_menu(ui);

            if ui
                .button("Animate")
                .on_hover_text("Morph between two exported SVGs")
                .clicked()
            {
                self.show_animate = true;
            }

            if ui
                .button("Trace")
                .on_hover_text("Turn a black & white image into Bézier curves")
                .clicked()
            {
                self.show_trace = true;
            }

            let msg = self.last_msg.clone();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(msg) = &msg {
                    ui.label(egui::RichText::new(msg).color(Color32::from_rgb(60, 100, 60)));
                }
            });
        });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("💾 Save").clicked() {
                self.save_dialog();
                ui.close_menu();
            }
            if ui.button("📂 Load…").clicked() {
                self.show_gallery = true;
                ui.close_menu();
            }
        });
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Edit", |ui| {
            let undo_enabled = !self.undo_stack.is_empty()
                || self.curves != self.last_committed_curves
                || self.groups != self.last_committed_groups;
            let redo_enabled = !self.redo_stack.is_empty();
            if ui
                .add_enabled(undo_enabled, egui::Button::new("↶ Undo"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                self.undo();
                ui.close_menu();
            }
            if ui
                .add_enabled(redo_enabled, egui::Button::new("↷ Redo"))
                .on_hover_text("Ctrl+Y / Ctrl+Shift+Z")
                .clicked()
            {
                self.redo();
                ui.close_menu();
            }
            ui.separator();
            ui.checkbox(&mut self.link_continuity, "Link S3[i]=S1[i+1]");
            ui.horizontal(|ui| {
                ui.label("Samples/seg:");
                ui.add(egui::DragValue::new(&mut self.samples_per_segment).range(4..=512));
            });
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("View", |ui| {
            ui.checkbox(&mut self.show_grid, "Grid");
            ui.checkbox(&mut self.show_axes, "Axes");
            ui.checkbox(&mut self.show_handles_all, "Handles");
            ui.separator();
            if ui.button("Fit view").clicked() {
                let size = ui.ctx().screen_rect().size();
                self.fit_to_curves(size);
                ui.close_menu();
            }
            let panel_label = if self.show_left_panel {
                "Hide left panel"
            } else {
                "Show left panel"
            };
            if ui.button(panel_label).on_hover_text("Ctrl+B").clicked() {
                self.show_left_panel = !self.show_left_panel;
                ui.close_menu();
            }
            ui.separator();
            ui.label(egui::RichText::new("Background").strong());
            let mut transparent = self.background.is_none();
            if ui
                .checkbox(&mut transparent, "Transparent")
                .on_hover_text("Transparent background (no fill)")
                .changed()
            {
                self.background = if transparent { None } else { Some([245, 245, 250]) };
            }
            if let Some(bg) = self.background.as_mut() {
                let _ = crate::ui::color::color_edit_hex_rgb(ui, "bg_hex", bg);
            }
            let img_ready = self.any_image_ready();
            pick_color_button_widget(
                ui,
                ColorPickTarget::Background,
                &mut self.color_pick_target,
                img_ready,
            );
        });
    }

    fn export_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Export", |ui| {
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.export_name)
                        .desired_width(160.0)
                        .hint_text("output"),
                );
            });

            ui.separator();
            export_target(ui, "PNG", PNG_EXPORT_DIR, "png");
            ui.horizontal(|ui| {
                ui.label("W");
                ui.add(egui::DragValue::new(&mut self.export_w).range(64..=8192));
                ui.label("H");
                ui.add(egui::DragValue::new(&mut self.export_h).range(64..=8192));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.export_transparent, "Transparent");
                ui.label("samples:");
                ui.add(egui::DragValue::new(&mut self.export_samples).range(8..=512));
            });
            if ui.button("Export PNG").clicked() {
                let path = build_named_path(&self.export_name, PNG_EXPORT_DIR, "png");
                let bg = if self.export_transparent {
                    None
                } else {
                    self.background
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
                ui.close_menu();
            }

            ui.separator();
            export_target(ui, "SVG", SVG_EXPORT_DIR, "svg");
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.export_frame_lock, "Lock frame").on_hover_text(
                    "Use a fixed world frame across exports so unchanged lines stay \
                     byte-identical (needed for clean Animate diffs). Auto-fit when off.",
                );
                if ui
                    .button("Set to current art")
                    .on_hover_text("Capture the current drawing bounds as the locked frame")
                    .clicked()
                {
                    self.capture_export_frame();
                }
            });
            if self.export_frame_lock {
                match self.export_frame {
                    Some(f) => {
                        ui.label(
                            egui::RichText::new(format!(
                                "frame x[{:.1}, {:.1}]  y[{:.1}, {:.1}]",
                                f[0], f[2], f[1], f[3]
                            ))
                            .small()
                            .weak(),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("frame captured on first export")
                                .small()
                                .weak(),
                        );
                    }
                }
            }
            if ui.button("Export SVG").clicked() {
                if self.export_frame_lock && self.export_frame.is_none() {
                    self.capture_export_frame();
                }
                let path = build_named_path(&self.export_name, SVG_EXPORT_DIR, "svg");
                let bg = if self.export_transparent {
                    None
                } else {
                    self.background
                };
                let frame = if self.export_frame_lock {
                    self.export_frame.map(|f| (f[0], f[1], f[2], f[3]))
                } else {
                    None
                };
                let cfg = SvgConfig {
                    width: self.export_w,
                    height: self.export_h,
                    background: bg,
                    padding_fraction: 0.05,
                    frame,
                    path: &path,
                };
                self.last_msg = Some(match export_svg(&self.curves, &cfg) {
                    Ok(()) => format!("Exported SVG → {}", path.display()),
                    Err(e) => format!("Error: {e}"),
                });
                ui.close_menu();
            }

            ui.separator();
            export_target(ui, "TEX", TEX_EXPORT_DIR, "tex");
            if ui.button("Export TEX").clicked() {
                let path = build_named_path(&self.export_name, TEX_EXPORT_DIR, "tex");
                let template_path = PathBuf::from(TEX_TEMPLATE_PATH);
                let cfg = TexConfig {
                    path: &path,
                    template_path: &template_path,
                };
                self.last_msg = Some(match export_tex(&self.curves, &self.groups, &cfg) {
                    Ok(()) => format!("Exported TEX → {}", path.display()),
                    Err(e) => format!("Error: {e}"),
                });
                ui.close_menu();
            }

            ui.separator();
            export_target(ui, "JS (Desmos)", JS_EXPORT_DIR, "js");
            ui.checkbox(&mut self.js_timelapse, "Time-lapse").on_hover_text(
                "Animate the drawing folder by folder with a play slider (S). \
                 Speed follows the Time-lapse Duration in the left panel.",
            );
            ui.horizontal(|ui| {
                ui.label("Layout:");
                ui.selectable_value(&mut self.js_grouped, true, "Grouped")
                    .on_hover_text("One Desmos folder per group, fills in front of each folder");
                ui.selectable_value(&mut self.js_grouped, false, "Non-grouped")
                    .on_hover_text(
                        "No art folders: every curve is emitted bottom to top as fill then \
                         line, so stacking matches the canvas exactly",
                    );
            });
            if ui.button("Export JS").clicked() {
                let path = build_named_path(&self.export_name, JS_EXPORT_DIR, "js");
                let template_path = PathBuf::from(TEX_TEMPLATE_PATH);
                let cfg = JsConfig {
                    path: &path,
                    template_path: &template_path,
                    timelapse: self.js_timelapse,
                    duration_secs: self.playback_duration_secs,
                    grouped: self.js_grouped,
                };
                self.last_msg = Some(match export_js(&self.curves, &self.groups, &cfg) {
                    Ok(()) => format!("Exported JS → {}", path.display()),
                    Err(e) => format!("Error: {e}"),
                });
                ui.close_menu();
            }
        });
    }
}

fn export_target(ui: &mut egui::Ui, label: &str, dir: &str, ext: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).strong());
        ui.label(egui::RichText::new(format!("→  {dir}/….{ext}")).weak());
    });
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
