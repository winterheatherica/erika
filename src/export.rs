use std::path::Path;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, StrokeDash, Transform,
};

use crate::curve::{CurveKind, CurveSet, LineStyle, P};

pub struct ExportConfig<'a> {
    pub width: u32,
    pub height: u32,
    pub samples_per_segment: usize,
    pub background: Option<[u8; 3]>,
    pub padding_fraction: f32,
    pub path: &'a Path,
}

pub fn export_png(curves: &[CurveSet], cfg: &ExportConfig) -> Result<(), String> {
    let mut pixmap = Pixmap::new(cfg.width, cfg.height)
        .ok_or_else(|| format!("Failed to allocate {}x{} pixmap", cfg.width, cfg.height))?;
    if let Some(bg) = cfg.background {
        pixmap.fill(Color::from_rgba8(bg[0], bg[1], bg[2], 255));
    }

    let bounds = compute_bounds(curves, cfg.samples_per_segment.max(8));
    let Some((mut min_x, mut min_y, mut max_x, mut max_y)) = bounds else {
        return Err("No visible curves with content to export".into());
    };
    let span = (max_x - min_x).max(max_y - min_y).max(1e-6);
    let pad = span * cfg.padding_fraction.max(0.0);
    min_x -= pad;
    min_y -= pad;
    max_x += pad;
    max_y += pad;

    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    let sx = cfg.width as f32 / span_x;
    let sy = cfg.height as f32 / span_y;
    let s = sx.min(sy);
    let offset_x = (cfg.width as f32 - span_x * s) * 0.5 - min_x * s;
    let offset_y = (cfg.height as f32 - span_y * s) * 0.5 - min_y * s;
    let h = cfg.height as f32;
    let world_to_px = |p: P| -> (f32, f32) { (offset_x + p.x * s, h - (offset_y + p.y * s)) };

    for c in curves {
        if !c.visible {
            continue;
        }
        match c.kind {
            CurveKind::Bezier => emit_bezier(&mut pixmap, c, &world_to_px),
            CurveKind::Ellipse => emit_ellipse(&mut pixmap, c, &world_to_px, s),
        }
    }

    pixmap
        .save_png(cfg.path)
        .map_err(|e| format!("Save error: {e}"))?;
    Ok(())
}

fn emit_bezier<F: Fn(P) -> (f32, f32)>(
    pixmap: &mut Pixmap,
    c: &CurveSet,
    world_to_px: &F,
) {
    let n = c.n();
    if n == 0 {
        return;
    }

    let build_path = |closed: bool| {
        let mut pb = PathBuilder::new();
        let (x0, y0) = world_to_px(c.s1[0]);
        pb.move_to(x0, y0);
        for i in 0..n {
            let (cx, cy) = world_to_px(c.s2[i]);
            let (ex, ey) = world_to_px(c.s3[i]);
            pb.quad_to(cx, cy, ex, ey);
        }
        if closed {
            pb.close();
        }
        pb.finish()
    };

    if c.fill_enabled {
        if let Some(path) = build_path(true) {
            let mut fill_paint = Paint::default();
            fill_paint.set_color_rgba8(
                c.fill_color[0],
                c.fill_color[1],
                c.fill_color[2],
                c.fill_color[3],
            );
            fill_paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &fill_paint,
                FillRule::EvenOdd,
                Transform::identity(),
                None,
            );
        }
    }
    if c.stroke_visible {
        if let Some(path) = build_path(false) {
            stroke_path(pixmap, &path, c);
        }
    }
    if c.show_control_poly {
        for i in 0..n {
            let mut pb = PathBuilder::new();
            let (a, b) = world_to_px(c.s1[i]);
            pb.move_to(a, b);
            let (a, b) = world_to_px(c.s2[i]);
            pb.line_to(a, b);
            let (a, b) = world_to_px(c.s3[i]);
            pb.line_to(a, b);
            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color_rgba8(c.color[0], c.color[1], c.color[2], 120);
                paint.anti_alias = true;
                let stroke = Stroke {
                    width: c.control_poly_thickness,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
    }
}

fn emit_ellipse<F: Fn(P) -> (f32, f32)>(
    pixmap: &mut Pixmap,
    c: &CurveSet,
    world_to_px: &F,
    _scale: f32,
) {
    let n = 256;
    let mut pb = PathBuilder::new();
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let p = c.eval_ellipse(t);
        let (x, y) = world_to_px(p);
        if k == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.close();
    let Some(path) = pb.finish() else { return };

    if c.fill_enabled {
        let mut fill_paint = Paint::default();
        fill_paint.set_color_rgba8(
            c.fill_color[0],
            c.fill_color[1],
            c.fill_color[2],
            c.fill_color[3],
        );
        fill_paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &fill_paint,
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
    }
    if c.stroke_visible {
        stroke_path(pixmap, &path, c);
    }
}

fn stroke_path(pixmap: &mut Pixmap, path: &tiny_skia::Path, c: &CurveSet) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(c.color[0], c.color[1], c.color[2], 255);
    paint.anti_alias = true;
    let dash_arr: Option<Vec<f32>> = match c.line_style {
        LineStyle::Solid => None,
        other => Some(scaled_pattern(other, c.thickness)),
    };
    let stroke = Stroke {
        width: c.thickness,
        miter_limit: 4.0,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        dash: dash_arr
            .as_deref()
            .and_then(|arr| StrokeDash::new(arr.to_vec(), 0.0)),
    };
    pixmap.stroke_path(path, &paint, &stroke, Transform::identity(), None);
}

fn scaled_pattern(style: LineStyle, thickness: f32) -> Vec<f32> {
    let base = style.pattern();
    let scale = thickness.max(0.5);
    base.iter().map(|v| v * scale).collect()
}

fn compute_bounds(curves: &[CurveSet], samples: usize) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut any = false;
    for c in curves {
        if !c.visible {
            continue;
        }
        for p in c.sampled_path(samples) {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
            any = true;
        }
    }
    if any {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}
