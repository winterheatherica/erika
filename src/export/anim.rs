use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElemTag {
    Path,
    Ellipse,
}

pub struct SvgElement {
    pub raw: String,
    pub tag: ElemTag,
    pub d: Option<String>,
    pub geom: String,
    pub morph_sig: String,
    pub color: [u8; 3],
}

pub struct SvgDoc {
    pub svg_open: String,
    pub background: Option<String>,
    pub elements: Vec<SvgElement>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Same,
    Morph,
    Crossfade,
    AddedInB,
    RemovedFromA,
}

impl DiffKind {
    pub fn label(self) -> &'static str {
        match self {
            DiffKind::Same => "identical",
            DiffKind::Morph => "morph",
            DiffKind::Crossfade => "cross-fade",
            DiffKind::AddedInB => "added (fade in)",
            DiffKind::RemovedFromA => "removed (fade out)",
        }
    }
}

pub struct AnimRow {
    pub index: usize,
    pub kind: DiffKind,
    pub color: [u8; 3],
    pub label: String,
    pub dur: f32,
    pub enabled: bool,
}

pub struct AnimDiff {
    pub doc_a: SvgDoc,
    pub doc_b: SvgDoc,
    pub same_count: usize,
    pub rows: Vec<AnimRow>,
}

impl AnimDiff {
    pub fn changing_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| !matches!(r.kind, DiffKind::Same))
            .count()
    }
}

pub fn list_svg_files(dir: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("svg"))
        .collect();
    out.sort();
    out
}

pub fn load_diff(a: &Path, b: &Path, default_dur: f32) -> Result<AnimDiff, String> {
    let text_a = std::fs::read_to_string(a).map_err(|e| format!("read A: {e}"))?;
    let text_b = std::fs::read_to_string(b).map_err(|e| format!("read B: {e}"))?;
    let doc_a = parse_svg(&text_a)?;
    let doc_b = parse_svg(&text_b)?;
    Ok(compute_diff(doc_a, doc_b, default_dur))
}

pub fn parse_svg(text: &str) -> Result<SvgDoc, String> {
    let svg_start = text.find("<svg").ok_or("No <svg> tag found")?;
    let open_end = text[svg_start..]
        .find('>')
        .map(|i| i + svg_start)
        .ok_or("Malformed <svg> tag")?;
    let svg_open = text[svg_start..=open_end].to_string();

    let body_start = open_end + 1;
    let body_end = text[body_start..]
        .find("</svg>")
        .map(|i| i + body_start)
        .unwrap_or(text.len());
    let mut rest = &text[body_start..body_end];

    let mut background = None;
    let mut elements = Vec::new();

    while let Some(lt) = rest.find('<') {
        let after = &rest[lt..];
        let Some(close) = after.find("/>") else {
            break;
        };
        let elem = &after[..close + 2];
        if elem.starts_with("<rect") {
            if background.is_none() {
                background = Some(elem.to_string());
            }
        } else if elem.starts_with("<path") || elem.starts_with("<ellipse") {
            elements.push(parse_elem(elem));
        }
        rest = &after[close + 2..];
    }

    Ok(SvgDoc {
        svg_open,
        background,
        elements,
    })
}

fn parse_elem(elem: &str) -> SvgElement {
    let tag = if elem.starts_with("<ellipse") {
        ElemTag::Ellipse
    } else {
        ElemTag::Path
    };
    let d = attr(elem, "d").map(|s| s.to_string());
    let (geom, morph_sig) = match tag {
        ElemTag::Path => {
            let dv = d.clone().unwrap_or_default();
            let sig: String = dv.chars().filter(|c| c.is_ascii_alphabetic()).collect();
            (dv, sig)
        }
        ElemTag::Ellipse => {
            let g = ["cx", "cy", "rx", "ry", "transform"]
                .iter()
                .filter_map(|k| attr(elem, k))
                .collect::<Vec<_>>()
                .join("|");
            (g, String::new())
        }
    };
    SvgElement {
        raw: elem.to_string(),
        tag,
        d,
        geom,
        morph_sig,
        color: parse_color(elem),
    }
}

fn attr<'a>(elem: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = elem.find(&key)? + key.len();
    let end = elem[start..].find('"')? + start;
    Some(&elem[start..end])
}

fn parse_color(elem: &str) -> [u8; 3] {
    let pick = |name: &str| -> Option<[u8; 3]> {
        attr(elem, name).filter(|v| *v != "none").and_then(parse_hex)
    };
    pick("stroke").or_else(|| pick("fill")).unwrap_or([90, 90, 90])
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

pub fn compute_diff(doc_a: SvgDoc, doc_b: SvgDoc, default_dur: f32) -> AnimDiff {
    let na = doc_a.elements.len();
    let nb = doc_b.elements.len();
    let shared = na.min(nb);
    let mut rows = Vec::new();
    let mut same_count = 0;

    for i in 0..shared {
        let ea = &doc_a.elements[i];
        let eb = &doc_b.elements[i];
        let kind = if ea.geom == eb.geom {
            same_count += 1;
            DiffKind::Same
        } else if ea.tag == ElemTag::Path
            && eb.tag == ElemTag::Path
            && ea.morph_sig == eb.morph_sig
        {
            DiffKind::Morph
        } else {
            DiffKind::Crossfade
        };
        rows.push(AnimRow {
            index: i,
            kind,
            color: ea.color,
            label: line_label(ea, i),
            dur: default_dur,
            enabled: true,
        });
    }

    for i in shared..na {
        let ea = &doc_a.elements[i];
        rows.push(AnimRow {
            index: i,
            kind: DiffKind::RemovedFromA,
            color: ea.color,
            label: line_label(ea, i),
            dur: default_dur,
            enabled: true,
        });
    }

    for i in shared..nb {
        let eb = &doc_b.elements[i];
        rows.push(AnimRow {
            index: i,
            kind: DiffKind::AddedInB,
            color: eb.color,
            label: line_label(eb, i),
            dur: default_dur,
            enabled: true,
        });
    }

    AnimDiff {
        doc_a,
        doc_b,
        same_count,
        rows,
    }
}

fn line_label(e: &SvgElement, index: usize) -> String {
    let tag = match e.tag {
        ElemTag::Path => "path",
        ElemTag::Ellipse => "ellipse",
    };
    format!("{tag} #{}", index + 1)
}

pub fn build_animated_svg(diff: &AnimDiff) -> String {
    let a = &diff.doc_a;
    let b = &diff.doc_b;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&a.svg_open);
    out.push('\n');
    if let Some(bg) = &a.background {
        out.push_str("  ");
        out.push_str(bg);
        out.push('\n');
    }

    for row in &diff.rows {
        match row.kind {
            DiffKind::Same => emit_static(&mut out, &a.elements[row.index].raw),
            DiffKind::Morph => {
                if row.enabled {
                    emit_morph(
                        &mut out,
                        &a.elements[row.index],
                        &b.elements[row.index],
                        row.dur,
                    );
                } else {
                    emit_static(&mut out, &a.elements[row.index].raw);
                }
            }
            DiffKind::Crossfade => {
                if row.enabled {
                    emit_opacity(&mut out, &a.elements[row.index].raw, row.dur, "1;0;1");
                    emit_opacity(&mut out, &b.elements[row.index].raw, row.dur, "0;1;0");
                } else {
                    emit_static(&mut out, &a.elements[row.index].raw);
                }
            }
            DiffKind::AddedInB => {
                if row.enabled {
                    emit_opacity(&mut out, &b.elements[row.index].raw, row.dur, "0;1;0");
                } else {
                    emit_static(&mut out, &b.elements[row.index].raw);
                }
            }
            DiffKind::RemovedFromA => {
                if row.enabled {
                    emit_opacity(&mut out, &a.elements[row.index].raw, row.dur, "1;0;1");
                } else {
                    emit_static(&mut out, &a.elements[row.index].raw);
                }
            }
        }
    }

    out.push_str("</svg>\n");
    out
}

fn emit_static(out: &mut String, raw: &str) {
    out.push_str("  ");
    out.push_str(raw);
    out.push('\n');
}

fn open_tag(raw: &str) -> &str {
    raw.trim_end()
        .strip_suffix("/>")
        .unwrap_or(raw)
        .trim_end()
}

fn tag_name(raw: &str) -> &str {
    if raw.starts_with("<ellipse") {
        "ellipse"
    } else {
        "path"
    }
}

fn emit_morph(out: &mut String, a: &SvgElement, b: &SvgElement, dur: f32) {
    let from = a.d.as_deref().unwrap_or("");
    let to = b.d.as_deref().unwrap_or("");
    out.push_str("  ");
    out.push_str(open_tag(&a.raw));
    out.push_str(">\n");
    out.push_str(&format!(
        "    <animate attributeName=\"d\" dur=\"{:.3}s\" repeatCount=\"indefinite\" calcMode=\"linear\" keyTimes=\"0;0.5;1\" values=\"{};{};{}\"/>\n",
        dur.max(0.01),
        from,
        to,
        from
    ));
    out.push_str("  </path>\n");
}

fn emit_opacity(out: &mut String, raw: &str, dur: f32, values: &str) {
    out.push_str("  ");
    out.push_str(open_tag(raw));
    out.push_str(">\n");
    out.push_str(&format!(
        "    <animate attributeName=\"opacity\" dur=\"{:.3}s\" repeatCount=\"indefinite\" calcMode=\"linear\" keyTimes=\"0;0.5;1\" values=\"{}\"/>\n",
        dur.max(0.01),
        values
    ));
    out.push_str(&format!("  </{}>\n", tag_name(raw)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG_A: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <rect width="100%" height="100%" fill="#f5f5fa"/>
  <path d="M 0.000 0.000 Q 1.000 1.000 2.000 0.000" stroke="#ff0000" stroke-width="2.500" fill="none"/>
  <path d="M 10.000 10.000 Q 11.000 11.000 12.000 10.000" stroke="#00ff00" stroke-width="2.500" fill="none"/>
</svg>
"##;

    const SVG_B: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <rect width="100%" height="100%" fill="#f5f5fa"/>
  <path d="M 0.000 0.000 Q 1.000 1.000 2.000 0.000" stroke="#ff0000" stroke-width="2.500" fill="none"/>
  <path d="M 50.000 50.000 Q 51.000 51.000 52.000 50.000" stroke="#00ff00" stroke-width="2.500" fill="none"/>
</svg>
"##;

    #[test]
    fn parse_extracts_paths_and_background() {
        let doc = parse_svg(SVG_A).unwrap();
        assert!(doc.background.is_some());
        assert_eq!(doc.elements.len(), 2);
        assert_eq!(doc.elements[0].tag, ElemTag::Path);
        assert!(doc.elements[0].d.as_deref().unwrap().starts_with("M 0.000"));
        assert_eq!(doc.elements[0].color, [255, 0, 0]);
    }

    #[test]
    fn diff_keeps_same_and_morphs_changed() {
        let a = parse_svg(SVG_A).unwrap();
        let b = parse_svg(SVG_B).unwrap();
        let diff = compute_diff(a, b, 2.0);
        assert_eq!(diff.same_count, 1);
        assert_eq!(diff.changing_count(), 1);
        assert!(matches!(diff.rows[0].kind, DiffKind::Same));
        assert!(matches!(diff.rows[1].kind, DiffKind::Morph));
    }

    #[test]
    fn structure_mismatch_falls_back_to_crossfade() {
        let a = parse_svg(SVG_A).unwrap();
        let mut b = parse_svg(SVG_B).unwrap();
        b.elements[1] = parse_elem(
            r##"<path d="M 0.000 0.000 Q 1.000 1.000 2.000 0.000 Q 3.000 3.000 4.000 0.000" stroke="#00ff00"/>"##,
        );
        let diff = compute_diff(a, b, 2.0);
        assert!(matches!(diff.rows[1].kind, DiffKind::Crossfade));
    }

    #[test]
    fn build_contains_ping_pong_morph() {
        let a = parse_svg(SVG_A).unwrap();
        let b = parse_svg(SVG_B).unwrap();
        let diff = compute_diff(a, b, 3.5);
        let svg = build_animated_svg(&diff);
        assert!(svg.contains("<animate attributeName=\"d\""));
        assert!(svg.contains("dur=\"3.500s\""));
        assert!(svg.contains("repeatCount=\"indefinite\""));
        assert!(svg.contains("keyTimes=\"0;0.5;1\""));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn disabled_row_emits_static() {
        let a = parse_svg(SVG_A).unwrap();
        let b = parse_svg(SVG_B).unwrap();
        let mut diff = compute_diff(a, b, 2.0);
        diff.rows[1].enabled = false;
        let svg = build_animated_svg(&diff);
        assert!(!svg.contains("<animate"));
    }
}
