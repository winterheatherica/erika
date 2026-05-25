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
            Ok((ci, w, h)) => {
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
}

fn load_image_file(path: &Path) -> Result<(egui::ColorImage, u32, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Read failed: {e}"))?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("Decode failed: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
    Ok((ci, w, h))
}
