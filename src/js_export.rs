use std::path::Path;

use crate::curve::{CurveKind, CurveSet, Group};
use crate::tex_export::{bezier_data_latex, bezier_plot_latex, ellipse_latex};

pub struct JsConfig<'a> {
    pub path: &'a Path,
    pub template_path: &'a Path,
}

const TEMPLATE_COLORS: &[&str] = &["#2d70b3", "#388c46", "#6042a6", "#000000"];

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
    for (k, line) in template_lines.iter().enumerate() {
        items.push(Item::Expr {
            id: fresh_id(&mut next_id),
            folder_id: template_folder.clone(),
            color: TEMPLATE_COLORS[k % TEMPLATE_COLORS.len()].to_string(),
            latex: line.clone(),
            parametric_max: None,
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
                    parametric_max: Some(c.n()),
                }),
                CurveKind::Ellipse => items.push(Item::Expr {
                    id: fresh_id(&mut next_id),
                    folder_id: folder_id.clone(),
                    color,
                    latex: ellipse_latex(c),
                    parametric_max: None,
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
            } => {
                let domain = match parametric_max {
                    Some(max) => format!(
                        ", \"parametricDomain\": {{ \"min\": \"0\", \"max\": \"{max}\" }}"
                    ),
                    None => String::new(),
                };
                format!(
                    "  {{ \"type\": \"expression\", \"id\": \"{}\", \"folderId\": \"{}\", \"color\": \"{}\", \"latex\": \"{}\"{} }}",
                    id,
                    folder_id,
                    color,
                    js_escape(latex),
                    domain
                )
            }
        })
        .collect();

    let mut out = String::new();
    out.push_str("// Erika -> Desmos export. Paste into the browser console on a Desmos graph.\n");
    out.push_str("// Requires the global `Calc` (open https://www.desmos.com/calculator).\n");
    out.push_str("// For an exact copy, run Calc.setBlank() before pasting this.\n");
    out.push_str("(function () {\n");
    out.push_str("  if (typeof Calc === \"undefined\") {\n");
    out.push_str("    console.error(\"Desmos `Calc` not found - open a Desmos calculator first.\");\n");
    out.push_str("    return;\n");
    out.push_str("  }\n");
    out.push_str("  var exprs = [\n");
    out.push_str(&lines.join(",\n"));
    out.push_str("\n  ];\n");
    out.push_str("  Calc.setExpressions(exprs);\n");
    out.push_str("  console.log(\"Erika: inserted \" + exprs.length + \" items.\");\n");
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
        assert!(out.contains("\"parametricDomain\": { \"min\": \"0\", \"max\": \"1\" }"));
        assert!(out.contains("A_{1}=[(0,0)]"));
        assert!(out.contains("Calc.setExpressions(exprs)"));
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
