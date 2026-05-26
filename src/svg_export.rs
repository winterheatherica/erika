use std::path::Path;

use crate::curve::{CurveKind, CurveSet, LineStyle, P};

pub struct SvgConfig<'a> {
    pub width: u32,
    pub height: u32,
    pub background: Option<[u8; 3]>,
    pub padding_fraction: f32,
    pub path: &'a Path,
}

pub fn export_svg(curves: &[CurveSet], cfg: &SvgConfig) -> Result<(), String> {
    let bounds = compute_bounds(curves, 32);
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

    let w2px = |p: P| -> (f32, f32) { (offset_x + p.x * s, h - (offset_y + p.y * s)) };

    let mut svg = String::new();
    svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        cfg.width, cfg.height, cfg.width, cfg.height
    ));
    if let Some(bg) = cfg.background {
        svg.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{}\"/>\n",
            rgb_hex(bg)
        ));
    }

    for c in curves {
        if !c.visible {
            continue;
        }
        match c.kind {
            CurveKind::Bezier => emit_bezier(&mut svg, c, &w2px, s),
            CurveKind::Ellipse => emit_ellipse(&mut svg, c, &w2px, s),
        }
    }

    svg.push_str("</svg>\n");
    std::fs::write(cfg.path, svg).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

fn emit_bezier<F: Fn(P) -> (f32, f32)>(svg: &mut String, c: &CurveSet, w2px: &F, scale: f32) {
    let n = c.n();
    if n == 0 {
        return;
    }
    let mut d = String::new();
    let (x0, y0) = w2px(c.s1[0]);
    d.push_str(&format!("M {:.3} {:.3}", x0, y0));
    for i in 0..n {
        let (cx, cy) = w2px(c.s2[i]);
        let (ex, ey) = w2px(c.s3[i]);
        d.push_str(&format!(" Q {:.3} {:.3} {:.3} {:.3}", cx, cy, ex, ey));
    }
    if c.fill_enabled {
        d.push_str(" Z");
    }

    let stroke_attr = if c.stroke_visible {
        format!(
            "stroke=\"{}\" stroke-width=\"{:.3}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{}",
            rgb_hex(c.color),
            c.thickness,
            dash_attr(c.line_style, c.thickness),
        )
    } else {
        "stroke=\"none\"".into()
    };
    let fill_attr = if c.fill_enabled {
        format!(
            "fill=\"{}\" fill-opacity=\"{:.3}\" fill-rule=\"evenodd\"",
            rgb_hex([c.fill_color[0], c.fill_color[1], c.fill_color[2]]),
            c.fill_color[3] as f32 / 255.0,
        )
    } else {
        "fill=\"none\"".into()
    };

    svg.push_str(&format!(
        "  <path d=\"{}\" {} {}/>\n",
        d, stroke_attr, fill_attr
    ));
    let _ = scale;
}

fn emit_ellipse<F: Fn(P) -> (f32, f32)>(svg: &mut String, c: &CurveSet, w2px: &F, scale: f32) {
    let (cx_px, cy_px) = w2px(P::new(c.ellipse_cx, c.ellipse_cy));
    let rx_px = c.ellipse_rx * scale;
    let ry_px = c.ellipse_ry * scale;
    let rot = -c.ellipse_rot_deg;

    let stroke_attr = if c.stroke_visible {
        format!(
            "stroke=\"{}\" stroke-width=\"{:.3}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{}",
            rgb_hex(c.color),
            c.thickness,
            dash_attr(c.line_style, c.thickness),
        )
    } else {
        "stroke=\"none\"".into()
    };
    let fill_attr = if c.fill_enabled {
        format!(
            "fill=\"{}\" fill-opacity=\"{:.3}\"",
            rgb_hex([c.fill_color[0], c.fill_color[1], c.fill_color[2]]),
            c.fill_color[3] as f32 / 255.0,
        )
    } else {
        "fill=\"none\"".into()
    };

    svg.push_str(&format!(
        "  <ellipse cx=\"{:.3}\" cy=\"{:.3}\" rx=\"{:.3}\" ry=\"{:.3}\" transform=\"rotate({:.3} {:.3} {:.3})\" {} {}/>\n",
        cx_px, cy_px, rx_px, ry_px, rot, cx_px, cy_px, stroke_attr, fill_attr
    ));
}

fn rgb_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn dash_attr(style: LineStyle, thickness: f32) -> String {
    let pattern = style.pattern();
    if pattern.is_empty() {
        return String::new();
    }
    let scale = thickness.max(0.5);
    let scaled: Vec<String> = pattern.iter().map(|v| format!("{:.3}", v * scale)).collect();
    format!(" stroke-dasharray=\"{}\"", scaled.join(","))
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
