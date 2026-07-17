use std::collections::HashMap;
use std::path::Path;

use crate::model::curve::{CurveKind, CurveSet, Group};
use crate::export::tex::{bezier_data_latex, bezier_plot_latex_restricted, ellipse_latex};

pub struct JsConfig<'a> {
    pub path: &'a Path,
    pub template_path: &'a Path,
    pub timelapse: bool,
    pub duration_secs: f32,
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

    let body = build_js_output_opts(
        curves,
        groups,
        &template_lines,
        cfg.timelapse,
        cfg.duration_secs,
    );

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
        lines: bool,
    },
    Slider {
        id: String,
        folder_id: String,
        latex: String,
        min: String,
        max: String,
        period_ms: u64,
    },
}

#[cfg(test)]
fn build_js_output(curves: &[CurveSet], groups: &[Group], template_lines: &[String]) -> String {
    build_js_output_opts(curves, groups, template_lines, false, 5.0)
}

fn build_js_output_opts(
    curves: &[CurveSet],
    groups: &[Group],
    template_lines: &[String],
    timelapse: bool,
    duration_secs: f32,
) -> String {
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
            lines: true,
        });
    }

    let folders = plan_folders(curves, groups);

    if timelapse {
        let folder_count = folders.len();
        let tl_folder = fresh_id(&mut next_id);
        items.push(Item::Folder {
            id: tl_folder.clone(),
            title: "Time-lapse".to_string(),
        });
        let period_ms = (duration_secs.max(0.1) * 1000.0).round() as u64;
        items.push(Item::Slider {
            id: fresh_id(&mut next_id),
            folder_id: tl_folder.clone(),
            latex: "S=0".to_string(),
            min: "0".to_string(),
            max: folder_count.to_string(),
            period_ms,
        });
        items.push(Item::Expr {
            id: fresh_id(&mut next_id),
            folder_id: tl_folder,
            color: "#000000".to_string(),
            latex: "q\\left(f,u\\right)=u\\min\\left(\\max\\left(S-f,0\\right),1\\right)"
                .to_string(),
            parametric_max: None,
            hidden: false,
            fill_opacity: None,
            lines: true,
        });
    }

    struct CurvePlot {
        stroke_color: String,
        latex: String,
        parametric_max: Option<usize>,
        data_latex: Vec<String>,
        fill: Option<(String, f32, String)>,
    }

    let mut folder_plan: Vec<(String, Vec<CurvePlot>)> = Vec::new();
    let mut bez_counters: HashMap<usize, usize> = HashMap::new();
    for (f, (gi, title, members)) in folders.iter().enumerate() {
        let g = &groups[*gi];
        let folder_units: usize = members.iter().map(|&ci| curves[ci].draw_units()).sum();
        let mut offset = 0usize;
        let bez_idx = bez_counters.entry(*gi).or_insert(0);
        let mut plots: Vec<CurvePlot> = Vec::new();

        for &ci in members {
            let c = &curves[ci];
            let stroke_color = hex(c.color);
            let plot = match c.kind {
                CurveKind::Bezier => {
                    let idx = *bez_idx;
                    *bez_idx += 1;
                    let gate = if timelapse {
                        folder_bezier_gate(f, folder_units, offset)
                    } else {
                        String::new()
                    };
                    let latex = bezier_plot_latex_restricted(&g.tex_param, idx, &gate);
                    let data_latex = bezier_data_latex(c, &g.tex_param, idx);
                    let fill = fill_opacity_for(c)
                        .map(|op| (fill_hex(c), op, bezier_plot_latex_restricted(&g.tex_param, idx, "")));
                    CurvePlot {
                        stroke_color,
                        latex,
                        parametric_max: Some(PLOT_DOMAIN_MAX),
                        data_latex,
                        fill,
                    }
                }
                CurveKind::Ellipse => {
                    let gate = if timelapse {
                        folder_ellipse_gate(f, folder_units, offset)
                    } else {
                        String::new()
                    };
                    let latex = format!("{}{}", ellipse_latex(c), gate);
                    let fill = fill_opacity_for(c).map(|op| (fill_hex(c), op, ellipse_fill_latex(c)));
                    CurvePlot {
                        stroke_color,
                        latex,
                        parametric_max: None,
                        data_latex: Vec::new(),
                        fill,
                    }
                }
            };
            plots.push(plot);
            offset += c.draw_units();
        }
        folder_plan.push((title.clone(), plots));
    }

    for (name, plots) in &folder_plan {
        if plots.iter().any(|p| p.fill.is_some()) {
            let fill_folder = fresh_id(&mut next_id);
            items.push(Item::Folder {
                id: fill_folder.clone(),
                title: format!("Fill {name}"),
            });
            for p in plots {
                if let Some((color, opacity, latex)) = &p.fill {
                    items.push(Item::Expr {
                        id: fresh_id(&mut next_id),
                        folder_id: fill_folder.clone(),
                        color: color.clone(),
                        latex: latex.clone(),
                        parametric_max: p.parametric_max,
                        hidden: false,
                        fill_opacity: Some(*opacity),
                        lines: false,
                    });
                }
            }
        }
        let folder_id = fresh_id(&mut next_id);
        items.push(Item::Folder {
            id: folder_id.clone(),
            title: name.clone(),
        });
        for p in plots {
            items.push(Item::Expr {
                id: fresh_id(&mut next_id),
                folder_id: folder_id.clone(),
                color: p.stroke_color.clone(),
                latex: p.latex.clone(),
                parametric_max: p.parametric_max,
                hidden: false,
                fill_opacity: None,
                lines: true,
            });
        }
        for p in plots {
            for latex in &p.data_latex {
                items.push(Item::Expr {
                    id: fresh_id(&mut next_id),
                    folder_id: folder_id.clone(),
                    color: p.stroke_color.clone(),
                    latex: latex.clone(),
                    parametric_max: None,
                    hidden: true,
                    fill_opacity: None,
                    lines: true,
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

fn plan_folders(curves: &[CurveSet], groups: &[Group]) -> Vec<(usize, String, Vec<usize>)> {
    let mut runs: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, c) in curves.iter().enumerate() {
        if !is_exportable(c) {
            continue;
        }
        let gi = group_index_of(c, groups);
        match runs.last_mut() {
            Some((last, members)) if *last == gi => members.push(i),
            _ => runs.push((gi, vec![i])),
        }
    }
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for (gi, _) in &runs {
        *counts.entry(*gi).or_insert(0) += 1;
    }
    let mut seen: HashMap<usize, usize> = HashMap::new();
    runs.into_iter()
        .map(|(gi, members)| {
            let title = if counts[&gi] > 1 {
                let k = seen.entry(gi).or_insert(0);
                *k += 1;
                format!("{} {}", groups[gi].name, k)
            } else {
                groups[gi].name.clone()
            };
            (gi, title, members)
        })
        .collect()
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

fn folder_reveal_expr(folder: usize, folder_units: usize) -> String {
    format!("q\\left({folder},{folder_units}\\right)")
}

fn folder_bezier_gate(folder: usize, folder_units: usize, offset: usize) -> String {
    let reveal = folder_reveal_expr(folder, folder_units);
    let amount = if offset == 0 {
        reveal
    } else {
        format!("{reveal}-{offset}")
    };
    format!("\\left\\{{t\\le {amount}\\right\\}}")
}

fn folder_ellipse_gate(folder: usize, folder_units: usize, offset: usize) -> String {
    let reveal = folder_reveal_expr(folder, folder_units);
    format!("\\left\\{{{reveal}>{offset}\\right\\}}")
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

fn fill_hex(c: &CurveSet) -> String {
    hex([c.fill_color[0], c.fill_color[1], c.fill_color[2]])
}

fn ellipse_fill_latex(c: &CurveSet) -> String {
    let s = ellipse_latex(c);
    match s.strip_suffix("=1") {
        Some(base) => format!("{base}\\le1"),
        None => s,
    }
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
                lines,
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
                let lines_attr = if *lines { "" } else { ", \"lines\": false" };
                format!(
                    "  {{ \"type\": \"expression\", \"id\": \"{}\", \"folderId\": \"{}\", \"color\": \"{}\"{}, \"latex\": \"{}\"{}{}{} }}",
                    id,
                    folder_id,
                    color,
                    hidden_attr,
                    js_escape(latex),
                    domain,
                    fill_attr,
                    lines_attr
                )
            }
            Item::Slider {
                id,
                folder_id,
                latex,
                min,
                max,
                period_ms,
            } => format!(
                "  {{ \"type\": \"expression\", \"id\": \"{}\", \"folderId\": \"{}\", \"color\": \"#000000\", \"latex\": \"{}\", \"slider\": {{ \"hardMin\": true, \"hardMax\": true, \"min\": \"{}\", \"max\": \"{}\", \"loopMode\": \"PLAY_ONCE\", \"isPlaying\": true, \"animationPeriod\": {} }} }}",
                id,
                folder_id,
                js_escape(latex),
                min,
                max,
                period_ms
            ),
        })
        .collect();

    let has_slider = items.iter().any(|i| matches!(i, Item::Slider { .. }));

    let mut out = String::new();
    out.push_str("// Erika -> Desmos export. Paste into the browser console on a Desmos graph.\n");
    out.push_str("// Requires the global `Calc` (open https://www.desmos.com/calculator).\n");
    out.push_str("// REPLACES every expression on the graph. Folder nesting needs setState\n");
    out.push_str("// (setExpressions ignores folderId), so we swap the live state's list.\n");
    if has_slider {
        out.push_str(
            "// Time-lapse: the `S` slider auto-plays once and draws the art folder by\n",
        );
        out.push_str("//   folder (floor(S) = current folder). Press play on `S` to replay.\n");
    }
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
    use crate::model::curve::P;

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
    fn fill_splits_into_fill_folder_and_stroke_only_group() {
        let groups = vec![Group::new(1, "Face", "A")];
        let mut c = bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        c.color = [200, 100, 50];
        c.fill_enabled = true;
        c.fill_color = [10, 20, 30, 128];
        let curves = vec![c];

        let out = build_js_output(&curves, &groups, &[]);

        assert!(out.contains("\"title\": \"Fill Face\""));
        let fill = out
            .lines()
            .find(|l| l.contains("\"lines\": false"))
            .expect("fill clone");
        assert!(fill.contains("\"fill\": true"));
        assert!(fill.contains("\"fillOpacity\": \"0.502\""));
        assert!(fill.contains("\"color\": \"#0a141e\""));
        assert!(fill.contains("B_{x}\\\\left(A_{1}"));

        let stroke = out
            .lines()
            .find(|l| l.contains("B_{x}\\\\left(A_{1}") && !l.contains("\"lines\": false"))
            .expect("stroke plot");
        assert!(!stroke.contains("\"fill\": true"));
        assert!(stroke.contains("\"color\": \"#c86432\""));
    }

    #[test]
    fn fill_folder_renders_behind_stroke_folders() {
        let groups = vec![Group::new(1, "Face", "A")];
        let mut c = bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        c.fill_enabled = true;
        let curves = vec![c];

        let out = build_js_output(&curves, &groups, &[]);
        let fill_pos = out.find("\"title\": \"Fill Face\"").expect("Fill Face folder");
        let face_pos = out.find("\"title\": \"Face\"").expect("Face folder");
        assert!(fill_pos < face_pos, "Fill folder should be emitted first (behind)");
    }

    #[test]
    fn fills_interleave_per_folder_in_layer_order() {
        let groups = vec![Group::new(1, "Hair", "A"), Group::new(2, "Face", "B")];
        let back = bezier("back", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let mut face = bezier("face", 2, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]);
        face.fill_enabled = true;
        let mut front = bezier("front", 1, &[(6.0, 6.0, 7.0, 7.0, 8.0, 6.0)]);
        front.fill_enabled = true;
        let curves = vec![back, face, front];

        let out = build_js_output(&curves, &groups, &[]);
        assert!(!out.contains("\"title\": \"Fill Hair 1\""), "no fill folder without fills");
        let h1 = out.find("\"title\": \"Hair 1\"").expect("Hair 1");
        let ff = out.find("\"title\": \"Fill Face\"").expect("Fill Face");
        let fa = out.find("\"title\": \"Face\"").expect("Face");
        let fh2 = out.find("\"title\": \"Fill Hair 2\"").expect("Fill Hair 2");
        let h2 = out.find("\"title\": \"Hair 2\"").expect("Hair 2");
        assert!(
            h1 < ff && ff < fa && fa < fh2 && fh2 < h2,
            "each fill folder sits right behind its stroke folder"
        );
    }

    #[test]
    fn timelapse_gates_strokes_but_not_fills() {
        let groups = vec![Group::new(1, "Face", "A")];
        let mut c = bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        c.fill_enabled = true;
        let curves = vec![c];

        let out = build_js_output_opts(&curves, &groups, &[], true, 5.0);
        let fill = out
            .lines()
            .find(|l| l.contains("\"lines\": false"))
            .expect("fill clone");
        assert!(!fill.contains("q\\\\left("), "fills must not be gated");
        let stroke = out
            .lines()
            .find(|l| l.contains("B_{x}\\\\left(A_{1}") && !l.contains("\"lines\": false"))
            .expect("stroke plot");
        assert!(stroke.contains("q\\\\left(0,1\\\\right)"), "strokes stay gated");
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

    #[test]
    fn folders_emit_in_layer_order_bottom_curve_first() {
        let groups = vec![Group::new(1, "Top", "T"), Group::new(2, "Bottom", "B")];
        let bottom = bezier("bottom", 2, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let top = bezier("top", 1, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]);
        let curves = vec![bottom, top];

        let out = build_js_output(&curves, &groups, &[]);
        let bottom_pos = out.find("\"title\": \"Bottom\"").expect("Bottom folder");
        let top_pos = out.find("\"title\": \"Top\"").expect("Top folder");
        assert!(
            bottom_pos < top_pos,
            "folder with the bottom-most curve should be emitted first"
        );
    }

    #[test]
    fn interleaved_groups_split_into_numbered_folders() {
        let groups = vec![Group::new(1, "Hair", "A"), Group::new(2, "Face", "B")];
        let back = bezier("back", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let face = bezier("face", 2, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]);
        let front = bezier("front", 1, &[(6.0, 6.0, 7.0, 7.0, 8.0, 6.0)]);
        let curves = vec![back, face, front];

        let out = build_js_output(&curves, &groups, &[]);
        let h1 = out.find("\"title\": \"Hair 1\"").expect("Hair 1 folder");
        let fa = out.find("\"title\": \"Face\"").expect("Face folder");
        let h2 = out.find("\"title\": \"Hair 2\"").expect("Hair 2 folder");
        assert!(h1 < fa && fa < h2, "folders follow layer order, bottom first");
        assert!(!out.contains("\"title\": \"Hair\" "), "no unnumbered Hair folder");
        assert!(!out.contains("\"title\": \"Face 1\""), "contiguous group keeps its name");
    }

    #[test]
    fn split_group_keeps_distinct_data_variables() {
        let groups = vec![Group::new(1, "Hair", "A"), Group::new(2, "Face", "B")];
        let back = bezier("back", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let face = bezier("face", 2, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]);
        let front = bezier("front", 1, &[(6.0, 6.0, 7.0, 7.0, 8.0, 6.0)]);
        let curves = vec![back, face, front];

        let out = build_js_output(&curves, &groups, &[]);
        assert!(out.contains("A_{1}=[(0,0)]"), "first Hair curve keeps index 1");
        assert!(out.contains("A_{11}=[(6,6)]"), "second Hair curve continues at index 11");
    }

    #[test]
    fn contiguous_group_stays_one_plain_folder() {
        let groups = vec![Group::new(1, "Hair", "A"), Group::new(2, "Face", "B")];
        let back = bezier("back", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let front = bezier("front", 1, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]);
        let face = bezier("face", 2, &[(6.0, 6.0, 7.0, 7.0, 8.0, 6.0)]);
        let curves = vec![back, front, face];

        let out = build_js_output(&curves, &groups, &[]);
        assert!(out.contains("\"title\": \"Hair\""));
        assert!(!out.contains("\"title\": \"Hair 1\""));
        assert!(!out.contains("\"title\": \"Hair 2\""));
    }

    #[test]
    fn timelapse_splits_interleaved_groups_and_reveals_in_layer_order() {
        let groups = vec![Group::new(1, "Hair", "A"), Group::new(2, "Face", "B")];
        let back = bezier("back", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let face = bezier("face", 2, &[(3.0, 3.0, 4.0, 4.0, 5.0, 3.0)]);
        let front = bezier("front", 1, &[(6.0, 6.0, 7.0, 7.0, 8.0, 6.0)]);
        let curves = vec![back, face, front];

        let out = build_js_output_opts(&curves, &groups, &[], true, 5.0);
        assert!(out.contains("\"title\": \"Hair 1\""));
        assert!(out.contains("\"title\": \"Hair 2\""));
        assert!(out.contains("\"max\": \"3\""), "slider counts three folders");
        assert!(out.contains("t\\\\le q\\\\left(0,1\\\\right)"), "Hair 1 draws first");
        assert!(out.contains("t\\\\le q\\\\left(1,1\\\\right)"), "Face draws second");
        assert!(out.contains("t\\\\le q\\\\left(2,1\\\\right)"), "Hair 2 draws last");
    }

    #[test]
    fn timelapse_adds_global_step_and_folder_step_helper() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier(
            "a",
            1,
            &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0), (2.0, 0.0, 3.0, 1.0, 4.0, 0.0)],
        )];

        let out = build_js_output_opts(&curves, &groups, &[], true, 5.0);
        assert!(out.contains("\"title\": \"Time-lapse\""));
        assert!(out.contains("\"latex\": \"S=0\""));
        assert!(out.contains("\"slider\""));
        assert!(out.contains("\"animationPeriod\": 5000"));
        assert!(out.contains("\"max\": \"1\""));
        assert!(out.contains("q\\\\left(f,u\\\\right)="));
        assert!(out.contains("t\\\\le q\\\\left(0,2\\\\right)\\\\right\\\\}"));
        assert!(out.contains("\"parametricDomain\": { \"min\": \"0\", \"max\": \"99\" }"));
    }

    #[test]
    fn timelapse_offsets_curves_within_a_folder() {
        let groups = vec![Group::new(1, "Face", "A")];
        let a = bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let b = bezier(
            "b",
            1,
            &[(2.0, 0.0, 3.0, 1.0, 4.0, 0.0), (4.0, 0.0, 5.0, 1.0, 6.0, 0.0)],
        );
        let curves = vec![a, b];

        let out = build_js_output_opts(&curves, &groups, &[], true, 3.0);
        assert!(out.contains("\"max\": \"1\""));
        assert!(out.contains("t\\\\le q\\\\left(0,3\\\\right)-1\\\\right\\\\}"));
    }

    #[test]
    fn timelapse_draws_folders_on_successive_steps() {
        let groups = vec![Group::new(1, "A", "A"), Group::new(2, "B", "B")];
        let a = bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let b = bezier("b", 2, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]);
        let curves = vec![a, b];

        let out = build_js_output_opts(&curves, &groups, &[], true, 4.0);
        assert!(out.contains("\"max\": \"2\""));
        assert!(out.contains("t\\\\le q\\\\left(0,1\\\\right)"));
        assert!(out.contains("t\\\\le q\\\\left(1,1\\\\right)"));
    }

    #[test]
    fn anim_js_roundtrip_on_real_export() {
        use crate::export::anim_js;
        let groups = vec![Group::new(1, "Face", "A")];
        let a = vec![
            bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]),
            bezier("b", 1, &[(5.0, 5.0, 6.0, 6.0, 7.0, 5.0)]),
        ];
        let b = vec![
            bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)]),
            bezier("b", 1, &[(9.0, 9.0, 10.0, 10.0, 11.0, 9.0)]),
        ];
        let ta = build_js_output(&a, &groups, &[]);
        let tb = build_js_output(&b, &groups, &[]);
        let ia = anim_js::parse_js(&ta).expect("parse A");
        let ib = anim_js::parse_js(&tb).expect("parse B");
        let diff = anim_js::compute_js_diff(vec![ia, ib]);
        assert_eq!(diff.same_count, 1, "curve a unchanged");
        assert_eq!(diff.changing_count(), 1, "curve b changed");
        let out = anim_js::build_animated_js(&diff, &[2.5]);
        assert!(out.contains("LOOP_FORWARD_REVERSE"));
        assert!(out.contains("\\\\frac{m_{orph}}{2.5}"));
        assert!(out.contains("\"animationPeriod\":2500"));
        assert!(out.contains("A_{1}=[(0,0)]"), "unchanged curve stays static");
    }

    #[test]
    fn non_timelapse_export_has_no_slider_or_gate() {
        let groups = vec![Group::new(1, "Face", "A")];
        let curves = vec![bezier("a", 1, &[(0.0, 0.0, 1.0, 1.0, 2.0, 0.0)])];
        let out = build_js_output_opts(&curves, &groups, &[], false, 5.0);
        assert!(!out.contains("\"slider\""));
        assert!(!out.contains("Time-lapse"));
        assert!(!out.contains("q\\\\left(f,u"));
    }
}
