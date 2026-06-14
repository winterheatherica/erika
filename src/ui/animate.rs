use std::path::PathBuf;

use eframe::egui;
use egui::Color32;

use crate::app::{App, SVG_EXPORT_DIR};
use crate::export::anim::{self, DiffKind};
use crate::export::path::build_named_path;

impl App {
    pub(crate) fn animate_window(&mut self, ctx: &egui::Context) {
        let files = anim::list_svg_files(SVG_EXPORT_DIR);
        let mut open = true;
        let mut do_generate = false;

        egui::Window::new("Animate")
            .open(&mut open)
            .resizable(true)
            .default_size([720.0, 560.0])
            .show(ctx, |ui| {
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
                    changed |= file_combo(ui, "anim_from", &files, &mut self.anim_from);
                });
                ui.horizontal(|ui| {
                    ui.label("To (B):");
                    changed |= file_combo(ui, "anim_to", &files, &mut self.anim_to);
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
                        ui.label(
                            egui::RichText::new("These two files have no differing lines.")
                                .weak(),
                        );
                    } else {
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            egui::Grid::new("anim_rows")
                                .num_columns(4)
                                .striped(true)
                                .spacing([14.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("On").strong());
                                    ui.label(egui::RichText::new("Line").strong());
                                    ui.label(egui::RichText::new("Change").strong());
                                    ui.label(egui::RichText::new("Duration").strong());
                                    ui.end_row();

                                    for r in diff.rows.iter_mut() {
                                        if matches!(r.kind, DiffKind::Same) {
                                            continue;
                                        }
                                        ui.checkbox(&mut r.enabled, "");
                                        ui.horizontal(|ui| {
                                            let c = r.color;
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(12.0, 12.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(
                                                rect,
                                                2.0,
                                                Color32::from_rgb(c[0], c[1], c[2]),
                                            );
                                            ui.label(&r.label);
                                        });
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
                    ui.label(
                        egui::RichText::new("Pick two SVG files above to compare.").weak(),
                    );
                }

                if let Some(msg) = &self.last_msg {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(msg).color(Color32::from_rgb(60, 100, 60)));
                }
            });

        self.show_animate = open;
        if do_generate {
            self.generate_animation();
        }
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
}

fn file_combo(
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
