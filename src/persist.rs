use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::curve::CurveSet;
use crate::image_ref::ReferenceImage;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct CameraState {
    pub center_x: f32,
    pub center_y: f32,
    pub scale: f32,
}

#[derive(Serialize, Deserialize)]
pub struct Project {
    #[serde(default = "default_version")]
    pub version: u32,
    pub curves: Vec<CurveSet>,
    #[serde(default)]
    pub reference_image: Option<ReferenceImage>,
    pub camera: CameraState,
    #[serde(default = "default_samples")]
    pub samples_per_segment: usize,
    #[serde(default = "default_bg")]
    pub background: [u8; 3],
}

fn default_version() -> u32 {
    1
}
fn default_samples() -> usize {
    32
}
fn default_bg() -> [u8; 3] {
    [245, 245, 250]
}

impl Project {
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }
}
