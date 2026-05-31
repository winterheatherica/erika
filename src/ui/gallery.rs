use std::path::{Path, PathBuf};

use eframe::egui;

use crate::app::App;
use crate::export::png::render_pixmap;
use crate::model::persist::Project;

const THUMB_W: u32 = 240;
const THUMB_H: u32 = 160;

struct Card {
    path: PathBuf,
    title: String,
    date: String,
    tex: Option<egui::TextureHandle>,
    is_current: bool,
}

impl App {
    pub(crate) fn gallery_window(&mut self, ctx: &egui::Context) {
        let current = self.current_project_path.clone();
        let mut cards: Vec<Card> = Vec::new();
        for p in self.saved_projects() {
            let title = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let date = file_date(&p);
            let tex = self.ensure_thumb(ctx, &p);
            let is_current = current.as_deref() == Some(p.as_path());
            cards.push(Card {
                path: p,
                title,
                date,
                tex,
                is_current,
            });
        }

        let mut open = true;
        let mut to_load: Option<PathBuf> = None;
        let mut browse = false;
        let search = &mut self.gallery_search;

        egui::Window::new("Saved Work")
            .open(&mut open)
            .resizable(true)
            .default_size([760.0, 540.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.add(
                        egui::TextEdit::singleline(search)
                            .hint_text("title")
                            .desired_width(220.0),
                    );
                    if !search.is_empty() && ui.button("✖").clicked() {
                        search.clear();
                    }
                    if ui.button("Browse…").clicked() {
                        browse = true;
                    }
                });
                ui.separator();

                let query = search.to_lowercase();
                let visible: Vec<&Card> = cards
                    .iter()
                    .filter(|c| query.is_empty() || c.title.to_lowercase().contains(&query))
                    .collect();

                if visible.is_empty() {
                    ui.label(egui::RichText::new("No saved projects in ./project").weak());
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let card_w = 184.0;
                        let tw = card_w - 16.0;
                        let th = tw * THUMB_H as f32 / THUMB_W as f32;
                        for card in &visible {
                            ui.allocate_ui(egui::vec2(card_w, th + 64.0), |ui| {
                                egui::Frame::group(ui.style()).show(ui, |ui| {
                                    ui.set_width(tw);
                                    let size = egui::vec2(tw, th);
                                    let clicked = match &card.tex {
                                        Some(tex) => {
                                            let src =
                                                egui::load::SizedTexture::new(tex.id(), size);
                                            ui.add(egui::ImageButton::new(egui::Image::new(src)))
                                                .clicked()
                                        }
                                        None => ui
                                            .add_sized(size, egui::Button::new("no preview"))
                                            .clicked(),
                                    };
                                    if clicked {
                                        to_load = Some(card.path.clone());
                                    }
                                    let title = if card.is_current {
                                        format!("● {}", card.title)
                                    } else {
                                        card.title.clone()
                                    };
                                    ui.label(egui::RichText::new(title).strong());
                                    ui.label(egui::RichText::new(&card.date).small().weak());
                                });
                            });
                        }
                    });
                });
            });

        self.show_gallery = open;
        if browse {
            self.load_dialog();
            self.show_gallery = false;
        }
        if let Some(path) = to_load {
            self.load_project(path);
            self.show_gallery = false;
        }
    }
    
    fn ensure_thumb(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
        if let Some(cached) = self.thumb_cache.get(path) {
            return cached.clone();
        }
        let tex = render_thumb(ctx, path);
        self.thumb_cache.insert(path.to_path_buf(), tex.clone());
        tex
    }
}

fn render_thumb(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    let proj = Project::load_from(path).ok()?;
    let bg = proj.background.unwrap_or([255, 255, 255]);
    let pixmap = render_pixmap(&proj.curves, THUMB_W, THUMB_H, Some(bg), 0.08, 12).ok()?;
    let image =
        egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], pixmap.data());
    Some(ctx.load_texture(
        format!("thumb:{}", path.display()),
        image,
        egui::TextureOptions::LINEAR,
    ))
}

fn file_date(path: &Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return String::new();
    };
    let Ok(modified) = meta.modified() else {
        return String::new();
    };
    let dt = chrono::DateTime::<chrono::Utc>::from(modified).with_timezone(&chrono::Local);
    dt.format("%b %d, %Y").to_string()
}
