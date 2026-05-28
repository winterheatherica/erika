use eframe::egui;
use egui::Color32;

use crate::app::{App, ColorPickTarget, RenderMode};
use crate::curve::{Arr, CurveKind, CurveSet, LineStyle, P, PALETTE};
use crate::ui_topbar::pick_color_button_widget;

impl App {
    pub(crate) fn left_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("left_scroll")
            .show(ui, |ui| {
                self.groups_section(ui);
                ui.separator();
                self.curves_section(ui);
                ui.separator();
                self.edit_overrides_section(ui);
                ui.separator();
                self.timelapse_section(ui);
                ui.separator();
                self.image_section(ui);
                ui.separator();
                self.shape_editor_section(ui);
            });
    }

    fn groups_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Groups");
        ui.label(
            egui::RichText::new(
                "TEX param: first char = variable, rest joins subscript (e.g. \"Head\" → H_{ead1}).",
            )
            .small()
            .weak(),
        );

        if ui.button("+ Add group").clicked() {
            let id = self.add_group();
            self.new_curve_group_id = Some(id);
        }

        let mut remove_group_id: Option<u64> = None;
        let can_delete = self.groups.len() > 1;
        for g in &mut self.groups {
            ui.horizontal(|ui| {
                ui.label("📁");
                ui.add(
                    egui::TextEdit::singleline(&mut g.name)
                        .desired_width(110.0)
                        .hint_text("name"),
                );
                ui.label("param:");
                ui.add(
                    egui::TextEdit::singleline(&mut g.tex_param)
                        .desired_width(90.0)
                        .hint_text("e.g. A"),
                );
                let del = ui.add_enabled(can_delete, egui::Button::new("🗑").small());
                let del = if can_delete {
                    del.on_hover_text("Delete group (curves reassigned to first remaining)")
                } else {
                    del.on_hover_text("Cannot delete the last remaining group")
                };
                if del.clicked() {
                    remove_group_id = Some(g.id);
                }
            });
        }
        if let Some(id) = remove_group_id {
            self.remove_group(id);
        }
    }

    fn edit_overrides_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Edit overrides");
        ui.label(
            egui::RichText::new("Applied during editing only (not during time-lapse playback).")
                .small()
                .weak(),
        );
        ui.checkbox(&mut self.edit_hide_handles, "Hide all handles");
        ui.checkbox(
            &mut self.edit_hide_control_polygon,
            "Hide all control polygons",
        );
        ui.checkbox(&mut self.edit_show_all_strokes, "Show all strokes");
        ui.checkbox(&mut self.edit_show_all_fills, "Show all fills");
        ui.checkbox(&mut self.edit_hide_all_fills, "Hide all fills")
            .on_hover_text("Wins over \"Show all fills\" if both are checked.");
    }

    fn timelapse_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Time-lapse");
        let total = self.total_draw_units();
        ui.label(
            egui::RichText::new(format!(
                "{} units (layer order = play order, top first)",
                total
            ))
            .small()
            .weak(),
        );

        ui.horizontal(|ui| {
            let playing = self.playback_active;
            let play_label = if playing { "⏸ Pause" } else { "▶ Play" };
            if ui.add_enabled(total > 0, egui::Button::new(play_label)).clicked() {
                if playing {
                    self.playback_pause();
                } else {
                    self.playback_play();
                }
            }
            if ui.button("⏹ Reset").clicked() {
                self.playback_stop();
            }
            if ui.button("⏭ End").clicked() {
                self.playback_seek_end();
            }
            ui.checkbox(&mut self.playback_loop, "Loop");
        });

        ui.horizontal(|ui| {
            ui.label("Progress:");
            let resp = ui.add(
                egui::Slider::new(&mut self.playback_progress, 0.0..=1.0)
                    .show_value(true)
                    .fixed_decimals(3),
            );
            if resp.dragged() {
                self.playback_active = false;
                self.playback_last_tick = None;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Duration:");
            ui.add(
                egui::DragValue::new(&mut self.playback_duration_secs)
                    .speed(0.1)
                    .range(0.1..=120.0)
                    .suffix(" s"),
            );
        });

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Render mode (overrides per-curve stroke/fill toggles):")
                .small()
                .weak(),
        );
        for mode in [
            RenderMode::Real,
            RenderMode::StrokeOnly,
            RenderMode::FillOnly,
            RenderMode::Both,
        ] {
            ui.radio_value(&mut self.render_mode, mode, mode.label());
        }
    }

    fn curves_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Curves");
        let groups_info: Vec<(u64, String)> = self
            .groups
            .iter()
            .map(|g| (g.id, g.name.clone()))
            .collect();
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
            let current_group_label = self
                .new_curve_group_id
                .and_then(|id| {
                    groups_info
                        .iter()
                        .find(|(gid, _)| *gid == id)
                        .map(|(_, n)| n.as_str())
                })
                .unwrap_or("(none)");
            egui::ComboBox::from_id_salt("new_curve_group")
                .selected_text(current_group_label)
                .width(100.0)
                .show_ui(ui, |ui| {
                    for (gid, gname) in &groups_info {
                        ui.selectable_value(&mut self.new_curve_group_id, Some(*gid), gname);
                    }
                });
            if ui.button("+ Add").clicked() {
                let name = if self.new_curve_name.trim().is_empty() {
                    format!("Curve {}", self.curves.len() + 1)
                } else {
                    self.new_curve_name.trim().to_string()
                };
                let color = PALETTE[self.curves.len() % PALETTE.len()];
                let mut new_curve = match self.new_curve_kind {
                    CurveKind::Bezier => CurveSet::empty(name, color),
                    CurveKind::Ellipse => {
                        CurveSet::new_ellipse(name, color, self.center_x, self.center_y)
                    }
                };
                new_curve.group_id = self
                    .new_curve_group_id
                    .or_else(|| self.groups.first().map(|g| g.id));
                self.curves.push(new_curve);
                self.selected = self.curves.len() - 1;
                self.new_curve_name.clear();
            }
        });

        let mut to_delete: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut to_forward: Option<usize> = None;
        let mut to_backward: Option<usize> = None;
        let mut to_front: Option<usize> = None;
        let mut to_back: Option<usize> = None;
        let total = self.curves.len();
        ui.label(
            egui::RichText::new("Layer order: top of list = behind, bottom = in front")
                .small()
                .weak(),
        );
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
                let group_label = c
                    .group_id
                    .and_then(|id| {
                        groups_info
                            .iter()
                            .find(|(gid, _)| *gid == id)
                            .map(|(_, n)| n.as_str())
                    })
                    .unwrap_or("(none)");
                egui::ComboBox::from_id_salt(format!("grp_{i}"))
                    .selected_text(group_label)
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for (gid, gname) in &groups_info {
                            ui.selectable_value(&mut c.group_id, Some(*gid), gname);
                        }
                    });
                if ui
                    .add_enabled(i + 1 < total, egui::Button::new("▲").small())
                    .on_hover_text("Bring forward (draw on top of next)")
                    .clicked()
                {
                    to_forward = Some(i);
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("▼").small())
                    .on_hover_text("Send backward (draw behind previous)")
                    .clicked()
                {
                    to_backward = Some(i);
                }
                if ui
                    .add_enabled(i + 1 < total, egui::Button::new("⏫").small())
                    .on_hover_text("Bring to front (draw on top of everything)")
                    .clicked()
                {
                    to_front = Some(i);
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("⏬").small())
                    .on_hover_text("Send to back (draw behind everything)")
                    .clicked()
                {
                    to_back = Some(i);
                }
                if ui.small_button("🗑").clicked() {
                    to_delete = Some(i);
                }
            });
        }
        if let Some(i) = select {
            self.selected = i;
        }
        if let Some(i) = to_forward {
            self.move_curve_forward(i);
        }
        if let Some(i) = to_backward {
            self.move_curve_backward(i);
        }
        if let Some(i) = to_front {
            self.move_curve_to_front(i);
        }
        if let Some(i) = to_back {
            self.move_curve_to_back(i);
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
}
