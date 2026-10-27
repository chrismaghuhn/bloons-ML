//! Track als Polylinie mit kumulierten Längen, Platzierungsregeln.

use btd_data::v1::{AreaKind, MapV1, Placement};

use crate::{real, Real};

#[derive(Clone, Debug)]
pub struct Track {
    xs: Vec<Real>,
    ys: Vec<Real>,
    cum: Vec<Real>,
    /// Steigung je Segment (dx/ds, dy/ds), vorberechnet: gleiche Formel wie in `pos`.
    slope: Vec<(Real, Real)>,
    pub length: Real,
}

impl Track {
    pub fn new(map: &MapV1) -> Self {
        let xs: Vec<Real> = map.points.iter().map(|p| real(p[0])).collect();
        let ys: Vec<Real> = map.points.iter().map(|p| real(p[1])).collect();
        // Längen kommen aus den Daten (validiert: monoton, passend zu points)
        let cum: Vec<Real> = map.cum.iter().map(|&c| real(c)).collect();
        let length = *cum.last().unwrap();
        let slope = (0..cum.len() - 1)
            .map(|j| {
                let dx = cum[j + 1] - cum[j];
                ((xs[j + 1] - xs[j]) / dx, (ys[j + 1] - ys[j]) / dx)
            })
            .collect();
        Track { xs, ys, cum, slope, length }
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
        let off = dist - self.cum[j];
        let (sx, sy) = self.slope[j];
        (sx * off + self.xs[j], sy * off + self.ys[j])
    }
}

/// Ob ein Turm mit Footprint-Radius `r` bei (x, y) auf der Karte stehen darf (ohne andere Türme).
///
/// Raster (sim_v0): die Zelle unter dem Mittelpunkt muss bebaubar sein (wie in der Referenz).
/// Flächen: der Mittelpunkt muss auf Land liegen und der Footprint-Kreis darf keine andere
/// Fläche (Track, Wasser, gesperrt) berühren.
pub fn placeable(map: &MapV1, x: f64, y: f64, r: f64) -> bool {
    let [x0, y0, x1, y1] = map.bounds;
    if !(x.is_finite() && y.is_finite()) || x - r < x0 || y - r < y0 || x + r > x1 || y + r > y1 {
        return false;
    }
    match map.placement {
        Placement::Grid => {
            let g = &map.grid;
            let (cx, cy) = (((x - g.x0) / g.cell).floor(), ((y - g.y0) / g.cell).floor());
            if cx < 0.0 || cy < 0.0 || cx >= g.w as f64 || cy >= g.h as f64 {
                return false;
            }
            g.buildable[cy as usize * g.w + cx as usize]
        }
        Placement::Areas => {
            // die letzte Fläche, die den Mittelpunkt enthält, entscheidet
            let on_land = map
                .areas
                .iter()
                .rev()
                .find(|a| contains(&a.polygon, x, y))
                .is_some_and(|a| a.kind == AreaKind::Land);
            on_land
                && map
                    .areas
                    .iter()
                    .filter(|a| a.kind != AreaKind::Land)
                    .all(|a| !contains(&a.polygon, x, y) && dist_to_polygon_edge(&a.polygon, x, y) >= r)
        }
    }
}

/// Kleinster Abstand von (x, y) zu einer Kante des Polygons.
fn dist_to_polygon_edge(poly: &[[f64; 2]], x: f64, y: f64) -> f64 {
    let mut best = f64::INFINITY;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ([ax, ay], [bx, by]) = (poly[j], poly[i]);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let (px, py) = (ax + t * dx - x, ay + t * dy - y);
        best = best.min((px * px + py * py).sqrt());
        j = i;
    }
    best
}

/// Punkt-in-Polygon (Even-Odd-Regel).
pub fn contains(poly: &[[f64; 2]], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ([xi, yi], [xj, yj]) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use btd_data::v1::{AreaV1, GridV1};

    fn map(points: Vec<[f64; 2]>, cum: Vec<f64>) -> MapV1 {
        MapV1 {
            name: "t".into(),
            bounds: [0.0, 0.0, 10.0, 10.0],
            points,
            cum,
            grid: GridV1 {
                x0: 0.0,
                y0: 0.0,
                cell: 5.0,
                w: 2,
                h: 2,
                path: vec![false; 4],
                buildable: vec![true, false, true, true],
            },
            placement: Placement::Grid,
            areas: vec![],
        }
    }

    fn track() -> Track {
        Track::new(&map(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]], vec![0.0, 10.0, 15.0]))
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
    fn zero_length_segment_is_never_used_for_interpolation() {
        let t = Track::new(&map(
            vec![[0.0, 0.0], [5.0, 0.0], [5.0, 0.0], [10.0, 0.0]],
            vec![0.0, 5.0, 5.0, 10.0],
        ));
        for (d, hint) in [(5.0, 0), (5.0, 3), (4.0, 2), (7.5, 1)] {
            let (x, y) = t.pos(d, t.segment(d, hint));
            assert!(x.is_finite() && y.is_finite(), "d={d} hint={hint}");
            assert_eq!(x, d);
        }
    }

    #[test]
    fn segment_moves_both_ways() {
        let t = track();
        assert_eq!(t.segment(12.0, 0), 1);
        assert_eq!(t.segment(3.0, 1), 0);
    }

    #[test]
    fn grid_placement() {
        let m = map(vec![[0.0, 0.0], [1.0, 0.0]], vec![0.0, 1.0]);
        assert!(placeable(&m, 2.5, 2.5, 0.5));
        assert!(!placeable(&m, 7.5, 2.5, 0.5), "Zelle (1,0) nicht bebaubar");
        assert!(!placeable(&m, 0.2, 2.5, 0.5), "Footprint ragt über den Rand");
        assert!(!placeable(&m, f64::NAN, 2.5, 0.5));
    }

    #[test]
    fn area_placement_uses_last_matching_area() {
        let mut m = map(vec![[0.0, 0.0], [1.0, 0.0]], vec![0.0, 1.0]);
        m.placement = Placement::Areas;
        let square = |a: f64, b: f64| vec![[a, a], [b, a], [b, b], [a, b]];
        m.areas = vec![
            AreaV1 { kind: AreaKind::Land, polygon: square(0.0, 10.0) },
            AreaV1 { kind: AreaKind::Track, polygon: square(4.0, 6.0) },
        ];
        assert!(placeable(&m, 2.0, 2.0, 0.5));
        assert!(!placeable(&m, 5.0, 5.0, 0.5), "auf dem Track");
        assert!(!placeable(&m, 3.7, 5.0, 0.5), "Footprint ragt in den Track");
        assert!(placeable(&m, 3.4, 5.0, 0.5), "berührt den Track nicht");
    }
}
