use std::path::{Path, PathBuf};

use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_opacity() -> f32 {
    0.5
}
fn default_true() -> bool {
    true
}
fn default_aspect() -> f32 {
    1.0
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ReferenceImage {
    pub path: PathBuf,
    pub world_x: f32,
    pub world_y: f32,
    pub world_w: f32,
    pub world_h: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_aspect")]
    pub aspect: f32,

    #[serde(skip)]
    pub texture: Option<egui::TextureHandle>,
    #[serde(skip)]
    pub load_error: Option<String>,

    #[serde(skip)]
    pub raw_rgba: Option<Vec<u8>>,
    #[serde(skip)]
    pub raw_w: u32,
    #[serde(skip)]
    pub raw_h: u32,
}

impl ReferenceImage {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            world_x: 0.0,
            world_y: 0.0,
            world_w: 0.0,
            world_h: 0.0,
            opacity: 0.5,
            visible: true,
            locked: false,
            aspect: 1.0,
            texture: None,
            load_error: None,
            raw_rgba: None,
            raw_w: 0,
            raw_h: 0,
        }
    }

    pub fn ensure_loaded(&mut self, ctx: &egui::Context, default_world_w: f32) {
        if self.texture.is_some() {
            return;
        }
        if self.load_error.is_some() {
            return;
        }
        let fresh = self.world_w <= 0.0 && self.world_h <= 0.0;

        match load_image_file(&self.path) {
            Ok((ci, rgba_bytes, w, h)) => {
                let tex = ctx.load_texture(
                    format!("ref:{}", self.path.display()),
                    ci,
                    egui::TextureOptions::LINEAR,
                );
                let nat_aspect = w as f32 / h.max(1) as f32;
                self.aspect = nat_aspect;
                if fresh {
                    self.world_w = default_world_w.max(1.0);
                    self.world_h = self.world_w / nat_aspect.max(1e-3);
                    self.world_x = -self.world_w * 0.5;
                    self.world_y = -self.world_h * 0.5;
                } else {
                    if self.world_w <= 0.0 {
                        self.world_w = default_world_w.max(1.0);
                    }
                    if self.world_h <= 0.0 {
                        self.world_h = self.world_w / nat_aspect.max(1e-3);
                    }
                }
                self.raw_rgba = Some(rgba_bytes);
                self.raw_w = w;
                self.raw_h = h;
                self.texture = Some(tex);
            }
            Err(e) => {
                self.load_error = Some(e);
            }
        }
    }

    pub fn is_ready(&self) -> bool {
        self.texture.is_some() && self.world_w > 0.0 && self.world_h > 0.0
    }

    pub fn fix_aspect(&mut self) {
        if self.aspect > 0.0 {
            self.world_h = self.world_w / self.aspect;
        }
    }

    pub fn sample_at_world(&self, wx: f32, wy: f32) -> Option<[u8; 4]> {
        if self.raw_rgba.is_none() || self.raw_w == 0 || self.raw_h == 0 {
            return None;
        }
        if self.world_w <= 0.0 || self.world_h <= 0.0 {
            return None;
        }
        let u = (wx - self.world_x) / self.world_w;
        let v = 1.0 - (wy - self.world_y) / self.world_h;
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let px = ((u * self.raw_w as f32) as u32).min(self.raw_w - 1);
        let py = ((v * self.raw_h as f32) as u32).min(self.raw_h - 1);
        let idx = ((py * self.raw_w + px) * 4) as usize;
        let rgba = self.raw_rgba.as_ref()?;
        if idx + 3 >= rgba.len() {
            return None;
        }
        Some([rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]])
    }
}

fn load_image_file(path: &Path) -> Result<(egui::ColorImage, Vec<u8>, u32, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Read failed: {e}"))?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("Decode failed: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.as_raw().clone();
    let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &raw);
    Ok((ci, raw, w, h))
}
