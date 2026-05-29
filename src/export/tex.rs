use std::collections::HashMap;
use std::path::Path;

use crate::model::curve::{CurveKind, CurveSet, Group};

const HEADER_LINES: usize = 19;

pub struct TexConfig<'a> {
    pub path: &'a Path,
    pub template_path: &'a Path,
}

pub fn export_tex(
    curves: &[CurveSet],
    groups: &[Group],
    cfg: &TexConfig,
) -> Result<(), String> {
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

    let out = build_tex_output(curves, groups, &header);

    if let Some(parent) = cfg.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
    }
    std::fs::write(cfg.path, out).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

fn build_tex_output(curves: &[CurveSet], groups: &[Group], header: &str) -> String {
    let mut out = String::from(header);

    let tex_param_for: HashMap<u64, &str> = groups
        .iter()
        .map(|g| (g.id, g.tex_param.as_str()))
        .collect();
    let fallback_param = groups.first().map(|g| g.tex_param.as_str()).unwrap_or("A");

    enum PlanEntry {
        Bezier { tex_param: String, idx: usize },
        Ellipse,
    }

    let mut counters: HashMap<u64, usize> = HashMap::new();
    let mut plan: Vec<(usize, PlanEntry)> = Vec::new();
    for (ci, c) in curves.iter().enumerate() {
        if !c.visible {
            continue;
        }
        match c.kind {
            CurveKind::Bezier => {
                if c.n() == 0 {
                    continue;
                }
                let (group_key, tex_param) = match c.group_id {
                    Some(id) => {
                        let t = tex_param_for.get(&id).copied().unwrap_or(fallback_param);
                        (id, t)
                    }
                    None => (0, fallback_param),
                };
                let idx = counters.entry(group_key).or_insert(0);
                plan.push((
                    ci,
                    PlanEntry::Bezier {
                        tex_param: tex_param.to_string(),
                        idx: *idx,
                    },
                ));
                *idx += 1;
            }
            CurveKind::Ellipse => {
                if c.ellipse_rx.abs() < 1e-6 || c.ellipse_ry.abs() < 1e-6 {
                    continue;
                }
                plan.push((ci, PlanEntry::Ellipse));
            }
        }
    }

    for (ci, entry) in &plan {
        match entry {
            PlanEntry::Bezier { tex_param, idx } => {
                emit_bezier_plot(&mut out, tex_param, *idx);
            }
            PlanEntry::Ellipse => emit_ellipse_tex(&mut out, &curves[*ci]),
        }
    }

    for (ci, entry) in &plan {
        if let PlanEntry::Bezier { tex_param, idx } = entry {
            emit_bezier_data(&mut out, &curves[*ci], tex_param, *idx);
        }
    }

    out
}

fn emit_bezier_plot(out: &mut String, tex_param: &str, idx: usize) {
    out.push_str(&bezier_plot_latex(tex_param, idx));
    out.push('\n');
}

fn emit_bezier_data(out: &mut String, c: &CurveSet, tex_param: &str, idx: usize) {
    for line in bezier_data_latex(c, tex_param, idx) {
        out.push_str(&line);
        out.push('\n');
    }
}

pub(crate) fn bezier_plot_latex(tex_param: &str, idx: usize) -> String {
    let (letter, s1_sub, s2_sub, s3_sub) = subscript_parts(tex_param, idx);
    format!(
        "\\left(B_{{x}}\\left({letter}_{{{s1_sub}}},{letter}_{{{s2_sub}}},{letter}_{{{s3_sub}}}\\right),B_{{y}}\\left({letter}_{{{s1_sub}}},{letter}_{{{s2_sub}}},{letter}_{{{s3_sub}}}\\right)\\right)"
    )
}

pub(crate) fn bezier_data_latex(c: &CurveSet, tex_param: &str, idx: usize) -> Vec<String> {
    let (letter, s1_sub, s2_sub, s3_sub) = subscript_parts(tex_param, idx);
    let n = c.n();
    let arrays = [(s1_sub, &c.s1), (s2_sub, &c.s2), (s3_sub, &c.s3)];
    arrays
        .into_iter()
        .map(|(sub, arr)| {
            let pts: Vec<String> = (0..n)
                .map(|i| format!("({},{})", fmt_num(arr[i].x), fmt_num(arr[i].y)))
                .collect();
            format!("{letter}_{{{sub}}}=[{}]", pts.join(","))
        })
        .collect()
}

fn subscript_parts(tex_param: &str, idx: usize) -> (char, String, String, String) {
    // Drop whitespace so a param like "Right Eye" yields R_{ightEye...}, not R_{ight Eye...}
    // (spaces inside a Desmos subscript are not valid variable-name characters).
    let cleaned: String = tex_param.chars().filter(|c| !c.is_whitespace()).collect();
    let mut chars = cleaned.chars();
    let letter = chars.next().unwrap_or('A');
    let rest: String = chars.collect();
    let counter = bijective_base9(idx);
    let prefix = format!("{rest}{counter}");
    (
        letter,
        format!("{prefix}1"),
        format!("{prefix}2"),
        format!("{prefix}3"),
    )
}

fn bijective_base9(n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let mut result = Vec::new();
    let mut x = n;
    while x > 0 {
        let r = (x - 1) % 9;
        result.push((b'1' + r as u8) as char);
        x = (x - 1) / 9;
    }
    result.iter().rev().collect()
}

fn emit_ellipse_tex(out: &mut String, c: &CurveSet) {
    out.push_str(&ellipse_latex(c));
    out.push('\n');
}

pub(crate) fn ellipse_latex(c: &CurveSet) -> String {
    let h = fmt_signed(c.ellipse_cx);
    let k = fmt_signed(c.ellipse_cy);
    let big_a = fmt_signed(c.ellipse_rot_deg.to_radians());
    let a = fmt_num(c.ellipse_rx.abs());
    let b = fmt_num(c.ellipse_ry.abs());

    format!(
        "\\frac{{\\left(\\left(x-{h}\\right)\\cos {big_a}+\\left(y-{k}\\right)\\sin {big_a}\\right)^{{2}}}}{{{a}^{{2}}}}+\\frac{{\\left(\\left(x-{h}\\right)\\sin {big_a}-\\left(y-{k}\\right)\\cos {big_a}\\right)^{{2}}}}{{{b}^{{2}}}}=1"
    )
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
    use crate::model::curve::P;

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
    fn bijective_base9_basic() {
        assert_eq!(bijective_base9(0), "");
        assert_eq!(bijective_base9(1), "1");
        assert_eq!(bijective_base9(9), "9");
        assert_eq!(bijective_base9(10), "11");
        assert_eq!(bijective_base9(18), "19");
        assert_eq!(bijective_base9(19), "21");
        assert_eq!(bijective_base9(90), "99");
        assert_eq!(bijective_base9(91), "111");
    }

    #[test]
    fn subscript_parts_single_letter() {
        let (letter, s1, s2, s3) = subscript_parts("A", 0);
        assert_eq!(letter, 'A');
        assert_eq!((s1.as_str(), s2.as_str(), s3.as_str()), ("1", "2", "3"));

        let (_, s1, s2, s3) = subscript_parts("A", 1);
        assert_eq!((s1.as_str(), s2.as_str(), s3.as_str()), ("11", "12", "13"));

        let (_, s1, s2, s3) = subscript_parts("A", 10);
        assert_eq!((s1.as_str(), s2.as_str(), s3.as_str()), ("111", "112", "113"));
    }

    #[test]
    fn subscript_parts_multi_letter() {
        let (letter, s1, s2, s3) = subscript_parts("Head", 0);
        assert_eq!(letter, 'H');
        assert_eq!(
            (s1.as_str(), s2.as_str(), s3.as_str()),
            ("ead1", "ead2", "ead3")
        );

        let (letter, s1, _, _) = subscript_parts("FolderA", 0);
        assert_eq!(letter, 'F');
        assert_eq!(s1, "olderA1");

        let (_, s1, s2, s3) = subscript_parts("Head", 1);
        assert_eq!(
            (s1.as_str(), s2.as_str(), s3.as_str()),
            ("ead11", "ead12", "ead13")
        );
    }

    #[test]
    fn subscript_parts_strips_whitespace() {
        let (letter, s1, s2, s3) = subscript_parts("Right Eye", 0);
        assert_eq!(letter, 'R');
        assert_eq!(
            (s1.as_str(), s2.as_str(), s3.as_str()),
            ("ightEye1", "ightEye2", "ightEye3")
        );
    }

    #[test]
    fn emit_bezier_plot_writes_plot_line() {
        let mut out = String::new();
        emit_bezier_plot(&mut out, "A", 0);
        assert_eq!(
            out.trim(),
            "\\left(B_{x}\\left(A_{1},A_{2},A_{3}\\right),B_{y}\\left(A_{1},A_{2},A_{3}\\right)\\right)"
        );
    }

    #[test]
    fn emit_bezier_data_writes_data_lines() {
        let mut c = CurveSet::empty("c", [0, 0, 0]);
        c.s1.push(P::new(0.0, 0.0));
        c.s2.push(P::new(1.0, 1.0));
        c.s3.push(P::new(2.0, 0.0));
        let mut out = String::new();
        emit_bezier_data(&mut out, &c, "A", 0);
        assert!(out.contains("A_{1}=[(0,0)]"));
        assert!(out.contains("A_{2}=[(1,1)]"));
        assert!(out.contains("A_{3}=[(2,0)]"));
    }

    fn make_bezier(name: &str, group_id: u64, points: &[(f32, f32, f32, f32, f32, f32)]) -> CurveSet {
        let mut c = CurveSet::empty(name, [0, 0, 0]);
        c.group_id = Some(group_id);
        for &(x1, y1, x2, y2, x3, y3) in points {
            c.s1.push(P::new(x1, y1));
            c.s2.push(P::new(x2, y2));
            c.s3.push(P::new(x3, y3));
        }
        c
    }

    #[test]
    fn build_tex_output_groups_all_plots_before_data() {
        let groups = vec![Group::new(1, "Hand", "A")];
        let curves = vec![
            make_bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]),
            make_bezier("b", 1, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]),
        ];

        let out = build_tex_output(&curves, &groups, "");
        let plot_a1 = out.find("B_{x}\\left(A_{1}").expect("A_1 plot");
        let plot_a11 = out.find("B_{x}\\left(A_{11}").expect("A_11 plot");
        let data_a1 = out.find("A_{1}=").expect("A_1 data");
        let data_a11 = out.find("A_{11}=").expect("A_11 data");

        assert!(plot_a1 < plot_a11, "plots should keep curve order");
        assert!(plot_a11 < data_a1, "every plot should appear before any data");
        assert!(data_a1 < data_a11, "data should keep curve order");
    }

    #[test]
    fn build_tex_output_separate_groups_use_independent_counters() {
        let groups = vec![
            Group::new(1, "Hand", "A"),
            Group::new(2, "Head", "H"),
        ];
        let curves = vec![
            make_bezier("hand1", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]),
            make_bezier("head1", 2, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]),
            make_bezier("hand2", 1, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]),
        ];

        let out = build_tex_output(&curves, &groups, "");
        assert!(out.contains("A_{1}="), "hand1 should be A_1");
        assert!(out.contains("H_{1}="), "head1 should be H_1");
        assert!(out.contains("A_{11}="), "hand2 should be A_11 (next in Hand group)");
    }

    #[test]
    fn build_tex_output_skips_invisible_and_empty() {
        let groups = vec![Group::new(1, "G", "A")];
        let mut visible = make_bezier("visible", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        visible.visible = true;
        let mut hidden = make_bezier("hidden", 1, &[(9.0, 9.0, 8.0, 8.0, 7.0, 7.0)]);
        hidden.visible = false;
        let empty = {
            let mut c = CurveSet::empty("empty", [0, 0, 0]);
            c.group_id = Some(1);
            c
        };

        let out = build_tex_output(&[visible, hidden, empty], &groups, "");
        assert!(out.contains("A_{1}=[(0,0)]"));
        assert!(!out.contains("9"));
        assert!(!out.contains("A_{11}"));
    }

    #[test]
    fn build_tex_output_multi_char_tex_param() {
        let groups = vec![Group::new(1, "Head", "Head")];
        let curves = vec![make_bezier("c", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];
        let out = build_tex_output(&curves, &groups, "");
        assert!(
            out.contains("H_{ead1},H_{ead2},H_{ead3}"),
            "multi-char tex_param should split first char as variable, rest as subscript prefix:\n{out}"
        );
        assert!(out.contains("H_{ead1}=[(0,0)]"));
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
