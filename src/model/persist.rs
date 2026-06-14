use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::curve::{CurveSet, Group};
use crate::model::image_ref::ReferenceImage;

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
    #[serde(default)]
    pub reference_images: Vec<ReferenceImage>,
    pub camera: CameraState,
    #[serde(default = "default_samples")]
    pub samples_per_segment: usize,
    #[serde(default = "default_bg")]
    pub background: Option<[u8; 3]>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub export: ExportSettings,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ExportSettings {
    #[serde(default = "default_export_name")]
    pub name: String,
    #[serde(default = "default_export_dim")]
    pub width: u32,
    #[serde(default = "default_export_dim")]
    pub height: u32,
    #[serde(default)]
    pub transparent: bool,
    #[serde(default = "default_export_samples")]
    pub samples: usize,
    #[serde(default)]
    pub timelapse: bool,
    #[serde(default)]
    pub frame: Option<[f32; 4]>,
    #[serde(default)]
    pub frame_lock: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            name: default_export_name(),
            width: default_export_dim(),
            height: default_export_dim(),
            transparent: false,
            samples: default_export_samples(),
            timelapse: false,
            frame: None,
            frame_lock: false,
        }
    }
}

fn default_export_name() -> String {
    "output".to_string()
}
fn default_export_dim() -> u32 {
    1024
}
fn default_export_samples() -> usize {
    64
}

fn default_version() -> u32 {
    1
}
fn default_samples() -> usize {
    32
}
fn default_bg() -> Option<[u8; 3]> {
    Some([245, 245, 250])
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
