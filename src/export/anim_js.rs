use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JsDiffKind {
    Morph,
    StructMismatch,
}

impl JsDiffKind {
    pub fn label(self) -> &'static str {
        match self {
            JsDiffKind::Morph => "morph",
            JsDiffKind::StructMismatch => "structure differs (static)",
        }
    }
}

pub struct JsRow {
    pub curve_key: String,
    pub label: String,
    pub color: [u8; 3],
    pub kind: JsDiffKind,
    pub dur: f32,
    pub enabled: bool,
}

pub struct JsDiff {
    pub items_a: Vec<Value>,
    pub to_rhs: HashMap<String, String>,
    pub rows: Vec<JsRow>,
    pub same_count: usize,
    pub added: usize,
    pub removed: usize,
}

impl JsDiff {
    pub fn changing_count(&self) -> usize {
        self.rows.len()
    }
}

pub fn list_js_files(dir: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("js"))
        .collect();
    out.sort();
    out
}

pub fn load_js_diff(a: &Path, b: &Path, default_dur: f32) -> Result<JsDiff, String> {
    let text_a = std::fs::read_to_string(a).map_err(|e| format!("read A: {e}"))?;
    let text_b = std::fs::read_to_string(b).map_err(|e| format!("read B: {e}"))?;
    let items_a = parse_js(&text_a)?;
    let items_b = parse_js(&text_b)?;
    Ok(compute_js_diff(items_a, items_b, default_dur))
}

pub fn parse_js(text: &str) -> Result<Vec<Value>, String> {
    let start = text
        .find("var list = [")
        .ok_or("Not an Erika Desmos export (no `var list`)")?;
    let arr_start = start + "var list = ".len();
    let end = text[arr_start..]
        .find("\n  ];")
        .map(|i| i + arr_start)
        .ok_or("Malformed export (no list terminator)")?;
    let arr_text = &text[arr_start..=end + 3];
    serde_json::from_str::<Vec<Value>>(arr_text).map_err(|e| format!("parse list: {e}"))
}

fn data_list_parts(latex: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = latex.split_once('=')?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    if rhs.starts_with('[') && rhs.ends_with(']') && lhs.contains("_{") {
        Some((lhs, rhs))
    } else {
        None
    }
}

fn curve_key(var: &str) -> Option<String> {
    let (letter, rest) = var.split_once("_{")?;
    let sub = rest.strip_suffix('}')?;
    if sub.is_empty() {
        return None;
    }
    let mut chars: Vec<char> = sub.chars().collect();
    chars.pop();
    let key_sub: String = chars.into_iter().collect();
    Some(format!("{letter}_{key_sub}"))
}

fn count_pts(rhs: &str) -> usize {
    rhs.matches('(').count()
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

fn data_lists(items: &[Value]) -> BTreeMap<String, (String, [u8; 3])> {
    let mut m = BTreeMap::new();
    for it in items {
        if let Some(latex) = it.get("latex").and_then(|v| v.as_str()) {
            if let Some((var, rhs)) = data_list_parts(latex) {
                let color = it
                    .get("color")
                    .and_then(|v| v.as_str())
                    .and_then(parse_hex)
                    .unwrap_or([90, 90, 90]);
                m.insert(var.to_string(), (rhs.to_string(), color));
            }
        }
    }
    m
}

fn curves_of(lists: &BTreeMap<String, (String, [u8; 3])>) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for var in lists.keys() {
        if let Some(key) = curve_key(var) {
            out.entry(key).or_default().push(var.clone());
        }
    }
    out
}

pub fn compute_js_diff(items_a: Vec<Value>, items_b: Vec<Value>, default_dur: f32) -> JsDiff {
    let a_lists = data_lists(&items_a);
    let b_lists = data_lists(&items_b);
    let a_curves = curves_of(&a_lists);
    let b_curves = curves_of(&b_lists);
    let b_keys: BTreeSet<&String> = b_curves.keys().collect();
    let a_keys: BTreeSet<&String> = a_curves.keys().collect();

    let mut rows = Vec::new();
    let mut to_rhs = HashMap::new();
    let mut same_count = 0;

    for (key, vars) in &a_curves {
        if !b_keys.contains(key) {
            continue;
        }
        let mut any_diff = false;
        let mut morphable = true;
        for var in vars {
            let a_rhs = &a_lists[var].0;
            match b_lists.get(var) {
                Some((b_rhs, _)) => {
                    if a_rhs != b_rhs {
                        any_diff = true;
                    }
                    if count_pts(a_rhs) != count_pts(b_rhs) {
                        morphable = false;
                    }
                }
                None => {
                    any_diff = true;
                    morphable = false;
                }
            }
        }
        if !any_diff {
            same_count += 1;
            continue;
        }
        let color = a_lists[&vars[0]].1;
        let label = vars.iter().min().cloned().unwrap_or_else(|| key.clone());
        let kind = if morphable {
            for var in vars {
                if let Some((b_rhs, _)) = b_lists.get(var) {
                    to_rhs.insert(var.clone(), b_rhs.clone());
                }
            }
            JsDiffKind::Morph
        } else {
            JsDiffKind::StructMismatch
        };
        rows.push(JsRow {
            curve_key: key.clone(),
            label,
            color,
            kind,
            dur: default_dur,
            enabled: true,
        });
    }

    let added = b_keys.difference(&a_keys).count();
    let removed = a_keys.difference(&b_keys).count();

    JsDiff {
        items_a,
        to_rhs,
        rows,
        same_count,
        added,
        removed,
    }
}

fn max_id(items: &[Value]) -> u32 {
    items
        .iter()
        .filter_map(|it| it.get("id"))
        .filter_map(|v| v.as_str())
        .filter_map(|s| s.parse::<u32>().ok())
        .max()
        .unwrap_or(1)
}

pub fn build_animated_js(diff: &JsDiff) -> String {
    let mut items = diff.items_a.clone();

    let mut slider_of: HashMap<String, String> = HashMap::new();
    let mut sliders: Vec<(String, f32)> = Vec::new();
    let mut k = 1u32;
    for row in &diff.rows {
        if matches!(row.kind, JsDiffKind::Morph) && row.enabled {
            let svar = format!("m_{{orph{k}}}");
            slider_of.insert(row.curve_key.clone(), svar.clone());
            sliders.push((svar, row.dur));
            k += 1;
        }
    }

    for it in items.iter_mut() {
        let replacement = {
            let Some(latex) = it.get("latex").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some((var, from_rhs)) = data_list_parts(latex) else {
                continue;
            };
            let Some(key) = curve_key(var) else {
                continue;
            };
            match (slider_of.get(&key), diff.to_rhs.get(var)) {
                (Some(svar), Some(to)) => Some(format!(
                    "{var}=\\left(1-{svar}\\right)\\cdot{from_rhs}+{svar}\\cdot{to}"
                )),
                _ => None,
            }
        };
        if let Some(new_latex) = replacement {
            if let Some(obj) = it.as_object_mut() {
                obj.insert("latex".to_string(), Value::String(new_latex));
            }
        }
    }

    if !sliders.is_empty() {
        let mut next = max_id(&items) + 1;
        let folder_id = next.to_string();
        next += 1;
        items.push(json!({ "type": "folder", "id": folder_id, "title": "Morph" }));
        for (svar, dur) in &sliders {
            let id = next.to_string();
            next += 1;
            let period = (dur.max(0.1) * 1000.0).round() as u64;
            items.push(json!({
                "type": "expression",
                "id": id,
                "folderId": folder_id,
                "color": "#000000",
                "latex": format!("{svar}=0"),
                "slider": {
                    "hardMin": true,
                    "hardMax": true,
                    "min": "0",
                    "max": "1",
                    "loopMode": "LOOP_FORWARD_REVERSE",
                    "isPlaying": true,
                    "animationPeriod": period
                }
            }));
        }
    }

    render_js_values(&items)
}

fn render_js_values(items: &[Value]) -> String {
    let mut out = String::new();
    out.push_str("// Erika -> Desmos morph export. Paste into the browser console on a Desmos graph.\n");
    out.push_str("// Requires the global `Calc` (open https://www.desmos.com/calculator).\n");
    out.push_str("// The m_orph# sliders auto-play (ping-pong) and morph each changed line.\n");
    out.push_str("(function () {\n");
    out.push_str("  if (typeof Calc === \"undefined\") {\n");
    out.push_str("    console.error(\"Desmos `Calc` not found - open a Desmos calculator first.\");\n");
    out.push_str("    return;\n");
    out.push_str("  }\n");
    out.push_str("  var list = [\n");
    let lines: Vec<String> = items
        .iter()
        .map(|it| format!("  {}", serde_json::to_string(it).unwrap_or_default()))
        .collect();
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

    fn wrap(items: &[&str]) -> String {
        let body: Vec<String> = items.iter().map(|s| format!("  {s}")).collect();
        format!(
            "// header\n(function () {{\n  var list = [\n{}\n  ];\n  var state = Calc.getState();\n}})();\n",
            body.join(",\n")
        )
    }

    fn js_a() -> String {
        wrap(&[
            r##"{ "type": "folder", "id": "2", "title": "Face" }"##,
            r##"{ "type": "expression", "id": "5", "folderId": "2", "color": "#ff0000", "latex": "A_{1}=[(0,0)]" }"##,
            r##"{ "type": "expression", "id": "6", "folderId": "2", "color": "#ff0000", "latex": "A_{2}=[(1,1)]" }"##,
            r##"{ "type": "expression", "id": "7", "folderId": "2", "color": "#ff0000", "latex": "A_{3}=[(2,0)]" }"##,
            r##"{ "type": "expression", "id": "8", "folderId": "2", "color": "#0000ff", "latex": "A_{11}=[(5,5)]" }"##,
            r##"{ "type": "expression", "id": "9", "folderId": "2", "color": "#0000ff", "latex": "A_{12}=[(6,6)]" }"##,
            r##"{ "type": "expression", "id": "10", "folderId": "2", "color": "#0000ff", "latex": "A_{13}=[(7,5)]" }"##,
        ])
    }

    fn js_b() -> String {
        wrap(&[
            r##"{ "type": "folder", "id": "2", "title": "Face" }"##,
            r##"{ "type": "expression", "id": "5", "folderId": "2", "color": "#ff0000", "latex": "A_{1}=[(0,0)]" }"##,
            r##"{ "type": "expression", "id": "6", "folderId": "2", "color": "#ff0000", "latex": "A_{2}=[(1,1)]" }"##,
            r##"{ "type": "expression", "id": "7", "folderId": "2", "color": "#ff0000", "latex": "A_{3}=[(2,0)]" }"##,
            r##"{ "type": "expression", "id": "8", "folderId": "2", "color": "#0000ff", "latex": "A_{11}=[(9,9)]" }"##,
            r##"{ "type": "expression", "id": "9", "folderId": "2", "color": "#0000ff", "latex": "A_{12}=[(10,10)]" }"##,
            r##"{ "type": "expression", "id": "10", "folderId": "2", "color": "#0000ff", "latex": "A_{13}=[(11,9)]" }"##,
        ])
    }

    #[test]
    fn parse_reads_all_items() {
        let items = parse_js(&js_a()).unwrap();
        assert_eq!(items.len(), 7);
        assert_eq!(items[0]["title"], "Face");
    }

    #[test]
    fn diff_finds_only_the_changed_curve() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let diff = compute_js_diff(a, b, 2.0);
        assert_eq!(diff.same_count, 1);
        assert_eq!(diff.changing_count(), 1);
        assert!(matches!(diff.rows[0].kind, JsDiffKind::Morph));
        assert_eq!(diff.rows[0].color, [0, 0, 255]);
        assert_eq!(diff.rows[0].label, "A_{11}");
    }

    #[test]
    fn build_adds_pingpong_slider_and_interpolation() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let diff = compute_js_diff(a, b, 3.5);
        let js = build_animated_js(&diff);
        assert!(js.contains("\"title\": \"Morph\"") || js.contains("\"title\":\"Morph\""));
        assert!(js.contains("m_{orph1}=0"));
        assert!(js.contains("LOOP_FORWARD_REVERSE"));
        assert!(js.contains("\"animationPeriod\":3500"));
        assert!(js.contains("1-m_{orph1}"));
        assert!(js.contains("[(9,9)]"));
        assert!(js.contains("[(5,5)]"));
    }

    #[test]
    fn unchanged_curve_is_not_rewritten() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let diff = compute_js_diff(a, b, 2.0);
        let js = build_animated_js(&diff);
        assert!(js.contains("A_{1}=[(0,0)]"));
    }

    #[test]
    fn disabled_row_produces_no_slider() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let mut diff = compute_js_diff(a, b, 2.0);
        diff.rows[0].enabled = false;
        let js = build_animated_js(&diff);
        assert!(!js.contains("LOOP_FORWARD_REVERSE"));
        assert!(js.contains("A_{11}=[(5,5)]"));
    }

    #[test]
    fn structure_mismatch_is_not_morphed() {
        let a = parse_js(&js_a()).unwrap();
        let b_text = wrap(&[
            r##"{ "type": "expression", "id": "8", "folderId": "2", "color": "#0000ff", "latex": "A_{11}=[(5,5),(8,8)]" }"##,
            r##"{ "type": "expression", "id": "9", "folderId": "2", "color": "#0000ff", "latex": "A_{12}=[(6,6),(9,9)]" }"##,
            r##"{ "type": "expression", "id": "10", "folderId": "2", "color": "#0000ff", "latex": "A_{13}=[(7,5),(10,8)]" }"##,
            r##"{ "type": "expression", "id": "5", "folderId": "2", "color": "#ff0000", "latex": "A_{1}=[(0,0)]" }"##,
            r##"{ "type": "expression", "id": "6", "folderId": "2", "color": "#ff0000", "latex": "A_{2}=[(1,1)]" }"##,
            r##"{ "type": "expression", "id": "7", "folderId": "2", "color": "#ff0000", "latex": "A_{3}=[(2,0)]" }"##,
        ]);
        let b = parse_js(&b_text).unwrap();
        let diff = compute_js_diff(a, b, 2.0);
        let mismatch = diff
            .rows
            .iter()
            .find(|r| r.curve_key == "A_1")
            .expect("changed curve");
        assert!(matches!(mismatch.kind, JsDiffKind::StructMismatch));
    }
}
