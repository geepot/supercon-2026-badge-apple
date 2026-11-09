//! B1 change-only waveforms (after supercon26-badge-os and phthallo/supercon),
//! changed-row upload windows and the DC-balance check every table must pass.
use crate::frame::{HALF_ROW, HEIGHT, PLANE_LEN};

/// Inclusive, screen-order byte coordinates within one controller's half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub x0: u16,
    pub x1: u16,
    pub y0: u16,
    pub y1: u16,
}

impl Window {
    /// Upload windows covering every byte where `new` differs from any of
    /// `olds`. Pass both candidates for controller RAM 0x10: the frame before
    /// the shown one (if a pull swaps memories) and the shown frame (if it
    /// copies new to old), so uploads are right either way. Changed rows are
    /// grouped by dynamic programming, trading a window's fixed cost
    /// (`window_bytes`, in byte-equivalents) against unchanged bytes sent
    /// inside a wider window. O(rows²) per side; run it while the panel drives.
    pub fn plan(new: &[u8], olds: &[&[u8]], window_bytes: usize) -> Vec<Self> {
        assert_eq!(new.len(), PLANE_LEN);
        assert!(olds.iter().all(|old| old.len() == PLANE_LEN));
        let rows: Vec<Self> = (0..HEIGHT as u16)
            .filter_map(|y| Self::row_span(new, olds, y))
            .collect();
        // best[j] = cheapest cost for rows[..j]; from[j] = start of its last window.
        let mut best = vec![0usize; rows.len() + 1];
        let mut from = vec![0usize; rows.len() + 1];
        for j in 1..=rows.len() {
            best[j] = usize::MAX;
            let (mut x0, mut x1) = (u16::MAX, 0);
            for i in (0..j).rev() {
                x0 = x0.min(rows[i].x0);
                x1 = x1.max(rows[i].x1);
                let area = usize::from(x1 - x0 + 1) * usize::from(rows[j - 1].y1 - rows[i].y0 + 1);
                let cost = best[i] + window_bytes + area;
                if cost < best[j] {
                    (best[j], from[j]) = (cost, i);
                }
            }
        }
        let mut windows = Vec::new();
        let mut j = rows.len();
        while j > 0 {
            let i = from[j];
            let span = rows[i..j].iter().fold(rows[i], |w, r| w.union(*r));
            windows.push(span);
            j = i;
        }
        windows.reverse();
        windows
    }

    /// Changed bytes of row `y` against any of `olds`; whole-row compares
    /// (memcmp) skip unchanged rows quickly.
    fn row_span(new: &[u8], olds: &[&[u8]], y: u16) -> Option<Self> {
        let at = usize::from(y) * HALF_ROW..(usize::from(y) + 1) * HALF_ROW;
        let a = &new[at.clone()];
        let mut span: Option<Self> = None;
        for b in olds.iter().map(|old| &old[at.clone()]).filter(|b| a != *b) {
            let differs = |(p, q): (&u8, &u8)| p != q;
            let row = Self {
                x0: a.iter().zip(b).position(differs).unwrap() as u16,
                x1: a.iter().zip(b).rposition(differs).unwrap() as u16,
                y0: y,
                y1: y,
            };
            span = Some(span.map_or(row, |s| s.union(row)));
        }
        span
    }

    fn union(self, other: Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            x1: self.x1.max(other.x1),
            y0: self.y0.min(other.y0),
            y1: self.y1.max(other.y1),
        }
    }
}

/// Video table: ten one-frame groups (~290 ms full height), the controller
/// floor (a zero-length group hangs it). Table = (old << 1) | new. A changing
/// pixel gets one opposite-rail preparation frame, then nine frames on its
/// rail: drawing nets +8 VDH frames and erasing −8, so a pixel's charge never
/// exceeds one transition. Unchanged pixels and VCOM stay grounded.
/// Group bytes: [level, on, off, ph2, on, off, ph2, repeat]; the first half
/// is skipped (ph2 = 0) and the second is one frame.
pub fn fast_lut() -> [u8; 400] {
    let mut lut = [0; 400];
    for (i, group) in lut.chunks_exact_mut(8).enumerate() {
        let rail = match (i / 10, i % 10) {
            (1, 0) | (2, 1..) => 2,
            (2, 0) | (1, 1..) => 1,
            _ => 0,
        };
        group.copy_from_slice(&[rail << 2 | rail, 1, 1, 0, 1, 0, 1, 1]);
    }
    lut
}

/// Black/white conditioning between passes: 18 of 20 frames on VDH (drawing)
/// or VDL (erasing), last group grounded, two repeats. Confirmed to clear
/// stock artwork to white in two black/white rounds.
pub fn cleanup_lut() -> [u8; 400] {
    let mut lut = [0; 400];
    for (i, group) in lut.chunks_exact_mut(8).enumerate() {
        let rail = match (i / 10, i % 10) {
            (_, 9) => 0,
            (1, _) => 1,
            (2, _) => 2,
            _ => 0,
        };
        group.copy_from_slice(&[rail << 2 | rail, 1, 1, 0, 1, 1, 1, 2]);
    }
    lut
}

/// Net drive per table in frames: +1 per VDH frame, −1 per VDL frame. A group
/// runs `repeat × Σ halves ph2 × (on at phase level, off at next level)`.
/// `None` if any frame uses the red (VDHR) rail.
pub fn net_drive(lut: &[u8; 400]) -> Option<[i32; 5]> {
    let mut net = [0; 5];
    for (i, g) in lut.chunks_exact(8).enumerate() {
        let mut group = 0;
        for (half, (on, off, reps)) in [(g[1], g[2], g[3]), (g[4], g[5], g[6])]
            .into_iter()
            .enumerate()
        {
            for (phase, frames) in [on, off].into_iter().enumerate() {
                let sign = match (g[0] >> (6 - 4 * half - 2 * phase)) & 3 {
                    0 => 0,
                    1 => 1,
                    2 => -1,
                    _ => return None,
                };
                group += sign * i32::from(frames) * i32::from(reps);
            }
        }
        net[i / 10] += group * i32::from(g[7]);
    }
    Some(net)
}

/// DC balance: unchanged pixels and VCOM net zero, and erasing exactly undoes
/// drawing. One-directional drive burned a pattern into this panel
/// (docs/optimizations.md); every table the firmware loads must pass this.
pub fn dc_balanced(lut: &[u8; 400]) -> bool {
    matches!(net_drive(lut), Some([0, draw, erase, 0, 0]) if draw + erase == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference grouping: one window per run of consecutive changed rows.
    fn bands(new: &[u8], olds: &[&[u8]]) -> Vec<Window> {
        let mut bands: Vec<Window> = Vec::new();
        for row in (0..HEIGHT as u16).filter_map(|y| Window::row_span(new, olds, y)) {
            match bands.last_mut() {
                Some(band) if band.y1 + 1 == row.y0 => *band = band.union(row),
                _ => bands.push(row),
            }
        }
        bands
    }

    #[test]
    fn tables_are_balanced_with_expected_frames() {
        assert_eq!(net_drive(&fast_lut()), Some([0, 8, -8, 0, 0]));
        assert_eq!(net_drive(&cleanup_lut()), Some([0, 36, -36, 0, 0]));
        assert!(dc_balanced(&fast_lut()) && dc_balanced(&cleanup_lut()));
        // Frame f of table t is the second-half level of group f.
        let lut = fast_lut();
        let frames = |t: usize| -> Vec<u8> { (0..10).map(|g| lut[(t * 10 + g) * 8] & 3).collect() };
        assert_eq!(frames(1), [2, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(frames(2), [1, 2, 2, 2, 2, 2, 2, 2, 2, 2]);
        for t in [0, 3, 4] {
            assert_eq!(frames(t), [0; 10]);
        }
        // One-directional maintenance (phthallo's unchanged-white push) fails.
        let mut unbalanced = fast_lut();
        unbalanced[9 * 8] = 0x0a;
        assert!(!dc_balanced(&unbalanced));
        unbalanced = fast_lut();
        unbalanced[10 * 8] = 0x0f; // Red rail.
        assert_eq!(net_drive(&unbalanced), None);
    }

    #[test]
    fn bands_cover_changed_rows_only() {
        let white = vec![0; PLANE_LEN];
        let mut frame = white.clone();
        frame[HALF_ROW * 671 + 59] = 1;
        let w = |x0, x1, y0, y1| Window { x0, x1, y0, y1 };
        assert_eq!(bands(&frame, &[&white]), [w(59, 59, 671, 671)]);
        // Rows 0-1 and 671 changed: two bands, unchanged rows between skipped.
        frame[3] = 1;
        frame[HALF_ROW + 7] = 1;
        assert_eq!(
            bands(&frame, &[&white]),
            [w(3, 7, 0, 1), w(59, 59, 671, 671)]
        );
        assert!(bands(&frame, &[&frame]).is_empty());
        // Two candidates: union of both rows' change spans.
        let mut other = white.clone();
        other[HALF_ROW + 20] = 1;
        assert_eq!(bands(&white, &[&frame, &other])[0], w(3, 20, 0, 1));
    }

    /// Model the controller both ways a pull might leave RAM 0x10: swapped
    /// (holds the frame before the shown one) or copied (holds the shown
    /// frame). Uploading against both candidates must leave 0x10 exactly
    /// equal to every new frame, including A -> B -> A flips and skips.
    #[test]
    fn uploads_survive_swapping_or_copying_controller_ram() {
        for swaps in [true, false] {
            let mut shown = [vec![0u8; PLANE_LEN], vec![0; PLANE_LEN]];
            let mut previous = shown.clone(); // Model: frame before shown.
            let mut ram = shown.clone(); // Actual controller 0x10.
            for step in 0..16usize {
                let mut next = [vec![0u8; PLANE_LEN], vec![0; PLANE_LEN]];
                let side = usize::from(step % 4 >= 2);
                if step % 2 == 0 {
                    next[side][(671 - step * 30) * HALF_ROW + step] = 0x80; // A -> B -> A
                }
                next[side][step * 3 * HALF_ROW] = 0x01;
                for s in 0..2 {
                    for w in Window::plan(&next[s], &[&previous[s], &shown[s]], 450) {
                        for y in w.y0..=w.y1 {
                            let at = usize::from(y) * HALF_ROW;
                            let range = at + usize::from(w.x0)..at + usize::from(w.x1) + 1;
                            ram[s][range.clone()].copy_from_slice(&next[s][range]);
                        }
                    }
                }
                assert_eq!(ram, next, "stale RAM on transition {step} (swaps={swaps})");
                // Pull: 0x10 becomes the shown frame (swap) or keeps new (copy).
                if swaps {
                    ram = shown.clone();
                }
                previous = std::mem::replace(&mut shown, next);
            }
        }
    }

    #[test]
    fn planned_windows_cover_every_change_and_cost_no_more_than_bands() {
        let cost = |ws: &[Window], overhead: usize| -> usize {
            ws.iter()
                .map(|w| overhead + usize::from(w.x1 - w.x0 + 1) * usize::from(w.y1 - w.y0 + 1))
                .sum()
        };
        let (mut seed, white) = (12345u32, vec![0u8; PLANE_LEN]);
        for overhead in [0, 450, 100_000] {
            let mut new = white.clone();
            for _ in 0..400 {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                new[(seed >> 8) as usize % PLANE_LEN] = (seed >> 24) as u8 | 1;
            }
            let olds: [&[u8]; 2] = [&white, &new[..PLANE_LEN / 2].repeat(2)];
            let plan = Window::plan(&new, &olds, overhead);
            let mut covered = vec![false; PLANE_LEN];
            for w in &plan {
                for y in w.y0..=w.y1 {
                    for x in w.x0..=w.x1 {
                        covered[usize::from(y) * HALF_ROW + usize::from(x)] = true;
                    }
                }
            }
            for (i, c) in covered.iter().enumerate() {
                let changed = olds.iter().any(|o| o[i] != new[i]);
                assert!(
                    !changed || *c,
                    "byte {i} not uploaded (overhead {overhead})"
                );
            }
            let bands = bands(&new, &olds);
            assert!(cost(&plan, overhead) <= cost(&bands, overhead));
            if overhead == 100_000 {
                assert_eq!(plan.len(), 1, "huge window cost merges everything");
            }
        }
    }
}
