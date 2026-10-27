//! Track als Polylinie mit kumulierten Längen.

use btd_data::MapDef;

use crate::{real, Real};

#[derive(Clone, Debug)]
pub struct Track {
    xs: Vec<Real>,
    ys: Vec<Real>,
    cum: Vec<Real>,
    pub length: Real,
}

impl Track {
    pub fn new(map: &MapDef) -> Self {
        let xs: Vec<Real> = map.points.iter().map(|p| real(p[0])).collect();
        let ys: Vec<Real> = map.points.iter().map(|p| real(p[1])).collect();
        let mut cum = vec![0.0 as Real];
        for i in 1..xs.len() {
            let (dx, dy) = (xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
            cum.push(cum[i - 1] + (dx * dx + dy * dy).sqrt());
        }
        let length = *cum.last().unwrap();
        Track { xs, ys, cum, length }
    }

    /// Segmentindex `j` mit `cum[j] <= dist < cum[j+1]`, ausgehend von einem Startwert.
    /// Da `dist` pro Bloon fast immer steigt, sind das meist 0–1 Schritte.
    #[inline]
    pub fn segment(&self, dist: Real, mut hint: usize) -> usize {
        let last = self.cum.len() - 2;
        hint = hint.min(last);
        while hint < last && dist >= self.cum[hint + 1] {
            hint += 1;
        }
        while hint > 0 && dist < self.cum[hint] {
            hint -= 1;
        }
        hint
    }

    /// Position auf dem Track, gleiche Formel wie `numpy.interp` (inkl. Klemmen an den Enden).
    #[inline]
    pub fn pos(&self, dist: Real, seg: usize) -> (Real, Real) {
        let n = self.cum.len() - 1;
        if dist <= self.cum[0] {
            return (self.xs[0], self.ys[0]);
        }
        if dist >= self.cum[n] {
            return (self.xs[n], self.ys[n]);
        }
        let j = seg;
        let dx = self.cum[j + 1] - self.cum[j];
        let off = dist - self.cum[j];
        let sx = (self.xs[j + 1] - self.xs[j]) / dx;
        let sy = (self.ys[j + 1] - self.ys[j]) / dx;
        (sx * off + self.xs[j], sy * off + self.ys[j])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track::new(&MapDef {
            name: "t".into(),
            grid_w: 1,
            grid_h: 1,
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]],
            path_mask: vec![false],
            buildable: vec![true],
        })
    }

    #[test]
    fn length_and_positions() {
        let t = track();
        assert_eq!(t.length, 15.0);
        assert_eq!(t.pos(5.0, t.segment(5.0, 0)), (5.0, 0.0));
        assert_eq!(t.pos(12.0, t.segment(12.0, 0)), (10.0, 2.0));
        assert_eq!(t.pos(10.0, t.segment(10.0, 0)), (10.0, 0.0));
    }

    #[test]
    fn clamps_outside_track() {
        let t = track();
        assert_eq!(t.pos(-1.0, t.segment(-1.0, 1)), (0.0, 0.0));
        assert_eq!(t.pos(99.0, t.segment(99.0, 0)), (10.0, 5.0));
    }

    #[test]
    fn segment_moves_both_ways() {
        let t = track();
        assert_eq!(t.segment(12.0, 0), 1);
        assert_eq!(t.segment(3.0, 1), 0);
    }
}
