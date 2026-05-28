use std::path::Path;

use crate::curve::{CurveKind, CurveSet};

const BEZIER_LETTERS: &[&str] = &[
    "S", "T", "U", "V", "W", "Y", "Z", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N",
    "O", "Q", "R",
];

const HEADER_LINES: usize = 19;

pub struct TexConfig<'a> {
    pub path: &'a Path,
    pub template_path: &'a Path,
}

pub fn export_tex(curves: &[CurveSet], cfg: &TexConfig) -> Result<(), String> {
    let template = std::fs::read_to_string(cfg.template_path).map_err(|e| {
        format!(
            "Could not read template '{}': {e}",
            cfg.template_path.display()
        )
    })?;

    let mut header = String::new();
    for line in template.lines().take(HEADER_LINES) {
        header.push_str(line);
        header.push('\n');
    }

    let out = build_tex_output(curves, &header);

    if let Some(parent) = cfg.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
    }
    std::fs::write(cfg.path, out).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

fn build_tex_output(curves: &[CurveSet], header: &str) -> String {
    let mut out = String::from(header);

    let mut plan: Vec<(usize, Option<usize>)> = Vec::new();
    let mut bezier_idx: usize = 0;
    for (ci, c) in curves.iter().enumerate() {
        if !c.visible {
            continue;
        }
        match c.kind {
            CurveKind::Bezier => {
                if c.n() == 0 {
                    continue;
                }
                plan.push((ci, Some(bezier_idx)));
                bezier_idx += 1;
            }
            CurveKind::Ellipse => {
                if c.ellipse_rx.abs() < 1e-6 || c.ellipse_ry.abs() < 1e-6 {
                    continue;
                }
                plan.push((ci, None));
            }
        }
    }

    for &(ci, bidx_opt) in &plan {
        match bidx_opt {
            Some(bidx) => emit_bezier_plot(&mut out, bidx),
            None => emit_ellipse_tex(&mut out, &curves[ci]),
        }
    }

    for &(ci, bidx_opt) in &plan {
        if let Some(bidx) = bidx_opt {
            emit_bezier_data(&mut out, &curves[ci], bidx);
        }
    }

    out
}

fn bezier_letter(idx: usize) -> &'static str {
    BEZIER_LETTERS[idx % BEZIER_LETTERS.len()]
}

fn bezier_subscript(idx: usize, sub: usize) -> String {
    let cycle = idx / BEZIER_LETTERS.len();
    if cycle == 0 {
        format!("{sub}")
    } else {
        format!("{sub},{}", cycle + 1)
    }
}

fn emit_bezier_plot(out: &mut String, idx: usize) {
    let letter = bezier_letter(idx);
    let s1_sub = bezier_subscript(idx, 1);
    let s2_sub = bezier_subscript(idx, 2);
    let s3_sub = bezier_subscript(idx, 3);

    out.push_str(&format!(
        "\\left(B_{{x}}\\left({letter}_{{{s1_sub}}},{letter}_{{{s2_sub}}},{letter}_{{{s3_sub}}}\\right),B_{{y}}\\left({letter}_{{{s1_sub}}},{letter}_{{{s2_sub}}},{letter}_{{{s3_sub}}}\\right)\\right)\n"
    ));
}

fn emit_bezier_data(out: &mut String, c: &CurveSet, idx: usize) {
    let letter = bezier_letter(idx);
    let s1_sub = bezier_subscript(idx, 1);
    let s2_sub = bezier_subscript(idx, 2);
    let s3_sub = bezier_subscript(idx, 3);

    let n = c.n();
    let arrays = [(&s1_sub, &c.s1), (&s2_sub, &c.s2), (&s3_sub, &c.s3)];
    for (sub, arr) in arrays {
        let pts: Vec<String> = (0..n)
            .map(|i| format!("({},{})", fmt_num(arr[i].x), fmt_num(arr[i].y)))
            .collect();
        out.push_str(&format!("{letter}_{{{sub}}}=[{}]\n", pts.join(",")));
    }
}

fn emit_ellipse_tex(out: &mut String, c: &CurveSet) {
    let h = fmt_signed(c.ellipse_cx);
    let k = fmt_signed(c.ellipse_cy);
    let big_a = fmt_signed(c.ellipse_rot_deg.to_radians());
    let a = fmt_num(c.ellipse_rx.abs());
    let b = fmt_num(c.ellipse_ry.abs());

    out.push_str(&format!(
        "\\frac{{\\left(\\left(x-{h}\\right)\\cos {big_a}+\\left(y-{k}\\right)\\sin {big_a}\\right)^{{2}}}}{{{a}^{{2}}}}+\\frac{{\\left(\\left(x-{h}\\right)\\sin {big_a}-\\left(y-{k}\\right)\\cos {big_a}\\right)^{{2}}}}{{{b}^{{2}}}}=1\n"
    ));
}

fn fmt_num(v: f32) -> String {
    if !v.is_finite() || v == 0.0 {
        return "0".to_string();
    }
    let s = format!("{v:.6}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn fmt_signed(v: f32) -> String {
    if v < 0.0 {
        format!("\\left({}\\right)", fmt_num(v))
    } else {
        fmt_num(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::P;

    #[test]
    fn fmt_num_trims_zeros() {
        assert_eq!(fmt_num(1.5), "1.5");
        assert_eq!(fmt_num(2.0), "2");
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(-3.25), "-3.25");
    }

    #[test]
    fn fmt_signed_wraps_negative() {
        assert_eq!(fmt_signed(2.5), "2.5");
        assert_eq!(fmt_signed(-1.0), "\\left(-1\\right)");
    }

    #[test]
    fn bezier_naming_first_cycle() {
        assert_eq!(bezier_letter(0), "S");
        assert_eq!(bezier_letter(1), "T");
        assert_eq!(bezier_subscript(0, 1), "1");
        assert_eq!(bezier_subscript(1, 2), "2");
    }

    #[test]
    fn bezier_naming_second_cycle_uses_double_subscript() {
        let n = BEZIER_LETTERS.len();
        assert_eq!(bezier_letter(n), "S");
        assert_eq!(bezier_subscript(n, 1), "1,2");
        assert_eq!(bezier_subscript(n + 1, 3), "3,2");
    }

    #[test]
    fn emit_bezier_plot_writes_plot_line() {
        let mut out = String::new();
        emit_bezier_plot(&mut out, 0);
        assert_eq!(
            out.trim(),
            "\\left(B_{x}\\left(S_{1},S_{2},S_{3}\\right),B_{y}\\left(S_{1},S_{2},S_{3}\\right)\\right)"
        );
    }

    #[test]
    fn emit_bezier_data_writes_data_lines() {
        let mut c = CurveSet::empty("c", [0, 0, 0]);
        c.s1.push(P::new(0.0, 0.0));
        c.s2.push(P::new(1.0, 1.0));
        c.s3.push(P::new(2.0, 0.0));
        let mut out = String::new();
        emit_bezier_data(&mut out, &c, 0);
        assert!(out.contains("S_{1}=[(0,0)]"));
        assert!(out.contains("S_{2}=[(1,1)]"));
        assert!(out.contains("S_{3}=[(2,0)]"));
    }

    #[test]
    fn build_tex_output_groups_all_plots_before_data() {
        let mut a = CurveSet::empty("a", [0, 0, 0]);
        a.s1.push(P::new(0.0, 0.0));
        a.s2.push(P::new(1.0, 1.0));
        a.s3.push(P::new(2.0, 0.0));
        let mut b = CurveSet::empty("b", [0, 0, 0]);
        b.s1.push(P::new(3.0, 3.0));
        b.s2.push(P::new(4.0, 4.0));
        b.s3.push(P::new(5.0, 3.0));
        let curves = vec![a, b];

        let out = build_tex_output(&curves, "");
        let plot_s = out.find("B_{x}\\left(S_").expect("S plot");
        let plot_t = out.find("B_{x}\\left(T_").expect("T plot");
        let data_s = out.find("S_{1}=").expect("S data");
        let data_t = out.find("T_{1}=").expect("T data");

        assert!(plot_s < plot_t, "plots should keep curve order");
        assert!(plot_t < data_s, "every plot should appear before any data");
        assert!(data_s < data_t, "data should keep curve order");
    }

    #[test]
    fn build_tex_output_skips_invisible_and_empty() {
        let mut visible = CurveSet::empty("visible", [0, 0, 0]);
        visible.s1.push(P::new(0.0, 0.0));
        visible.s2.push(P::new(1.0, 1.0));
        visible.s3.push(P::new(2.0, 0.0));

        let mut hidden = CurveSet::empty("hidden", [0, 0, 0]);
        hidden.s1.push(P::new(9.0, 9.0));
        hidden.s2.push(P::new(8.0, 8.0));
        hidden.s3.push(P::new(7.0, 7.0));
        hidden.visible = false;

        let empty = CurveSet::empty("empty", [0, 0, 0]);

        let out = build_tex_output(&[visible, hidden, empty], "");
        assert!(out.contains("S_{1}"));
        assert!(!out.contains("9"));
        assert!(!out.contains("T_"));
    }

    #[test]
    fn emit_ellipse_inlines_values() {
        let mut c = CurveSet::new_ellipse("e", [0, 0, 0], 2.5, 1.0);
        c.ellipse_rx = 1.5;
        c.ellipse_ry = 0.8;
        c.ellipse_rot_deg = 0.0;
        let mut out = String::new();
        emit_ellipse_tex(&mut out, &c);
        assert!(out.contains("x-2.5"));
        assert!(out.contains("y-1"));
        assert!(out.contains("1.5^{2}"));
        assert!(out.contains("0.8^{2}"));
        assert!(out.contains("=1"));
    }
}
