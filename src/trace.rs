use std::collections::HashMap;

const CORNER_FRAC: f32 = 0.15;
const SMOOTH_FRAC: f32 = 0.5;

pub struct TraceOptions {
    pub threshold: u8,
    pub invert: bool,
    pub min_area_px: f64,
    pub simplify_px: f32,
    pub corner_deg: f32,
}

impl Default for TraceOptions {
    fn default() -> Self {
        Self {
            threshold: 128,
            invert: false,
            min_area_px: 64.0,
            simplify_px: 1.5,
            corner_deg: 60.0,
        }
    }
}

pub struct TracedShape {
    pub points: Vec<(f32, f32)>,
    pub area: f64,
    pub is_hole: bool,
}

#[derive(Clone, Default)]
pub struct Spline {
    pub s1: Vec<(f32, f32)>,
    pub s2: Vec<(f32, f32)>,
    pub s3: Vec<(f32, f32)>,
}

impl Spline {
    pub fn len(&self) -> usize {
        self.s1.len().min(self.s2.len()).min(self.s3.len())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn ink_mask(rgba: &[u8], w: usize, h: usize, threshold: u8, invert: bool) -> Vec<bool> {
    let mut mask = vec![false; w * h];
    for (i, m) in mask.iter_mut().enumerate() {
        let base = i * 4;
        if rgba[base + 3] < 128 {
            continue;
        }
        let gray = 0.299 * rgba[base] as f32
            + 0.587 * rgba[base + 1] as f32
            + 0.114 * rgba[base + 2] as f32;
        *m = (gray < threshold as f32) != invert;
    }
    mask
}

pub fn trace_bitmap(rgba: &[u8], w: usize, h: usize, opts: &TraceOptions) -> Vec<TracedShape> {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Vec::new();
    }
    let mask = ink_mask(rgba, w, h, opts.threshold, opts.invert);
    let loops = boundary_loops(&mask, w, h);

    let mut out = Vec::new();
    for ring in loops {
        let ring = collapse_collinear(ring);
        if ring.len() < 3 {
            continue;
        }
        let signed = shoelace(&ring);
        if signed.abs() < opts.min_area_px {
            continue;
        }
        let pts: Vec<(f32, f32)> = ring.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
        let pts = simplify_closed(&pts, opts.simplify_px.max(0.0));
        if pts.len() < 3 {
            continue;
        }
        out.push(TracedShape {
            points: pts,
            area: signed.abs(),
            is_hole: signed < 0.0,
        });
    }
    out.sort_by(|a, b| b.area.total_cmp(&a.area));
    out
}

fn boundary_loops(ink: &[bool], w: usize, h: usize) -> Vec<Vec<(i64, i64)>> {
    let is_ink = |x: i64, y: i64| -> bool {
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && ink[y as usize * w + x as usize]
    };

    let mut edges: Vec<((i64, i64), (i64, i64))> = Vec::new();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !is_ink(x, y) {
                continue;
            }
            if !is_ink(x, y - 1) {
                edges.push(((x, y), (x + 1, y)));
            }
            if !is_ink(x + 1, y) {
                edges.push(((x + 1, y), (x + 1, y + 1)));
            }
            if !is_ink(x, y + 1) {
                edges.push(((x + 1, y + 1), (x, y + 1)));
            }
            if !is_ink(x - 1, y) {
                edges.push(((x, y + 1), (x, y)));
            }
        }
    }

    let mut by_start: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (idx, (s, _)) in edges.iter().enumerate() {
        by_start.entry(*s).or_default().push(idx);
    }

    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let mut ring: Vec<(i64, i64)> = Vec::new();
        let mut cur = start;
        loop {
            used[cur] = true;
            let (s, e) = edges[cur];
            ring.push(s);
            let Some(cands) = by_start.get(&e) else {
                break;
            };
            let din = (e.0 - s.0, e.1 - s.1);
            let prefs = [(-din.1, din.0), din, (din.1, -din.0)];
            let mut next = None;
            'pick: for pref in prefs {
                for &ci in cands {
                    if used[ci] {
                        continue;
                    }
                    let (cs, ce) = edges[ci];
                    if (ce.0 - cs.0, ce.1 - cs.1) == pref {
                        next = Some(ci);
                        break 'pick;
                    }
                }
            }
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        if ring.len() >= 4 {
            loops.push(ring);
        }
    }
    loops
}

fn collapse_collinear(ring: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    let n = ring.len();
    if n < 3 {
        return ring;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = ring[(i + n - 1) % n];
        let cur = ring[i];
        let next = ring[(i + 1) % n];
        let d1 = (cur.0 - prev.0, cur.1 - prev.1);
        let d2 = (next.0 - cur.0, next.1 - cur.1);
        if d1.0 * d2.1 - d1.1 * d2.0 != 0 {
            out.push(cur);
        }
    }
    out
}

fn shoelace(ring: &[(i64, i64)]) -> f64 {
    let n = ring.len();
    let mut sum = 0i64;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        sum += a.0 * b.1 - b.0 * a.1;
    }
    sum as f64 * 0.5
}

fn perp_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len2.sqrt()
}

fn rdp(pts: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let a = pts[0];
    let b = pts[pts.len() - 1];
    let mut maxd = -1.0f32;
    let mut idx = 0usize;
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = perp_dist(*p, a, b);
        if d > maxd {
            maxd = d;
            idx = i;
        }
    }
    if maxd <= eps {
        vec![a, b]
    } else {
        let mut left = rdp(&pts[..=idx], eps);
        let right = rdp(&pts[idx..], eps);
        left.pop();
        left.extend(right);
        left
    }
}

fn simplify_closed(pts: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if eps <= 0.0 || pts.len() < 8 {
        return pts.to_vec();
    }
    let mut far = 0usize;
    let mut fd = -1.0f32;
    for (i, p) in pts.iter().enumerate() {
        let d = (p.0 - pts[0].0).powi(2) + (p.1 - pts[0].1).powi(2);
        if d > fd {
            fd = d;
            far = i;
        }
    }
    if far == 0 {
        return pts.to_vec();
    }
    let mut head = rdp(&pts[..=far], eps);
    let mut tail_src: Vec<(f32, f32)> = pts[far..].to_vec();
    tail_src.push(pts[0]);
    let tail = rdp(&tail_src, eps);
    head.pop();
    head.extend_from_slice(&tail[..tail.len().saturating_sub(1)]);
    head
}

fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn mid(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    lerp(a, b, 0.5)
}

pub fn quad_spline_closed(pts: &[(f32, f32)], corner_deg: f32) -> Option<Spline> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let corner_cos = corner_deg.clamp(0.0, 180.0).to_radians().cos();

    let mut t_in = Vec::with_capacity(n);
    let mut t_out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = pts[(i + n - 1) % n];
        let cur = pts[i];
        let next = pts[(i + 1) % n];
        let vin = (cur.0 - prev.0, cur.1 - prev.1);
        let vout = (next.0 - cur.0, next.1 - cur.1);
        let lin = (vin.0 * vin.0 + vin.1 * vin.1).sqrt();
        let lout = (vout.0 * vout.0 + vout.1 * vout.1).sqrt();
        let cos = if lin > 1e-6 && lout > 1e-6 {
            (vin.0 * vout.0 + vin.1 * vout.1) / (lin * lout)
        } else {
            1.0
        };
        let frac = if cos < corner_cos {
            CORNER_FRAC
        } else {
            SMOOTH_FRAC
        };
        t_in.push(lerp(cur, prev, frac));
        t_out.push(lerp(cur, next, frac));
    }

    for i in 0..n {
        let j = (i + 1) % n;
        let dx = t_out[i].0 - t_in[j].0;
        let dy = t_out[i].1 - t_in[j].1;
        if dx * dx + dy * dy < 1e-6 {
            t_in[j] = t_out[i];
        }
    }

    let mut sp = Spline::default();
    for i in 0..n {
        sp.s1.push(t_in[i]);
        sp.s2.push(pts[i]);
        sp.s3.push(t_out[i]);

        let nxt = t_in[(i + 1) % n];
        if t_out[i] != nxt {
            sp.s1.push(t_out[i]);
            sp.s2.push(mid(t_out[i], nxt));
            sp.s3.push(nxt);
        }
    }
    Some(sp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                out.extend_from_slice(&f(x, y));
            }
        }
        out
    }

    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn dark_square_traces_to_four_corners() {
        let data = img(20, 20, |x, y| {
            if (5..15).contains(&x) && (5..15).contains(&y) {
                BLACK
            } else {
                WHITE
            }
        });
        let shapes = trace_bitmap(&data, 20, 20, &TraceOptions::default());
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].points.len(), 4, "a square keeps 4 vertices");
        assert!(!shapes[0].is_hole);
        assert_eq!(shapes[0].area, 100.0);
    }

    #[test]
    fn invert_traces_the_light_region_instead() {
        let data = img(20, 20, |x, y| {
            if (5..15).contains(&x) && (5..15).contains(&y) {
                BLACK
            } else {
                WHITE
            }
        });
        let opts = TraceOptions {
            invert: true,
            ..Default::default()
        };
        let shapes = trace_bitmap(&data, 20, 20, &opts);
        let total: f64 = shapes.iter().map(|s| s.area).sum();
        assert!(
            shapes.iter().any(|s| s.is_hole),
            "the dark square becomes a hole once inverted"
        );
        assert!(total > 0.0);
    }

    #[test]
    fn ring_yields_an_outer_loop_and_a_hole() {
        let data = img(24, 24, |x, y| {
            let outer = (4..20).contains(&x) && (4..20).contains(&y);
            let inner = (9..15).contains(&x) && (9..15).contains(&y);
            if outer && !inner { BLACK } else { WHITE }
        });
        let opts = TraceOptions {
            min_area_px: 4.0,
            ..Default::default()
        };
        let shapes = trace_bitmap(&data, 24, 24, &opts);
        assert_eq!(shapes.len(), 2);
        assert!(!shapes[0].is_hole, "largest loop is the outer boundary");
        assert!(shapes[1].is_hole, "inner loop is flagged as a hole");
        assert_eq!(shapes[0].area, 256.0);
        assert_eq!(shapes[1].area, 36.0);
    }

    #[test]
    fn diagonal_blobs_stay_separate_loops() {
        let data = img(10, 10, |x, y| {
            if (x < 5 && y < 5) || (x >= 5 && y >= 5) {
                BLACK
            } else {
                WHITE
            }
        });
        let opts = TraceOptions {
            min_area_px: 4.0,
            ..Default::default()
        };
        let shapes = trace_bitmap(&data, 10, 10, &opts);
        assert_eq!(shapes.len(), 2, "touching diagonally is not one shape");
    }

    #[test]
    fn min_area_drops_specks() {
        let data = img(30, 30, |x, y| {
            if (10..25).contains(&x) && (10..25).contains(&y) {
                BLACK
            } else if x == 1 && y == 1 {
                BLACK
            } else {
                WHITE
            }
        });
        let opts = TraceOptions {
            min_area_px: 4.0,
            ..Default::default()
        };
        let shapes = trace_bitmap(&data, 30, 30, &opts);
        assert_eq!(shapes.len(), 1, "the 1 px speck is dropped");
    }

    #[test]
    fn threshold_controls_what_counts_as_ink() {
        let gray = [160, 160, 160, 255];
        let data = img(16, 16, |x, y| {
            if (4..12).contains(&x) && (4..12).contains(&y) {
                gray
            } else {
                WHITE
            }
        });
        let low = TraceOptions {
            threshold: 100,
            ..Default::default()
        };
        assert!(trace_bitmap(&data, 16, 16, &low).is_empty(), "gray is too light");
        let high = TraceOptions {
            threshold: 200,
            ..Default::default()
        };
        assert_eq!(trace_bitmap(&data, 16, 16, &high).len(), 1);
    }

    #[test]
    fn spline_segments_chain_into_a_closed_loop() {
        let pts = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let sp = quad_spline_closed(&pts, 60.0).unwrap();
        let n = sp.len();
        assert!(n >= 4);
        for i in 0..n {
            let next = (i + 1) % n;
            assert_eq!(
                sp.s3[i], sp.s1[next],
                "segment {i} must end where the next one starts"
            );
        }
    }

    #[test]
    fn smooth_polyline_uses_one_segment_per_vertex() {
        let pts: Vec<(f32, f32)> = (0..16)
            .map(|k| {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                (a.cos() * 50.0, a.sin() * 50.0)
            })
            .collect();
        let sp = quad_spline_closed(&pts, 60.0).unwrap();
        assert_eq!(sp.len(), 16, "no extra joins on a smooth circle");
        for i in 0..16 {
            let expected = mid(pts[i], pts[(i + 1) % 16]);
            assert!((sp.s3[i].0 - expected.0).abs() < 1e-3);
            assert!((sp.s3[i].1 - expected.1).abs() < 1e-3);
        }
    }

    #[test]
    fn sharp_corners_keep_their_point() {
        let pts = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let sp = quad_spline_closed(&pts, 60.0).unwrap();
        assert_eq!(sp.len(), 8, "each corner adds a straight joining segment");
        for p in &pts {
            let hugged = sp
                .s1
                .iter()
                .zip(&sp.s3)
                .any(|(a, b)| {
                    let da = (a.0 - p.0).abs() + (a.1 - p.1).abs();
                    let db = (b.0 - p.0).abs() + (b.1 - p.1).abs();
                    da < 2.0 && db < 2.0
                });
            assert!(hugged, "corner {p:?} should stay sharp");
        }
    }

    #[test]
    fn long_contours_stay_a_single_spline() {
        let pts: Vec<(f32, f32)> = (0..250)
            .map(|k| {
                let a = k as f32 / 250.0 * std::f32::consts::TAU;
                (a.cos() * 100.0, a.sin() * 100.0)
            })
            .collect();
        let sp = quad_spline_closed(&pts, 60.0).unwrap();
        assert_eq!(sp.len(), 250, "no splitting at the 99 segment mark");
        for i in 0..sp.len() {
            let next = (i + 1) % sp.len();
            assert_eq!(sp.s3[i], sp.s1[next], "the loop stays continuous");
        }
    }
}
