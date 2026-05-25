use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct P {
    pub x: f32,
    pub y: f32,
}

impl P {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arr {
    S1,
    S2,
    S3,
}

impl Arr {
    pub fn label(self) -> &'static str {
        match self {
            Arr::S1 => "S1",
            Arr::S2 => "S2",
            Arr::S3 => "S3",
        }
    }
    pub fn all() -> [Arr; 3] {
        [Arr::S1, Arr::S2, Arr::S3]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    DashDot,
    LongDash,
}

impl LineStyle {
    pub fn label(self) -> &'static str {
        match self {
            LineStyle::Solid => "Solid",
            LineStyle::Dashed => "Dashed",
            LineStyle::Dotted => "Dotted",
            LineStyle::DashDot => "Dash-Dot",
            LineStyle::LongDash => "Long Dash",
        }
    }
    pub fn all() -> [LineStyle; 5] {
        [
            LineStyle::Solid,
            LineStyle::Dashed,
            LineStyle::Dotted,
            LineStyle::DashDot,
            LineStyle::LongDash,
        ]
    }
    pub fn pattern(self) -> &'static [f32] {
        match self {
            LineStyle::Solid => &[],
            LineStyle::Dashed => &[12.0, 6.0],
            LineStyle::Dotted => &[1.5, 5.0],
            LineStyle::DashDot => &[12.0, 5.0, 1.5, 5.0],
            LineStyle::LongDash => &[24.0, 8.0],
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_thickness() -> f32 {
    2.5
}
fn default_cp_thickness() -> f32 {
    1.0
}
fn default_fill_color() -> [u8; 4] {
    [80, 160, 255, 60]
}
fn default_line_style() -> LineStyle {
    LineStyle::Solid
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurveSet {
    pub name: String,
    pub s1: Vec<P>,
    pub s2: Vec<P>,
    pub s3: Vec<P>,
    pub color: [u8; 3],
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_true")]
    pub show_handles: bool,
    #[serde(default)]
    pub show_control_poly: bool,
    #[serde(default = "default_thickness")]
    pub thickness: f32,
    #[serde(default = "default_cp_thickness")]
    pub control_poly_thickness: f32,
    #[serde(default = "default_true")]
    pub stroke_visible: bool,
    #[serde(default = "default_line_style")]
    pub line_style: LineStyle,
    #[serde(default)]
    pub fill_enabled: bool,
    #[serde(default = "default_fill_color")]
    pub fill_color: [u8; 4],
    #[serde(default)]
    pub active_segment: usize,
}

impl CurveSet {
    pub fn empty(name: impl Into<String>, color: [u8; 3]) -> Self {
        Self {
            name: name.into(),
            s1: Vec::new(),
            s2: Vec::new(),
            s3: Vec::new(),
            color,
            visible: true,
            show_handles: true,
            show_control_poly: false,
            thickness: 2.5,
            control_poly_thickness: 1.0,
            stroke_visible: true,
            line_style: LineStyle::Solid,
            fill_enabled: false,
            fill_color: [color[0], color[1], color[2], 60],
            active_segment: 0,
        }
    }

    pub fn n(&self) -> usize {
        self.s1.len().min(self.s2.len()).min(self.s3.len())
    }

    pub fn get(&self, a: Arr) -> &Vec<P> {
        match a {
            Arr::S1 => &self.s1,
            Arr::S2 => &self.s2,
            Arr::S3 => &self.s3,
        }
    }

    pub fn get_mut(&mut self, a: Arr) -> &mut Vec<P> {
        match a {
            Arr::S1 => &mut self.s1,
            Arr::S2 => &mut self.s2,
            Arr::S3 => &mut self.s3,
        }
    }

    pub fn eval_segment(&self, i: usize, j: f32) -> P {
        let p0 = self.s1[i];
        let p1 = self.s2[i];
        let p2 = self.s3[i];
        let u = 1.0 - j;
        P::new(
            u * u * p0.x + 2.0 * u * j * p1.x + j * j * p2.x,
            u * u * p0.y + 2.0 * u * j * p1.y + j * j * p2.y,
        )
    }

    pub fn append_segment(&mut self) {
        if self.s1.is_empty() && self.s2.is_empty() && self.s3.is_empty() {
            self.s1.push(P::new(0.0, 0.0));
            self.s2.push(P::new(1.0, 1.0));
            self.s3.push(P::new(2.0, 0.0));
            self.active_segment = 0;
            return;
        }

        let new_s1 = self
            .s3
            .last()
            .copied()
            .or_else(|| self.s1.last().copied())
            .unwrap_or(P::new(0.0, 0.0));

        let last_s1 = self.s1.last().copied().unwrap_or(P::new(0.0, 0.0));
        let last_s3 = self.s3.last().copied().unwrap_or(P::new(1.0, 0.0));
        let dx = last_s3.x - last_s1.x;
        let dy = last_s3.y - last_s1.y;
        let (sx, sy) = if dx * dx + dy * dy < 1e-10 {
            (1.0, 0.0)
        } else {
            (dx, dy)
        };
        let new_s3 = P::new(new_s1.x + sx, new_s1.y + sy);

        let new_s2 = match (self.s2.last(), self.s3.last()) {
            (Some(&l2), Some(&l3)) => P::new(2.0 * l3.x - l2.x, 2.0 * l3.y - l2.y),
            _ => P::new((new_s1.x + new_s3.x) * 0.5, (new_s1.y + new_s3.y) * 0.5),
        };

        self.s1.push(new_s1);
        self.s2.push(new_s2);
        self.s3.push(new_s3);
        self.active_segment = self.n().saturating_sub(1);
    }

    pub fn pop_segment(&mut self) {
        self.s1.pop();
        self.s2.pop();
        self.s3.pop();
        let n = self.n();
        if n == 0 {
            self.active_segment = 0;
        } else if self.active_segment >= n {
            self.active_segment = n - 1;
        }
    }

    pub fn clamp_active_segment(&mut self) {
        let n = self.n();
        if n == 0 {
            self.active_segment = 0;
        } else if self.active_segment >= n {
            self.active_segment = n - 1;
        }
    }

    pub fn sampled_path(&self, samples_per_segment: usize) -> Vec<P> {
        let n = self.n();
        let mut pts = Vec::with_capacity(n * samples_per_segment + 1);
        if n == 0 {
            return pts;
        }
        for i in 0..n {
            for k in 0..samples_per_segment {
                let j = k as f32 / samples_per_segment as f32;
                pts.push(self.eval_segment(i, j));
            }
        }
        pts.push(self.eval_segment(n - 1, 1.0));
        pts
    }
}

pub const PALETTE: &[[u8; 3]] = &[
    [80, 160, 255],
    [240, 100, 100],
    [80, 200, 120],
    [220, 160, 60],
    [180, 100, 220],
    [100, 200, 220],
    [240, 120, 180],
    [120, 200, 100],
    [60, 80, 200],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_endpoints() {
        let mut c = CurveSet::empty("test", [0, 0, 0]);
        c.append_segment();
        let start = c.eval_segment(0, 0.0);
        assert!((start.x - c.s1[0].x).abs() < 1e-5);
        assert!((start.y - c.s1[0].y).abs() < 1e-5);
        let end = c.eval_segment(0, 1.0);
        assert!((end.x - c.s3[0].x).abs() < 1e-5);
        assert!((end.y - c.s3[0].y).abs() < 1e-5);
    }

    #[test]
    fn append_segment_chains_s3_to_s1() {
        let mut c = CurveSet::empty("test", [0, 0, 0]);
        c.append_segment();
        let s3_first = c.s3[0];
        c.append_segment();
        assert_eq!(c.s1[1], s3_first);
    }

    #[test]
    fn n_is_min_length() {
        let mut c = CurveSet::empty("test", [0, 0, 0]);
        c.append_segment();
        c.append_segment();
        c.s1.pop();
        assert_eq!(c.n(), 1);
    }

    #[test]
    fn serde_roundtrip() {
        let mut c = CurveSet::empty("hello", [10, 20, 30]);
        c.append_segment();
        c.append_segment();
        c.line_style = LineStyle::DashDot;
        c.fill_enabled = true;
        let json = serde_json::to_string(&c).unwrap();
        let back: CurveSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, c.name);
        assert_eq!(back.s1.len(), 2);
        assert_eq!(back.line_style, LineStyle::DashDot);
        assert!(back.fill_enabled);
    }
}
