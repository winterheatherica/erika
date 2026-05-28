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

    let mut out = String::new();
    for line in template.lines().take(HEADER_LINES) {
        out.push_str(line);
        out.push('\n');
    }

    let mut bezier_idx: usize = 0;
    for c in curves {
        if !c.visible {
            continue;
        }
        match c.kind {
            CurveKind::Bezier => {
                if c.n() == 0 {
                    continue;
                }
                emit_bezier_tex(&mut out, c, bezier_idx);
                bezier_idx += 1;
            }
            CurveKind::Ellipse => {
                if c.ellipse_rx.abs() < 1e-6 || c.ellipse_ry.abs() < 1e-6 {
                    continue;
                }
                emit_ellipse_tex(&mut out, c);
            }
        }
    }

    if let Some(parent) = cfg.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
    }
    std::fs::write(cfg.path, out).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
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

fn emit_bezier_tex(out: &mut String, c: &CurveSet, idx: usize) {
    let letter = bezier_letter(idx);
    let s1_sub = bezier_subscript(idx, 1);
    let s2_sub = bezier_subscript(idx, 2);
    let s3_sub = bezier_subscript(idx, 3);

    out.push_str(&format!(
        "\\left(B_{{x}}\\left({letter}_{{{s1_sub}}},{letter}_{{{s2_sub}}},{letter}_{{{s3_sub}}}\\right),B_{{y}}\\left({letter}_{{{s1_sub}}},{letter}_{{{s2_sub}}},{letter}_{{{s3_sub}}}\\right)\\right)\n"
    ));

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
    fn emit_bezier_writes_expected_lines() {
        let mut c = CurveSet::empty("c", [0, 0, 0]);
        c.s1.push(P::new(0.0, 0.0));
        c.s2.push(P::new(1.0, 1.0));
        c.s3.push(P::new(2.0, 0.0));
        let mut out = String::new();
        emit_bezier_tex(&mut out, &c, 0);
        assert!(out.contains("B_{x}\\left(S_{1},S_{2},S_{3}\\right)"));
        assert!(out.contains("S_{1}=[(0,0)]"));
        assert!(out.contains("S_{2}=[(1,1)]"));
        assert!(out.contains("S_{3}=[(2,0)]"));
    }

    #[test]
    fn multiple_bezier_curves_use_different_letters() {
        let mut a = CurveSet::empty("a", [0, 0, 0]);
        a.s1.push(P::new(0.0, 0.0));
        a.s2.push(P::new(1.0, 1.0));
        a.s3.push(P::new(2.0, 0.0));
        let mut b = CurveSet::empty("b", [0, 0, 0]);
        b.s1.push(P::new(3.0, 3.0));
        b.s2.push(P::new(4.0, 4.0));
        b.s3.push(P::new(5.0, 3.0));

        let mut out = String::new();
        emit_bezier_tex(&mut out, &a, 0);
        emit_bezier_tex(&mut out, &b, 1);

        assert!(out.contains("S_{1}=[(0,0)]"));
        assert!(out.contains("T_{1}=[(3,3)]"));
        assert!(out.contains("T_{3}=[(5,3)]"));
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
