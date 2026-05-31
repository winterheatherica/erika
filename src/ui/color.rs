use eframe::egui;

pub(crate) fn color_edit_hex_rgb(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    rgb: &mut [u8; 3],
) -> bool {
    let mut changed = ui.color_edit_button_srgb(rgb).changed();

    let id = ui.make_persistent_id(id_salt);
    let canonical = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    let mut buf = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| canonical.clone());

    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .desired_width(64.0)
            .hint_text("#rrggbb")
            .font(egui::TextStyle::Monospace),
    );

    if resp.has_focus() || resp.lost_focus() {
        if let Some(parsed) = parse_hex_rgb(&buf) {
            if parsed != *rgb {
                *rgb = parsed;
                changed = true;
            }
        }
        ui.data_mut(|d| d.insert_temp(id, buf));
    } else {
        ui.data_mut(|d| d.insert_temp(id, canonical));
    }

    changed
}

pub(crate) fn color_edit_hex_rgba(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    rgba: &mut [u8; 4],
) -> bool {
    let mut changed = ui.color_edit_button_srgba_unmultiplied(rgba).changed();

    let id = ui.make_persistent_id(id_salt);
    let canonical = format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        rgba[0], rgba[1], rgba[2], rgba[3]
    );
    let mut buf = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| canonical.clone());

    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .desired_width(80.0)
            .hint_text("#rrggbbaa")
            .font(egui::TextStyle::Monospace),
    );

    if resp.has_focus() || resp.lost_focus() {
        if let Some(parsed) = parse_hex_rgba(&buf, rgba[3]) {
            if parsed != *rgba {
                *rgba = parsed;
                changed = true;
            }
        }
        ui.data_mut(|d| d.insert_temp(id, buf));
    } else {
        ui.data_mut(|d| d.insert_temp(id, canonical));
    }

    changed
}

fn parse_hex_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    match s.len() {
        3 => {
            let n = u16::from_str_radix(s, 16).ok()?;
            let r = ((n >> 8) & 0xf) as u8;
            let g = ((n >> 4) & 0xf) as u8;
            let b = (n & 0xf) as u8;
            Some([r * 17, g * 17, b * 17])
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some([r, g, b])
        }
        _ => None,
    }
}

fn parse_hex_rgba(s: &str, fallback_alpha: u8) -> Option<[u8; 4]> {
    let t = s.trim().trim_start_matches('#');
    match t.len() {
        3 | 6 => parse_hex_rgb(s).map(|[r, g, b]| [r, g, b, fallback_alpha]),
        4 => {
            let n = u16::from_str_radix(t, 16).ok()?;
            let r = ((n >> 12) & 0xf) as u8;
            let g = ((n >> 8) & 0xf) as u8;
            let b = ((n >> 4) & 0xf) as u8;
            let a = (n & 0xf) as u8;
            Some([r * 17, g * 17, b * 17, a * 17])
        }
        8 => {
            let r = u8::from_str_radix(&t[0..2], 16).ok()?;
            let g = u8::from_str_radix(&t[2..4], 16).ok()?;
            let b = u8::from_str_radix(&t[4..6], 16).ok()?;
            let a = u8::from_str_radix(&t[6..8], 16).ok()?;
            Some([r, g, b, a])
        }
        _ => None,
    }
}
