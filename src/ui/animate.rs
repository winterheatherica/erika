use std::path::PathBuf;

use eframe::egui;
use egui::Color32;

use crate::app::{App, AnimFormat, JS_EXPORT_DIR, SVG_EXPORT_DIR};
use crate::export::anim::{self, DiffKind};
use crate::export::anim_js::{self, JsDiffKind};
use crate::export::path::build_named_path;

impl App {
    pub(crate) fn animate_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut do_generate = false;

        egui::Window::new("Animate")
            .open(&mut open)
            .resizable(true)
            .default_size([720.0, 600.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    ui.selectable_value(&mut self.anim_format, AnimFormat::Svg, AnimFormat::Svg.label());
                    ui.selectable_value(
                        &mut self.anim_format,
                        AnimFormat::DesmosJs,
                        AnimFormat::DesmosJs.label(),
                    );
                });
                ui.separator();

                do_generate = match self.anim_format {
                    AnimFormat::Svg => self.animate_svg_body(ui),
                    AnimFormat::DesmosJs => self.animate_js_body(ui),
                };

                if let Some(msg) = &self.last_msg {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(msg).color(Color32::from_rgb(60, 100, 60)));
                }
            });

        self.show_animate = open;
        if do_generate {
            match self.anim_format {
                AnimFormat::Svg => self.generate_animation(),
                AnimFormat::DesmosJs => self.generate_js_animation(),
            }
        }
    }

    fn animate_svg_body(&mut self, ui: &mut egui::Ui) -> bool {
        let files = anim::list_svg_files(SVG_EXPORT_DIR);
        let mut do_generate = false;

        ui.label(egui::RichText::new("Morph between two exported SVGs").strong());
        ui.label(
            egui::RichText::new(
                "Matching lines are compared in order. Identical lines stay still; \
                 changed lines animate with a per-line duration.",
            )
            .small()
            .weak(),
        );
        ui.add_space(6.0);

        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("From (A):");
            changed |= file_combo(ui, "anim_svg_from", &files, &mut self.anim_from);
        });
        ui.horizontal(|ui| {
            ui.label("To (B):");
            changed |= file_combo(ui, "anim_svg_to", &files, &mut self.anim_to);
        });

        let mut apply_all = false;
        ui.horizontal(|ui| {
            if ui.button("↻ Detect differences").clicked() {
                changed = true;
            }
            ui.separator();
            ui.label("Default:");
            ui.add(
                egui::DragValue::new(&mut self.anim_default_dur)
                    .range(0.1..=60.0)
                    .speed(0.1)
                    .suffix(" s"),
            );
            if ui.button("Apply to all").clicked() {
                apply_all = true;
            }
        });

        if changed {
            self.recompute_diff();
        }
        ui.separator();

        let from_set = self.anim_from.is_some();
        let to_set = self.anim_to.is_some();
        let default_dur = self.anim_default_dur;

        if let Some(diff) = &mut self.anim_diff {
            let changing = diff.changing_count();
            ui.label(format!(
                "{} identical (kept static)  ·  {} changing",
                diff.same_count, changing
            ));
            ui.add_space(4.0);

            if apply_all {
                for r in diff.rows.iter_mut() {
                    if !matches!(r.kind, DiffKind::Same) {
                        r.dur = default_dur;
                    }
                }
            }

            if changing == 0 {
                ui.label(egui::RichText::new("These two files have no differing lines.").weak());
            } else {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    egui::Grid::new("anim_svg_grid")
                        .num_columns(4)
                        .striped(true)
                        .spacing([14.0, 6.0])
                        .show(ui, |ui| {
                            grid_header(ui, "Line");
                            for r in diff.rows.iter_mut() {
                                if matches!(r.kind, DiffKind::Same) {
                                    continue;
                                }
                                ui.checkbox(&mut r.enabled, "");
                                row_label(ui, r.color, &r.label);
                                ui.label(r.kind.label());
                                ui.add_enabled(
                                    r.enabled,
                                    egui::DragValue::new(&mut r.dur)
                                        .range(0.1..=60.0)
                                        .speed(0.1)
                                        .suffix(" s"),
                                );
                                ui.end_row();
                            }
                        });
                });
            }

            ui.separator();
            let ready = from_set && to_set && changing > 0;
            if ui
                .add_enabled(ready, egui::Button::new("✨ Generate animated SVG"))
                .clicked()
            {
                do_generate = true;
            }
        } else {
            ui.label(egui::RichText::new("Pick two SVG files above to compare.").weak());
        }
        do_generate
    }

    fn animate_js_body(&mut self, ui: &mut egui::Ui) -> bool {
        let files = anim_js::list_js_files(JS_EXPORT_DIR);
        let mut do_generate = false;

        ui.label(egui::RichText::new("Morph through exported Desmos JS keyframes").strong());
        ui.label(
            egui::RichText::new(
                "Keyframes play in order on one auto-playing timeline (ping-pong); each step \
                 has its own duration. Changed curves interpolate their control points. Paste \
                 the result into the Desmos console.",
            )
            .small()
            .weak(),
        );
        ui.add_space(4.0);
        ui.checkbox(&mut self.anim_js_normalize, "Normalize keyframes")
            .on_hover_text(
                "Resample every shape to the same point count and match shapes across \
                 keyframes by position, so files with different curves still morph. Shapes \
                 that appear or vanish shrink to a point.",
            );
        if self.anim_js_normalize {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label("Points per shape:");
                ui.add(egui::DragValue::new(&mut self.anim_js_points).range(4..=99))
                    .on_hover_text(
                        "Segments each shape is rebuilt with. Higher = closer to the \
                         original outline, up to the Desmos domain of 99.",
                    );
            });
        }
        ui.add_space(6.0);

        let want_durs = self.anim_js_files.len().saturating_sub(1);
        while self.anim_js_durs.len() < want_durs {
            self.anim_js_durs.push(self.anim_default_dur);
        }
        self.anim_js_durs.truncate(want_durs);

        let mut changed = false;
        let mut remove_idx: Option<usize> = None;
        let n = self.anim_js_files.len();
        for i in 0..n {
            ui.horizontal(|ui| {
                ui.label(format!("Keyframe {}:", i + 1));
                changed |= file_combo(
                    ui,
                    &format!("anim_js_kf_{i}"),
                    &files,
                    &mut self.anim_js_files[i],
                );
                if n > 2 && ui.small_button("✖").clicked() {
                    remove_idx = Some(i);
                }
            });
            if i + 1 < n {
                ui.horizontal(|ui| {
                    ui.add_space(80.0);
                    ui.label("⬇ step");
                    ui.add(
                        egui::DragValue::new(&mut self.anim_js_durs[i])
                            .range(0.1..=60.0)
                            .speed(0.1)
                            .suffix(" s"),
                    );
                });
            }
        }
        if let Some(i) = remove_idx {
            self.anim_js_files.remove(i);
            let di = i.saturating_sub(1);
            if di < self.anim_js_durs.len() {
                self.anim_js_durs.remove(di);
            }
            changed = true;
        }
        ui.horizontal(|ui| {
            if ui.button("➕ Add keyframe").clicked() {
                self.anim_js_files.push(None);
                self.anim_js_durs.push(self.anim_default_dur);
                changed = true;
            }
            let total: f32 = self.anim_js_durs.iter().sum();
            ui.label(egui::RichText::new(format!("Total: {total:.1} s")).weak());
        });

        let mut apply_all = false;
        ui.horizontal(|ui| {
            if ui.button("↻ Detect differences").clicked() {
                changed = true;
            }
            ui.separator();
            ui.label("Default:");
            ui.add(
                egui::DragValue::new(&mut self.anim_default_dur)
                    .range(0.1..=60.0)
                    .speed(0.1)
                    .suffix(" s"),
            );
            if ui.button("Apply to all steps").clicked() {
                apply_all = true;
            }
        });
        if apply_all {
            let d = self.anim_default_dur;
            for v in &mut self.anim_js_durs {
                *v = d;
            }
        }

        if changed {
            self.recompute_js_diff();
        }
        ui.separator();

        let all_set = self.anim_js_files.iter().all(|f| f.is_some());

        let normalized = self.anim_js_normalize;
        if let Some(diff) = &mut self.anim_js_diff {
            let changing = diff.changing_count();
            let summary = if normalized {
                format!(
                    "{} keyframes  ·  {} shape slots, all morphing",
                    diff.keyframes, changing
                )
            } else {
                let mut s = format!(
                    "{} keyframes  ·  {} identical (kept static)  ·  {} changing",
                    diff.keyframes, diff.same_count, changing
                );
                if diff.added > 0 || diff.removed > 0 {
                    s.push_str(&format!(
                        "  ·  {} added / {} removed (not animated)",
                        diff.added, diff.removed
                    ));
                }
                s
            };
            ui.label(summary);
            ui.add_space(4.0);

            if changing == 0 {
                ui.label(egui::RichText::new("These keyframes have no differing curves.").weak());
            } else {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    egui::Grid::new("anim_js_grid")
                        .num_columns(3)
                        .striped(true)
                        .spacing([14.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("On").strong());
                            ui.label(
                                egui::RichText::new(if normalized { "Shape" } else { "Curve" })
                                    .strong(),
                            );
                            ui.label(egui::RichText::new("Change").strong());
                            ui.end_row();
                            for r in diff.rows.iter_mut() {
                                let morphable = matches!(r.kind, JsDiffKind::Morph);
                                ui.add_enabled_ui(morphable, |ui| {
                                    ui.checkbox(&mut r.enabled, "");
                                });
                                row_label(ui, r.color, &r.label);
                                ui.label(r.kind.label());
                                ui.end_row();
                            }
                        });
                });
            }

            ui.separator();
            let any_morph = diff
                .rows
                .iter()
                .any(|r| matches!(r.kind, JsDiffKind::Morph) && r.enabled);
            let ready = all_set && any_morph;
            if ui
                .add_enabled(ready, egui::Button::new("✨ Generate animated JS"))
                .clicked()
            {
                do_generate = true;
            }
        } else {
            ui.label(
                egui::RichText::new("Pick a Desmos JS file for every keyframe to compare.").weak(),
            );
        }
        do_generate
    }

    fn recompute_diff(&mut self) {
        let (Some(a), Some(b)) = (self.anim_from.clone(), self.anim_to.clone()) else {
            self.anim_diff = None;
            return;
        };
        match anim::load_diff(&a, &b, self.anim_default_dur) {
            Ok(diff) => self.anim_diff = Some(diff),
            Err(e) => {
                self.anim_diff = None;
                self.last_msg = Some(format!("Animate error: {e}"));
            }
        }
    }

    fn recompute_js_diff(&mut self) {
        let paths: Vec<PathBuf> = self.anim_js_files.iter().flatten().cloned().collect();
        if paths.len() < 2 || paths.len() != self.anim_js_files.len() {
            self.anim_js_diff = None;
            return;
        }
        let normalize = self.anim_js_normalize.then_some(self.anim_js_points);
        match anim_js::load_js_seq(&paths, normalize) {
            Ok(diff) => self.anim_js_diff = Some(diff),
            Err(e) => {
                self.anim_js_diff = None;
                self.last_msg = Some(format!("Animate error: {e}"));
            }
        }
    }

    fn generate_animation(&mut self) {
        let Some(diff) = &self.anim_diff else {
            return;
        };
        let svg = anim::build_animated_svg(diff);
        let stem = format!("{}-morph", self.export_name);
        let path = build_named_path(&stem, SVG_EXPORT_DIR, "svg");
        self.last_msg = Some(match std::fs::write(&path, svg) {
            Ok(()) => format!("Exported animation → {}", path.display()),
            Err(e) => format!("Error: {e}"),
        });
    }

    fn generate_js_animation(&mut self) {
        let Some(diff) = &self.anim_js_diff else {
            return;
        };
        let js = anim_js::build_animated_js(diff, &self.anim_js_durs);
        let stem = format!("{}-morph", self.export_name);
        let path = build_named_path(&stem, JS_EXPORT_DIR, "js");
        self.last_msg = Some(match std::fs::write(&path, js) {
            Ok(()) => format!("Exported animation → {}", path.display()),
            Err(e) => format!("Error: {e}"),
        });
    }
}

fn grid_header(ui: &mut egui::Ui, second: &str) {
    ui.label(egui::RichText::new("On").strong());
    ui.label(egui::RichText::new(second).strong());
    ui.label(egui::RichText::new("Change").strong());
    ui.label(egui::RichText::new("Duration").strong());
    ui.end_row();
}

fn row_label(ui: &mut egui::Ui, color: [u8; 3], label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 2.0, Color32::from_rgb(color[0], color[1], color[2]));
        ui.label(label);
    });
}

pub(crate) fn file_combo(
    ui: &mut egui::Ui,
    id: &str,
    files: &[PathBuf],
    sel: &mut Option<PathBuf>,
) -> bool {
    let mut changed = false;
    let current = sel
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("— pick a file —")
        .to_string();
    egui::ComboBox::from_id_salt(id)
        .selected_text(current)
        .width(420.0)
        .show_ui(ui, |ui| {
            for f in files {
                let name = f
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                let is_sel = sel.as_deref() == Some(f.as_path());
                if ui.selectable_label(is_sel, name).clicked() {
                    *sel = Some(f.clone());
                    changed = true;
                }
            }
        });
    changed
}
