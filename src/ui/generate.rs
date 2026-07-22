use std::path::PathBuf;

use eframe::egui;
use egui::Color32;

use crate::app::{App, IMPORT_IMAGE_DIR};
use crate::model::curve::{CurveSet, P};
use crate::trace::{self, TraceOptions};
use crate::ui::animate::file_combo;

pub(crate) fn list_image_files(dir: &str) -> Vec<PathBuf> {
    let exts = ["png", "jpg", "jpeg", "bmp", "webp"];
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|e| exts.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

impl App {
    pub(crate) fn trace_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut do_trace = false;

        egui::Window::new("Trace")
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Trace a black & white image into curves").strong());
                ui.label(
                    egui::RichText::new(
                        "Every outline becomes one curve: a single chain of quadratic Bézier \
                         segments, however long it needs to be. All curves land in one new \
                         folder. Load the same file as a reference image first to have them \
                         line up on the canvas.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(6.0);

                let files = list_image_files(IMPORT_IMAGE_DIR);
                ui.horizontal(|ui| {
                    ui.label("Image:");
                    file_combo(ui, "trace_image", &files, &mut self.trace_image);
                });

                ui.horizontal(|ui| {
                    ui.label("Threshold:");
                    ui.add(egui::DragValue::new(&mut self.trace_threshold).range(0..=255))
                        .on_hover_text("Pixels darker than this count as ink");
                    ui.checkbox(&mut self.trace_invert, "Invert")
                        .on_hover_text("Trace the light areas instead of the dark ones");
                });
                ui.horizontal(|ui| {
                    ui.label("Min area:");
                    ui.add(
                        egui::DragValue::new(&mut self.trace_min_area)
                            .range(1..=100000)
                            .suffix(" px"),
                    )
                    .on_hover_text("Outlines enclosing fewer pixels are skipped");
                    ui.label("Detail:");
                    ui.add(
                        egui::DragValue::new(&mut self.trace_simplify)
                            .range(0.0..=12.0)
                            .speed(0.1)
                            .suffix(" px"),
                    )
                    .on_hover_text("Simplification tolerance. Lower = more segments, closer fit.");
                });
                ui.horizontal(|ui| {
                    ui.label("Corner above:");
                    ui.add(
                        egui::DragValue::new(&mut self.trace_corner_deg)
                            .range(0.0..=180.0)
                            .suffix("°"),
                    )
                    .on_hover_text("Turns sharper than this stay pointed instead of rounding");
                });
                ui.checkbox(&mut self.trace_fill, "Fill shapes")
                    .on_hover_text(
                        "Fill each outline instead of stroking it. Holes are drawn on top in \
                         the background color.",
                    );

                ui.add_space(6.0);
                if ui
                    .add_enabled(
                        self.trace_image.is_some(),
                        egui::Button::new("✨ Generate curves"),
                    )
                    .clicked()
                {
                    do_trace = true;
                }

                if let Some(msg) = &self.last_msg {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(msg).color(Color32::from_rgb(60, 100, 60)));
                }
            });

        self.show_trace = open;
        if do_trace {
            self.run_trace();
        }
    }

    fn run_trace(&mut self) {
        let Some(path) = self.trace_image.clone() else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.last_msg = Some(format!("Trace error: read failed: {e}"));
                return;
            }
        };
        let decoded = match image::load_from_memory(&bytes) {
            Ok(i) => i,
            Err(e) => {
                self.last_msg = Some(format!("Trace error: decode failed: {e}"));
                return;
            }
        };
        let rgba = decoded.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);

        let opts = TraceOptions {
            threshold: self.trace_threshold,
            invert: self.trace_invert,
            min_area_px: self.trace_min_area as f64,
            simplify_px: self.trace_simplify,
            corner_deg: self.trace_corner_deg,
        };
        let shapes = trace::trace_bitmap(rgba.as_raw(), w, h, &opts);
        if shapes.is_empty() {
            self.last_msg = Some(
                "Trace found no outlines - adjust the threshold, Invert, or lower Min area."
                    .to_string(),
            );
            return;
        }

        let placement = self
            .reference_images
            .iter()
            .find(|i| i.is_ready() && i.path.file_name() == path.file_name())
            .map(|i| (i.world_x, i.world_y, i.world_w, i.world_h));
        let (ox, oy, ww, wh) = placement.unwrap_or_else(|| {
            let ww = 12.0f32;
            let wh = ww * (h as f32 / w.max(1) as f32);
            (-ww * 0.5, -wh * 0.5, ww, wh)
        });
        let to_world =
            |(px, py): (f32, f32)| P::new(ox + px / w as f32 * ww, oy + (1.0 - py / h as f32) * wh);

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("trace")
            .to_string();
        let gid = self.add_group();
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == gid) {
            g.name = stem.clone();
        }

        let ink = if self.trace_invert {
            [245, 245, 245]
        } else {
            [20, 20, 20]
        };
        let hole_color = self.background.unwrap_or([245, 245, 250]);

        let mut added = 0usize;
        let mut segments = 0usize;
        for (si, shape) in shapes.iter().enumerate() {
            let Some(spline) = trace::quad_spline_closed(&shape.points, opts.corner_deg) else {
                continue;
            };
            if spline.is_empty() {
                continue;
            }
            let color = if shape.is_hole { hole_color } else { ink };
            let mut c = CurveSet::empty(format!("Path {}", si + 1), color);
            c.group_id = Some(gid);
            c.show_handles = false;
            c.thickness = 2.0;
            if self.trace_fill {
                c.fill_enabled = true;
                c.fill_color = [color[0], color[1], color[2], 255];
                c.stroke_visible = false;
            }
            c.s1 = spline.s1.iter().copied().map(to_world).collect();
            c.s2 = spline.s2.iter().copied().map(to_world).collect();
            c.s3 = spline.s3.iter().copied().map(to_world).collect();
            segments += spline.len();
            self.curves.push(c);
            added += 1;
        }

        if added == 0 {
            self.last_msg = Some("Trace produced no usable outlines.".to_string());
            return;
        }

        self.multi_select.clear();
        self.set_active_group(gid);
        self.selected = self.curves.len() - 1;
        self.last_msg = Some(format!(
            "Traced {added} curves, {segments} segments → folder \"{stem}\""
        ));
    }
}
