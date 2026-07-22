pub type Pt = (f32, f32);

pub struct Shape {
    pub vars: Vec<String>,
    pub color: [u8; 3],
    pub outline: Vec<Pt>,
}

pub struct Slot {
    pub color: [u8; 3],
    pub source: Option<Vec<String>>,
    pub rings: Vec<Vec<Pt>>,
}

pub fn centroid(pts: &[Pt]) -> Pt {
    if pts.is_empty() {
        return (0.0, 0.0);
    }
    let inv = 1.0 / pts.len() as f32;
    let sx: f32 = pts.iter().map(|p| p.0).sum();
    let sy: f32 = pts.iter().map(|p| p.1).sum();
    (sx * inv, sy * inv)
}

pub fn chain_outline(s1: &[Pt], s2: &[Pt], s3: &[Pt], per_segment: usize) -> Vec<Pt> {
    let n = s1.len().min(s2.len()).min(s3.len());
    let steps = per_segment.max(2);
    let mut out = Vec::with_capacity(n * steps);
    for i in 0..n {
        for k in 0..steps {
            let t = k as f32 / steps as f32;
            let u = 1.0 - t;
            out.push((
                u * u * s1[i].0 + 2.0 * u * t * s2[i].0 + t * t * s3[i].0,
                u * u * s1[i].1 + 2.0 * u * t * s2[i].1 + t * t * s3[i].1,
            ));
        }
    }
    out
}

pub fn resample_closed(poly: &[Pt], n: usize) -> Vec<Pt> {
    if n == 0 || poly.is_empty() {
        return Vec::new();
    }
    let m = poly.len();
    if m == 1 {
        return vec![poly[0]; n];
    }
    let mut cum = Vec::with_capacity(m + 1);
    cum.push(0.0f32);
    for i in 0..m {
        let a = poly[i];
        let b = poly[(i + 1) % m];
        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        cum.push(cum[i] + d);
    }
    let total = cum[m];
    if total <= 1e-9 {
        return vec![poly[0]; n];
    }
    let mut out = Vec::with_capacity(n);
    let mut j = 0usize;
    for k in 0..n {
        let target = total * k as f32 / n as f32;
        while j + 1 < m && cum[j + 1] < target {
            j += 1;
        }
        let seg = cum[j + 1] - cum[j];
        let t = if seg > 1e-9 {
            ((target - cum[j]) / seg).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let a = poly[j];
        let b = poly[(j + 1) % m];
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
    out
}

pub fn align_ring(reference: &[Pt], cand: &[Pt]) -> Vec<Pt> {
    let n = cand.len();
    if n == 0 || reference.len() != n {
        return cand.to_vec();
    }
    let mut best = cand.to_vec();
    let mut best_cost = f32::INFINITY;
    for reversed in [false, true] {
        let base: Vec<Pt> = if reversed {
            cand.iter().rev().copied().collect()
        } else {
            cand.to_vec()
        };
        for shift in 0..n {
            let mut cost = 0.0f32;
            for i in 0..n {
                let p = base[(i + shift) % n];
                let r = reference[i];
                cost += (p.0 - r.0).powi(2) + (p.1 - r.1).powi(2);
                if cost >= best_cost {
                    break;
                }
            }
            if cost < best_cost {
                best_cost = cost;
                best = (0..n).map(|i| base[(i + shift) % n]).collect();
            }
        }
    }
    best
}

pub fn ring_to_spline(ring: &[Pt]) -> (Vec<Pt>, Vec<Pt>, Vec<Pt>) {
    let n = ring.len();
    let mid = |a: Pt, b: Pt| ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
    let mut s1 = Vec::with_capacity(n);
    let mut s2 = Vec::with_capacity(n);
    let mut s3 = Vec::with_capacity(n);
    for i in 0..n {
        let p0 = ring[i];
        let p1 = ring[(i + 1) % n];
        let p2 = ring[(i + 2) % n];
        s1.push(mid(p0, p1));
        s2.push(p1);
        s3.push(mid(p1, p2));
    }
    (s1, s2, s3)
}

struct SlotBuild {
    color: [u8; 3],
    source: Option<Vec<String>>,
    rings: Vec<Option<Vec<Pt>>>,
    last: Option<Vec<Pt>>,
}

pub fn normalize_frames(frames: &[Vec<Shape>], n: usize) -> Vec<Slot> {
    let f = frames.len();
    if f == 0 || n < 3 {
        return Vec::new();
    }
    let mut slots: Vec<SlotBuild> = Vec::new();

    for (k, frame) in frames.iter().enumerate() {
        let rings: Vec<Vec<Pt>> = frame
            .iter()
            .map(|s| resample_closed(&s.outline, n))
            .filter(|r| r.len() == n)
            .collect();
        if rings.len() != frame.len() {

        }

        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (si, slot) in slots.iter().enumerate() {
            let Some(last) = &slot.last else {
                continue;
            };
            let c1 = centroid(last);
            for (ci, ring) in rings.iter().enumerate() {
                let c2 = centroid(ring);
                let d = ((c1.0 - c2.0).powi(2) + (c1.1 - c2.1).powi(2)).sqrt();
                pairs.push((d, si, ci));
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut slot_taken = vec![false; slots.len()];
        let mut shape_slot: Vec<Option<usize>> = vec![None; rings.len()];
        for (_, si, ci) in pairs {
            if slot_taken[si] || shape_slot[ci].is_some() {
                continue;
            }
            slot_taken[si] = true;
            shape_slot[ci] = Some(si);
        }

        for (ci, ring) in rings.into_iter().enumerate() {
            let color = frame.get(ci).map(|s| s.color).unwrap_or([90, 90, 90]);
            match shape_slot[ci] {
                Some(si) => {
                    let aligned = match &slots[si].last {
                        Some(prev) => align_ring(prev, &ring),
                        None => ring,
                    };
                    slots[si].last = Some(aligned.clone());
                    slots[si].rings[k] = Some(aligned);
                }
                None => {
                    let mut rings_slot = vec![None; f];
                    rings_slot[k] = Some(ring.clone());
                    slots.push(SlotBuild {
                        color,
                        source: if k == 0 {
                            frame.get(ci).map(|s| s.vars.clone())
                        } else {
                            None
                        },
                        rings: rings_slot,
                        last: Some(ring),
                    });
                }
            }
        }
    }

    let mut out = Vec::with_capacity(slots.len());
    for slot in slots {
        let known = slot.rings;
        let mut filled: Vec<Vec<Pt>> = Vec::with_capacity(f);
        for k in 0..f {
            if let Some(r) = &known[k] {
                filled.push(r.clone());
                continue;
            }
            let mut anchor: Option<&Vec<Pt>> = None;
            for d in 1..=f {
                if d <= k {
                    if let Some(r) = &known[k - d] {
                        anchor = Some(r);
                        break;
                    }
                }
                if k + d < f {
                    if let Some(r) = &known[k + d] {
                        anchor = Some(r);
                        break;
                    }
                }
            }
            match anchor {
                Some(r) => filled.push(vec![centroid(r); n]),
                None => filled.push(vec![(0.0, 0.0); n]),
            }
        }
        out.push(Slot {
            color: slot.color,
            source: slot.source,
            rings: filled,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(cx: f32, cy: f32, r: f32, n: usize) -> Vec<Pt> {
        (0..n)
            .map(|k| {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                (cx + a.cos() * r, cy + a.sin() * r)
            })
            .collect()
    }

    fn shape(outline: Vec<Pt>) -> Shape {
        Shape {
            vars: Vec::new(),
            color: [0, 0, 0],
            outline,
        }
    }

    #[test]
    fn resample_gives_the_requested_count_and_even_spacing() {
        let square = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let out = resample_closed(&square, 8);
        assert_eq!(out.len(), 8);
        let step = |a: Pt, b: Pt| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        for i in 0..8 {
            let d = step(out[i], out[(i + 1) % 8]);
            assert!((d - 5.0).abs() < 1e-3, "step {i} was {d}");
        }
    }

    #[test]
    fn resample_collapses_a_degenerate_outline() {
        let dot = vec![(3.0, 4.0), (3.0, 4.0), (3.0, 4.0)];
        let out = resample_closed(&dot, 6);
        assert_eq!(out.len(), 6);
        assert!(out.iter().all(|p| *p == (3.0, 4.0)));
    }

    #[test]
    fn align_undoes_a_rotated_start_point() {
        let base = circle(0.0, 0.0, 10.0, 16);
        let rotated: Vec<Pt> = (0..16).map(|i| base[(i + 5) % 16]).collect();
        let fixed = align_ring(&base, &rotated);
        for i in 0..16 {
            assert!((fixed[i].0 - base[i].0).abs() < 1e-3);
            assert!((fixed[i].1 - base[i].1).abs() < 1e-3);
        }
    }

    #[test]
    fn align_undoes_a_reversed_winding() {
        let base = circle(0.0, 0.0, 10.0, 12);
        let reversed: Vec<Pt> = base.iter().rev().copied().collect();
        let fixed = align_ring(&base, &reversed);
        let cost: f32 = (0..12)
            .map(|i| (fixed[i].0 - base[i].0).powi(2) + (fixed[i].1 - base[i].1).powi(2))
            .sum();
        assert!(cost < 1e-3, "winding should be flipped back, cost {cost}");
    }

    #[test]
    fn spline_from_ring_chains_exactly() {
        let ring = circle(0.0, 0.0, 5.0, 10);
        let (s1, s2, s3) = ring_to_spline(&ring);
        assert_eq!(s1.len(), 10);
        assert_eq!(s2.len(), 10);
        for i in 0..10 {
            assert_eq!(s3[i], s1[(i + 1) % 10], "segment {i} must chain");
        }
    }

    #[test]
    fn frames_with_different_point_counts_end_up_equal_length() {
        let frames = vec![
            vec![shape(circle(0.0, 0.0, 5.0, 7))],
            vec![shape(circle(1.0, 1.0, 6.0, 40))],
            vec![shape(circle(2.0, 0.0, 4.0, 23))],
        ];
        let slots = normalize_frames(&frames, 16);
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].rings.len(), 3);
        assert!(slots[0].rings.iter().all(|r| r.len() == 16));
    }

    #[test]
    fn a_shape_appearing_later_collapses_in_earlier_frames() {
        let frames = vec![
            vec![shape(circle(0.0, 0.0, 5.0, 20))],
            vec![
                shape(circle(0.0, 0.0, 5.0, 20)),
                shape(circle(50.0, 50.0, 3.0, 20)),
            ],
        ];
        let slots = normalize_frames(&frames, 12);
        assert_eq!(slots.len(), 2, "the newcomer gets its own slot");
        let newcomer = &slots[1];
        assert!(newcomer.source.is_none(), "it does not exist in frame 0");
        let first = &newcomer.rings[0];
        assert!(
            first.iter().all(|p| *p == first[0]),
            "collapsed to a single point before it appears"
        );
        let c = centroid(&newcomer.rings[1]);
        assert!((first[0].0 - c.0).abs() < 1.0 && (first[0].1 - c.1).abs() < 1.0);
        assert!(newcomer.rings[1].iter().any(|p| *p != first[0]));
    }

    #[test]
    fn a_shape_disappearing_collapses_in_later_frames() {
        let frames = vec![
            vec![
                shape(circle(0.0, 0.0, 5.0, 20)),
                shape(circle(50.0, 50.0, 3.0, 20)),
            ],
            vec![shape(circle(0.0, 0.0, 5.0, 20))],
        ];
        let slots = normalize_frames(&frames, 12);
        assert_eq!(slots.len(), 2);
        let gone = slots
            .iter()
            .find(|s| {
                let last = &s.rings[1];
                last.iter().all(|p| *p == last[0])
            })
            .expect("one slot collapses in the last frame");
        assert!(gone.rings[0].iter().any(|p| *p != gone.rings[0][0]));
    }

    #[test]
    fn shapes_are_matched_by_position_not_by_order() {
        let far = circle(100.0, 0.0, 4.0, 20);
        let near = circle(0.0, 0.0, 5.0, 20);
        let frames = vec![
            vec![shape(near.clone()), shape(far.clone())],
            vec![shape(far.clone()), shape(near.clone())],
        ];
        let slots = normalize_frames(&frames, 12);
        assert_eq!(slots.len(), 2, "no spurious slots when order flips");
        for slot in &slots {
            let c0 = centroid(&slot.rings[0]);
            let c1 = centroid(&slot.rings[1]);
            let moved = ((c1.0 - c0.0).powi(2) + (c1.1 - c0.1).powi(2)).sqrt();
            assert!(moved < 5.0, "a slot should stay put, moved {moved}");
        }
    }
}
