use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::{App, HandleId};
use crate::curve::{Arr, CurveKind, CurveSet, P};

impl App {
    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        match self.background {
            Some(bg) => {
                painter.rect_filled(rect, 0.0, Color32::from_rgb(bg[0], bg[1], bg[2]));
            }
            None => draw_transparency_checker(&painter, rect),
        }

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

        let progress = self.playback_progress;
        if progress >= 1.0 {
            for c in &self.curves {
                if !c.visible {
                    continue;
                }
                self.draw_curve(&painter, rect, c);
            }
        } else {
            let mut units: Vec<(usize, u64)> = Vec::new();
            for (ci, c) in self.curves.iter().enumerate() {
                if !c.visible {
                    continue;
                }
                let count = c.draw_units();
                for ui in 0..count {
                    let ts = c.created_at.get(ui).copied().unwrap_or(u64::MAX);
                    units.push((ci, ts));
                }
            }
            units.sort_unstable_by_key(|&(_, ts)| ts);

            let total = units.len() as f32;
            let units_at = progress * total;
            let full_count = units_at.floor() as usize;
            let partial_frac = units_at - full_count as f32;

            let mut per_curve_full = vec![0_usize; self.curves.len()];
            let mut per_curve_partial = vec![0.0_f32; self.curves.len()];
            for (i, &(ci, _)) in units.iter().enumerate() {
                if i < full_count {
                    per_curve_full[ci] += 1;
                } else if i == full_count {
                    if partial_frac > 0.0 {
                        per_curve_partial[ci] = partial_frac;
                    }
                    break;
                } else {
                    break;
                }
            }

            for (ci, c) in self.curves.iter().enumerate() {
                if !c.visible {
                    continue;
                }
                let full = per_curve_full[ci];
                let partial = per_curve_partial[ci];
                let amount = full as f32 + partial;
                if amount <= 0.0 {
                    continue;
                }
                if full >= c.draw_units() {
                    self.draw_curve(&painter, rect, c);
                } else {
                    self.draw_curve_partial(&painter, rect, c, amount);
                }
            }
        }

        if self.show_handles_all
            && !self.edit_hide_handles
            && self.playback_progress >= 1.0
        {
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
        let in_playback = self.playback_progress < 1.0;
        let (eff_stroke, eff_fill) = if in_playback {
            (
                self.render_mode.effective_stroke(c.stroke_visible),
                self.render_mode.effective_fill(c.fill_enabled),
            )
        } else {
            (
                c.stroke_visible || self.edit_show_all_strokes,
                (c.fill_enabled || self.edit_show_all_fills) && !self.edit_hide_all_fills,
            )
        };
        let show_cp = c.show_control_poly
            && (in_playback || !self.edit_hide_control_polygon);

        let stroke_world = c.sampled_path(stroke_samples);
        let stroke_pts: Vec<Pos2> = stroke_world.iter().map(|p| self.w2s(rect, *p)).collect();

        if eff_fill {
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

        if eff_stroke && stroke_pts.len() >= 2 {
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

        if c.is_bezier() && show_cp {
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

    fn draw_curve_partial(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        c: &CurveSet,
        partial: f32,
    ) {
        let stroke_samples = self.samples_per_segment.max(4);
        let eff_stroke = self.render_mode.effective_stroke(c.stroke_visible);
        let eff_fill = self.render_mode.effective_fill(c.fill_enabled);
        let stroke_world = c.sampled_path_partial(stroke_samples, partial);
        let stroke_pts: Vec<Pos2> = stroke_world.iter().map(|p| self.w2s(rect, *p)).collect();

        if eff_fill {
            let fill_samples = stroke_samples.max(64);
            let fill_world = if fill_samples == stroke_samples {
                stroke_world.clone()
            } else {
                c.sampled_path_partial(fill_samples, partial)
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

        if eff_stroke && stroke_pts.len() >= 2 {
            let color = Color32::from_rgb(c.color[0], c.color[1], c.color[2]);
            let stroke = Stroke::new(c.thickness, color);
            painter.add(egui::Shape::line(stroke_pts, stroke));
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
                            Arr::S1 | Arr::S3 => 7.5,
                            Arr::S2 => 6.5,
                        };
                        let r = if is_active { base_r } else { base_r * 0.7 };
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
                    if is_sel { 8.5 } else { 6.0 },
                    color,
                    Stroke::new(if is_center_drag { 2.5 } else { 1.5 }, color),
                );
                painter.circle(
                    sp_rx,
                    if is_sel { 7.5 } else { 5.5 },
                    white,
                    Stroke::new(if is_rx_drag { 2.5 } else { 1.5 }, color),
                );
                painter.circle(
                    sp_ry,
                    if is_sel { 7.5 } else { 5.5 },
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

fn draw_transparency_checker(painter: &egui::Painter, rect: Rect) {
    let cell = (rect.width().max(rect.height()) / 48.0).clamp(10.0, 40.0);
    painter.rect_filled(rect, 0.0, Color32::from_gray(250));
    let dark = Color32::from_gray(220);
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for r in 0..rows {
        for c in 0..cols {
            if (r + c) % 2 == 0 {
                continue;
            }
            let min = Pos2::new(rect.left() + c as f32 * cell, rect.top() + r as f32 * cell);
            let cell_rect = Rect::from_min_size(min, Vec2::new(cell, cell)).intersect(rect);
            painter.rect_filled(cell_rect, 0.0, dark);
        }
    }
}
