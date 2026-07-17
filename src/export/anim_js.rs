use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

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
    pub enabled: bool,
}

pub struct JsDiff {
    pub items_a: Vec<Value>,
    pub keyframes: usize,
    pub to_rhs: HashMap<String, Vec<String>>,
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

pub fn load_js_seq(paths: &[PathBuf]) -> Result<JsDiff, String> {
    if paths.len() < 2 {
        return Err("Pick at least two keyframes".to_string());
    }
    let mut files = Vec::with_capacity(paths.len());
    for p in paths {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("?");
        let text = std::fs::read_to_string(p).map_err(|e| format!("read {name}: {e}"))?;
        files.push(parse_js(&text).map_err(|e| format!("{name}: {e}"))?);
    }
    Ok(compute_js_diff(files))
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

pub fn compute_js_diff(files: Vec<Vec<Value>>) -> JsDiff {
    let keyframes = files.len();
    if keyframes == 0 {
        return JsDiff {
            items_a: Vec::new(),
            keyframes: 0,
            to_rhs: HashMap::new(),
            rows: Vec::new(),
            same_count: 0,
            added: 0,
            removed: 0,
        };
    }

    let lists: Vec<BTreeMap<String, (String, [u8; 3])>> =
        files.iter().map(|f| data_lists(f)).collect();
    let curves: Vec<BTreeMap<String, Vec<String>>> = lists.iter().map(curves_of).collect();
    let first_lists = &lists[0];
    let first_curves = &curves[0];
    let later_lists = &lists[1..];
    let later_curves = &curves[1..];

    let mut rows = Vec::new();
    let mut to_rhs: HashMap<String, Vec<String>> = HashMap::new();
    let mut same_count = 0;
    let mut removed = 0;

    for (key, vars) in first_curves {
        if !later_curves.iter().all(|c| c.contains_key(key)) {
            removed += 1;
            continue;
        }
        let mut any_diff = false;
        let mut morphable = true;
        for var in vars {
            let (a_rhs, _) = &first_lists[var];
            let a_pts = count_pts(a_rhs);
            for l in later_lists {
                match l.get(var) {
                    Some((rhs, _)) => {
                        if rhs != a_rhs {
                            any_diff = true;
                        }
                        if count_pts(rhs) != a_pts {
                            morphable = false;
                        }
                    }
                    None => {
                        any_diff = true;
                        morphable = false;
                    }
                }
            }
        }
        if !any_diff {
            same_count += 1;
            continue;
        }
        let color = first_lists[&vars[0]].1;
        let label = vars.iter().min().cloned().unwrap_or_else(|| key.clone());
        let kind = if morphable {
            for var in vars {
                let seq: Vec<String> = later_lists.iter().map(|l| l[var].0.clone()).collect();
                to_rhs.insert(var.clone(), seq);
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
            enabled: true,
        });
    }

    let a_keys: BTreeSet<&String> = first_curves.keys().collect();
    let mut added_keys: BTreeSet<&String> = BTreeSet::new();
    for c in later_curves {
        for k in c.keys() {
            if !a_keys.contains(k) {
                added_keys.insert(k);
            }
        }
    }
    let added = added_keys.len();

    let items_a = files.into_iter().next().unwrap_or_default();

    JsDiff {
        items_a,
        keyframes,
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

fn fmt_secs(v: f32) -> String {
    let mut s = format!("{v:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

pub fn build_animated_js(diff: &JsDiff, durs: &[f32]) -> String {
    let mut items = diff.items_a.clone();
    let n_seg = diff.keyframes.saturating_sub(1);
    let seg_dur = |i: usize| durs.get(i).copied().unwrap_or(2.0).max(0.1);

    let enabled: BTreeSet<&str> = diff
        .rows
        .iter()
        .filter(|r| matches!(r.kind, JsDiffKind::Morph) && r.enabled)
        .map(|r| r.curve_key.as_str())
        .collect();

    let mut any_morph = false;
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
            if !enabled.contains(key.as_str()) {
                continue;
            }
            let Some(seq) = diff.to_rhs.get(var) else {
                continue;
            };
            let mut vals: Vec<&str> = Vec::with_capacity(seq.len() + 1);
            vals.push(from_rhs);
            vals.extend(seq.iter().map(|s| s.as_str()));
            let mut out = format!("{var}={from_rhs}");
            let mut start = 0.0f32;
            for i in 0..vals.len().saturating_sub(1) {
                let d = seg_dur(i);
                let frac = if i == 0 {
                    format!("\\frac{{m_{{orph}}}}{{{}}}", fmt_secs(d))
                } else {
                    format!("\\frac{{m_{{orph}}-{}}}{{{}}}", fmt_secs(start), fmt_secs(d))
                };
                out.push_str(&format!(
                    "+\\min\\left(\\max\\left({frac},0\\right),1\\right)\\cdot\\left({}-{}\\right)",
                    vals[i + 1],
                    vals[i]
                ));
                start += d;
            }
            out
        };
        any_morph = true;
        if let Some(obj) = it.as_object_mut() {
            obj.insert("latex".to_string(), Value::String(replacement));
        }
    }

    if any_morph && n_seg > 0 {
        let total: f32 = (0..n_seg).map(seg_dur).sum();
        let mut next = max_id(&items) + 1;
        let folder_id = next.to_string();
        next += 1;
        items.push(json!({ "type": "folder", "id": folder_id, "title": "Morph" }));
        let period = (total * 1000.0).round() as u64;
        items.push(json!({
            "type": "expression",
            "id": next.to_string(),
            "folderId": folder_id,
            "color": "#000000",
            "latex": "m_{orph}=0",
            "slider": {
                "hardMin": true,
                "hardMax": true,
                "min": "0",
                "max": fmt_secs(total),
                "loopMode": "LOOP_FORWARD_REVERSE",
                "isPlaying": true,
                "animationPeriod": period
            }
        }));
    }

    render_js_values(&items)
}

fn render_js_values(items: &[Value]) -> String {
    let mut out = String::new();
    out.push_str("// Erika -> Desmos morph export. Paste into the browser console on a Desmos graph.\n");
    out.push_str("// Requires the global `Calc` (open https://www.desmos.com/calculator).\n");
    out.push_str("// The m_orph slider auto-plays (ping-pong) and morphs each changed line through the keyframes.\n");
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

    fn js_c() -> String {
        wrap(&[
            r##"{ "type": "folder", "id": "2", "title": "Face" }"##,
            r##"{ "type": "expression", "id": "5", "folderId": "2", "color": "#ff0000", "latex": "A_{1}=[(0,0)]" }"##,
            r##"{ "type": "expression", "id": "6", "folderId": "2", "color": "#ff0000", "latex": "A_{2}=[(1,1)]" }"##,
            r##"{ "type": "expression", "id": "7", "folderId": "2", "color": "#ff0000", "latex": "A_{3}=[(2,0)]" }"##,
            r##"{ "type": "expression", "id": "8", "folderId": "2", "color": "#0000ff", "latex": "A_{11}=[(1,1)]" }"##,
            r##"{ "type": "expression", "id": "9", "folderId": "2", "color": "#0000ff", "latex": "A_{12}=[(2,2)]" }"##,
            r##"{ "type": "expression", "id": "10", "folderId": "2", "color": "#0000ff", "latex": "A_{13}=[(3,1)]" }"##,
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
        let diff = compute_js_diff(vec![a, b]);
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
        let diff = compute_js_diff(vec![a, b]);
        let js = build_animated_js(&diff, &[3.5]);
        assert!(js.contains("\"title\": \"Morph\"") || js.contains("\"title\":\"Morph\""));
        assert!(js.contains("m_{orph}=0"));
        assert!(js.contains("LOOP_FORWARD_REVERSE"));
        assert!(js.contains("\"animationPeriod\":3500"));
        assert!(js.contains("\"max\":\"3.5\""));
        assert!(js.contains("\\\\frac{m_{orph}}{3.5}"));
        assert!(js.contains("[(9,9)]"));
        assert!(js.contains("[(5,5)]"));
    }

    #[test]
    fn three_keyframes_use_per_step_durations() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let c = parse_js(&js_c()).unwrap();
        let diff = compute_js_diff(vec![a, b, c]);
        assert_eq!(diff.keyframes, 3);
        assert_eq!(diff.same_count, 1);
        assert_eq!(diff.changing_count(), 1);
        let js = build_animated_js(&diff, &[1.0, 2.5]);
        assert!(js.contains("\\\\frac{m_{orph}}{1}"));
        assert!(js.contains("\\\\frac{m_{orph}-1}{2.5}"));
        assert!(js.contains("\\\\left([(9,9)]-[(5,5)]\\\\right)"));
        assert!(js.contains("\\\\left([(1,1)]-[(9,9)]\\\\right)"));
        assert!(js.contains("\"animationPeriod\":3500"));
        assert!(js.contains("\"max\":\"3.5\""));
    }

    #[test]
    fn unchanged_curve_is_not_rewritten() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let diff = compute_js_diff(vec![a, b]);
        let js = build_animated_js(&diff, &[2.0]);
        assert!(js.contains("A_{1}=[(0,0)]"));
    }

    #[test]
    fn disabled_row_produces_no_slider() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let mut diff = compute_js_diff(vec![a, b]);
        diff.rows[0].enabled = false;
        let js = build_animated_js(&diff, &[2.0]);
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
        let diff = compute_js_diff(vec![a, b]);
        let mismatch = diff
            .rows
            .iter()
            .find(|r| r.curve_key == "A_1")
            .expect("changed curve");
        assert!(matches!(mismatch.kind, JsDiffKind::StructMismatch));
    }

    #[test]
    fn curve_missing_in_a_later_keyframe_is_not_animated() {
        let a = parse_js(&js_a()).unwrap();
        let b = parse_js(&js_b()).unwrap();
        let c_text = wrap(&[
            r##"{ "type": "expression", "id": "5", "folderId": "2", "color": "#ff0000", "latex": "A_{1}=[(0,0)]" }"##,
            r##"{ "type": "expression", "id": "6", "folderId": "2", "color": "#ff0000", "latex": "A_{2}=[(1,1)]" }"##,
            r##"{ "type": "expression", "id": "7", "folderId": "2", "color": "#ff0000", "latex": "A_{3}=[(2,0)]" }"##,
        ]);
        let c = parse_js(&c_text).unwrap();
        let diff = compute_js_diff(vec![a, b, c]);
        assert_eq!(diff.removed, 1);
        assert!(diff.rows.iter().all(|r| r.curve_key != "A_1"));
    }
}
