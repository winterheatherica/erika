use std::path::Path;

use crate::curve::{CurveKind, CurveSet, Group};
use crate::tex_export::{bezier_data_latex, bezier_plot_latex, ellipse_latex};

pub struct JsConfig<'a> {
    pub path: &'a Path,
    pub template_path: &'a Path,
}

const PLOT_DOMAIN_MAX: usize = 99;

pub fn export_js(curves: &[CurveSet], groups: &[Group], cfg: &JsConfig) -> Result<(), String> {
    let template = std::fs::read_to_string(cfg.template_path).map_err(|e| {
        format!(
            "Could not read template '{}': {e}",
            cfg.template_path.display()
        )
    })?;
    let template_lines: Vec<String> = template
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    let body = build_js_output(curves, groups, &template_lines);

    if let Some(parent) = cfg.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
    }
    std::fs::write(cfg.path, body).map_err(|e| format!("Write failed: {e}"))?;
    Ok(())
}

enum Item {
    Folder {
        id: String,
        title: String,
    },
    Expr {
        id: String,
        folder_id: String,
        color: String,
        latex: String,
        parametric_max: Option<usize>,
        hidden: bool,
        fill_opacity: Option<f32>,
    },
}

fn build_js_output(curves: &[CurveSet], groups: &[Group], template_lines: &[String]) -> String {
    let mut items: Vec<Item> = Vec::new();
    let mut next_id: u32 = 2;

    let template_folder = fresh_id(&mut next_id);
    items.push(Item::Folder {
        id: template_folder.clone(),
        title: "Template".to_string(),
    });
    for line in template_lines.iter() {
        items.push(Item::Expr {
            id: fresh_id(&mut next_id),
            folder_id: template_folder.clone(),
            color: "#000000".to_string(),
            latex: line.clone(),
            parametric_max: None,
            hidden: is_plottable_t_helper(line),
            fill_opacity: None,
        });
    }

    for (gi, g) in groups.iter().enumerate() {
        let members: Vec<usize> = curves
            .iter()
            .enumerate()
            .filter(|(_, c)| group_index_of(c, groups) == gi && is_exportable(c))
            .map(|(i, _)| i)
            .collect();
        if members.is_empty() {
            continue;
        }

        let folder_id = fresh_id(&mut next_id);
        items.push(Item::Folder {
            id: folder_id.clone(),
            title: g.name.clone(),
        });

        let mut bez_idx = 0usize;
        let idx_for: Vec<Option<usize>> = members
            .iter()
            .map(|&ci| {
                if curves[ci].is_bezier() {
                    let v = bez_idx;
                    bez_idx += 1;
                    Some(v)
                } else {
                    None
                }
            })
            .collect();

        for (k, &ci) in members.iter().enumerate() {
            let c = &curves[ci];
            let color = hex(c.color);
            match c.kind {
                CurveKind::Bezier => items.push(Item::Expr {
                    id: fresh_id(&mut next_id),
                    folder_id: folder_id.clone(),
                    color,
                    latex: bezier_plot_latex(&g.tex_param, idx_for[k].unwrap()),
                    parametric_max: Some(PLOT_DOMAIN_MAX),
                    hidden: false,
                    fill_opacity: fill_opacity_for(c),
                }),
                CurveKind::Ellipse => items.push(Item::Expr {
                    id: fresh_id(&mut next_id),
                    folder_id: folder_id.clone(),
                    color,
                    latex: ellipse_latex(c),
                    parametric_max: None,
                    hidden: false,
                    fill_opacity: fill_opacity_for(c),
                }),
            }
        }

        for (k, &ci) in members.iter().enumerate() {
            let Some(idx) = idx_for[k] else { continue };
            let c = &curves[ci];
            let color = hex(c.color);
            for latex in bezier_data_latex(c, &g.tex_param, idx) {
                items.push(Item::Expr {
                    id: fresh_id(&mut next_id),
                    folder_id: folder_id.clone(),
                    color: color.clone(),
                    latex,
                    parametric_max: None,
                    hidden: true,
                    fill_opacity: None,
                });
            }
        }
    }

    render_js(&items)
}

fn fresh_id(next: &mut u32) -> String {
    let v = next.to_string();
    *next += 1;
    v
}

fn group_index_of(c: &CurveSet, groups: &[Group]) -> usize {
    match c.group_id {
        Some(id) => groups.iter().position(|g| g.id == id).unwrap_or(0),
        None => 0,
    }
}

fn is_exportable(c: &CurveSet) -> bool {
    match c.kind {
        CurveKind::Bezier => c.n() > 0,
        CurveKind::Ellipse => c.ellipse_rx.abs() >= 1e-6 && c.ellipse_ry.abs() >= 1e-6,
    }
}

fn is_plottable_t_helper(latex: &str) -> bool {
    match latex.split_once('=') {
        Some((lhs, _)) => lhs.trim_end().ends_with("\\left(t\\right)"),
        None => false,
    }
}

fn fill_opacity_for(c: &CurveSet) -> Option<f32> {
    if c.fill_enabled {
        Some(c.fill_color[3] as f32 / 255.0)
    } else {
        None
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn js_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(ch),
        }
    }
    out
}

fn render_js(items: &[Item]) -> String {
    let lines: Vec<String> = items
        .iter()
        .map(|item| match item {
            Item::Folder { id, title } => format!(
                "  {{ \"type\": \"folder\", \"id\": \"{}\", \"title\": \"{}\" }}",
                id,
                js_escape(title)
            ),
            Item::Expr {
                id,
                folder_id,
                color,
                latex,
                parametric_max,
                hidden,
                fill_opacity,
            } => {
                let hidden_attr = if *hidden { ", \"hidden\": true" } else { "" };
                let domain = match parametric_max {
                    Some(max) => format!(
                        ", \"parametricDomain\": {{ \"min\": \"0\", \"max\": \"{max}\" }}"
                    ),
                    None => String::new(),
                };
                let fill_attr = match fill_opacity {
                    Some(op) => format!(", \"fill\": true, \"fillOpacity\": \"{:.3}\"", op),
                    None => String::new(),
                };
                format!(
                    "  {{ \"type\": \"expression\", \"id\": \"{}\", \"folderId\": \"{}\", \"color\": \"{}\"{}, \"latex\": \"{}\"{}{} }}",
                    id,
                    folder_id,
                    color,
                    hidden_attr,
                    js_escape(latex),
                    domain,
                    fill_attr
                )
            }
        })
        .collect();

    let mut out = String::new();
    out.push_str("// Erika -> Desmos export. Paste into the browser console on a Desmos graph.\n");
    out.push_str("// Requires the global `Calc` (open https://www.desmos.com/calculator).\n");
    out.push_str("// REPLACES every expression on the graph. Folder nesting needs setState\n");
    out.push_str("// (setExpressions ignores folderId), so we swap the live state's list.\n");
    out.push_str("(function () {\n");
    out.push_str("  if (typeof Calc === \"undefined\") {\n");
    out.push_str("    console.error(\"Desmos `Calc` not found - open a Desmos calculator first.\");\n");
    out.push_str("    return;\n");
    out.push_str("  }\n");
    out.push_str("  var list = [\n");
    out.push_str(&lines.join(",\n"));
    out.push_str("\n  ];\n");
    out.push_str("  var state = Calc.getState();\n");
    out.push_str("  state.expressions.list = list;\n");
    out.push_str("  Calc.setState(state);\n");
    out.push_str("  console.log(\"Erika: set \" + list.length + \" items.\");\n");
    out.push_str("})();\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::P;

    fn bezier(name: &str, group_id: u64, segs: &[(f32, f32, f32, f32, f32, f32)]) -> CurveSet {
        let mut c = CurveSet::empty(name, [10, 20, 30]);
        c.group_id = Some(group_id);
        for &(x1, y1, x2, y2, x3, y3) in segs {
            c.s1.push(P::new(x1, y1));
            c.s2.push(P::new(x2, y2));
            c.s3.push(P::new(x3, y3));
        }
        c
    }

    #[test]
    fn template_folder_holds_helper_lines() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];
        let tpl = vec!["i\\left(t\\right)=t".to_string()];

        let out = build_js_output(&curves, &groups, &tpl);
        assert!(out.contains("\"type\": \"folder\", \"id\": \"2\", \"title\": \"Template\""));
        assert!(out.contains("\"folderId\": \"2\""));
        assert!(out.contains("i\\\\left(t\\\\right)=t"));
    }

    #[test]
    fn group_becomes_folder_with_plot_and_data() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];

        let out = build_js_output(&curves, &groups, &[]);
        assert!(out.contains("\"id\": \"3\", \"title\": \"Face\""));
        assert!(out.contains("B_{x}\\\\left(A_{1}"));
        assert!(out.contains("\"parametricDomain\": { \"min\": \"0\", \"max\": \"99\" }"));
        assert!(out.contains("A_{1}=[(0,0)]"));
        assert!(out.contains("Calc.setState(state)"));
    }

    #[test]
    fn plot_domain_is_zero_to_99() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier(
            "a",
            1,
            &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0), (2.0, 0.0, 3.0, 1.0, 4.0, 0.0)],
        )];
        let out = build_js_output(&curves, &groups, &[]);
        assert!(out.contains("\"parametricDomain\": { \"min\": \"0\", \"max\": \"99\" }"));
        assert!(!out.contains("\"max\": \"2\""));
    }

    #[test]
    fn segment_data_lists_are_hidden() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];
        let out = build_js_output(&curves, &groups, &[]);
        let data = out.lines().find(|l| l.contains("A_{1}=")).unwrap();
        assert!(data.contains("\"hidden\": true"));
    }

    #[test]
    fn uses_setstate_so_folders_nest() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];
        let out = build_js_output(&curves, &groups, &[]);
        assert!(out.contains("state.expressions.list = list"));
        assert!(out.contains("Calc.setState(state)"));
        assert!(!out.contains("Calc.setExpressions("));
    }

    #[test]
    fn t_helper_lines_are_hidden_but_multiarg_helpers_are_not() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];
        let tpl = vec![
            "i\\left(t\\right)=\\operatorname{ceil}\\left(t\\right)".to_string(),
            "B_{x}\\left(X_{1},X_{2},X_{3}\\right)=j\\left(t\\right)".to_string(),
        ];

        let out = build_js_output(&curves, &groups, &tpl);
        let line_i = out.lines().find(|l| l.contains("\"id\": \"3\"")).unwrap();
        assert!(line_i.contains("\"hidden\": true"));
        let line_b = out.lines().find(|l| l.contains("\"id\": \"4\"")).unwrap();
        assert!(!line_b.contains("\"hidden\": true"));
    }

    #[test]
    fn fill_enabled_curve_emits_fill_on_plot_only() {
        let groups = vec![Group::new(1, "Face", "A")];
        let mut c = bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        c.fill_enabled = true;
        c.fill_color = [10, 20, 30, 128];
        let curves = vec![c];

        let out = build_js_output(&curves, &groups, &[]);
        let plot = out.lines().find(|l| l.contains("B_{x}\\\\left(A_{1}")).unwrap();
        assert!(plot.contains("\"fill\": true"));
        assert!(plot.contains("\"fillOpacity\": \"0.502\""));
        let data = out.lines().find(|l| l.contains("A_{1}=")).unwrap();
        assert!(!data.contains("\"fill\": true"));
    }

    #[test]
    fn fill_disabled_curve_has_no_fill() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];

        let out = build_js_output(&curves, &groups, &[]);
        assert!(!out.contains("\"fill\": true"));
    }

    #[test]
    fn empty_groups_are_skipped() {
        let groups = vec![Group::new(1, "Used", "A"), Group::new(2, "Empty", "B")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];

        let out = build_js_output(&curves, &groups, &[]);
        assert!(out.contains("\"title\": \"Used\""));
        assert!(!out.contains("\"title\": \"Empty\""));
    }
}
