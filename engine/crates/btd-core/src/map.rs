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

/// Fester Zielpunkt für `Targeting::TrackBest` (Mortar, docs/design-towers-step3.md §3): der
/// Streckenpunkt (Raster 1 Einheit entlang des Pfads, nur innerhalb von `bounds`) mit der meisten
/// Streckenlänge im Kreis mit `radius`; bei Gleichstand der mit der größeren Pfad-Distanz. In f64 und
/// fester Reihenfolge gerechnet, damit f32- und f64-Build denselben Punkt bekommen.
pub fn best_track_point(map: &MapV1, radius: f64) -> (f64, f64) {
    let (pts, cum) = (&map.points, &map.cum);
    let [x0, y0, x1, y1] = map.bounds;
    let at = |s: f64| {
        let j = cum.partition_point(|&c| c <= s).clamp(1, cum.len() - 1);
        let seg = cum[j] - cum[j - 1];
        let t = if seg > 0.0 { ((s - cum[j - 1]) / seg).clamp(0.0, 1.0) } else { 1.0 };
        (pts[j - 1][0] + (pts[j][0] - pts[j - 1][0]) * t, pts[j - 1][1] + (pts[j][1] - pts[j - 1][1]) * t)
    };
    let length = *cum.last().unwrap();
    let mut best = (f64::NEG_INFINITY, at(0.0));
    let mut s = 0.0;
    while s <= length {
        let (cx, cy) = at(s);
        if (x0..=x1).contains(&cx) && (y0..=y1).contains(&cy) {
            let covered = length_in_circle(pts, cx, cy, radius);
            // Gleichstand (bis auf Rundung der Schnittpunkte): der spätere Punkt gewinnt
            if covered >= best.0 - 1e-9 {
                best = (best.0.max(covered), (cx, cy));
            }
        }
        s += 1.0;
    }
    best.1
}

/// Länge der Polylinie innerhalb des Kreises (Summe der geschnittenen Segmentstücke).
fn length_in_circle(pts: &[[f64; 2]], cx: f64, cy: f64, r: f64) -> f64 {
    let mut total = 0.0;
    for w in pts.windows(2) {
        let (ax, ay) = (w[0][0] - cx, w[0][1] - cy);
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let a = dx * dx + dy * dy;
        if a == 0.0 {
            continue;
        }
        // |A + t D|² <= r²  <=>  a t² + 2 b t + c <= 0
        let (b, c) = (ax * dx + ay * dy, ax * ax + ay * ay - r * r);
        let disc = b * b - a * c;
        if disc <= 0.0 {
            continue;
        }
        let root = disc.sqrt();
        let (t0, t1) = (((-b - root) / a).max(0.0), ((-b + root) / a).min(1.0));
        if t1 > t0 {
            total += (t1 - t0) * a.sqrt();
        }
    }
    total
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

    fn track_map(points: Vec<[f64; 2]>) -> MapV1 {
        let mut cum = vec![0.0];
        for w in points.windows(2) {
            cum.push(cum.last().unwrap() + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
        }
        let grid = btd_data::v1::GridV1 {
            x0: -100.0,
            y0: -100.0,
            cell: 10.0,
            w: 20,
            h: 20,
            path: vec![false; 400],
            buildable: vec![true; 400],
        };
        MapV1 {
            name: "test".into(),
            bounds: [-100.0, -100.0, 100.0, 100.0],
            points,
            cum,
            grid,
            placement: Placement::Grid,
            areas: Vec::new(),
        }
    }

    #[test]
    fn best_track_point_prefers_dense_track_then_later_points() {
        // Haarnadel: zwei parallele Bahnen bei y = 0 und y = 10 bis x = 50 → am dichtesten in der Kurve
        let m = track_map(vec![[-90.0, 0.0], [50.0, 0.0], [50.0, 10.0], [-90.0, 10.0]]);
        let (x, y) = best_track_point(&m, 20.0);
        assert!((25.0..=50.0).contains(&x) && (0.0..=10.0).contains(&y), "in der Kurve: ({x}, {y})");
        // gerade Strecke: überall gleich viel, außer an den Enden → Gleichstand, der spätere Punkt gewinnt
        let m = track_map(vec![[-90.0, 0.0], [90.0, 0.0]]);
        let (x, _) = best_track_point(&m, 10.0);
        assert_eq!(x, 80.0, "letzter Punkt mit vollem Kreis");
        // Punkte außerhalb der bounds zählen nicht
        let m = track_map(vec![[-90.0, 0.0], [90.0, 0.0], [90.0, 200.0], [91.0, 200.0], [91.0, 0.0]]);
        let (_, y) = best_track_point(&m, 10.0);
        assert!(y <= 100.0);
    }

    #[test]
    fn length_in_circle_clips_segments() {
        let pts = [[-10.0, 0.0], [10.0, 0.0]];
        assert!((length_in_circle(&pts, 0.0, 0.0, 5.0) - 10.0).abs() < 1e-12);
        assert!((length_in_circle(&pts, 8.0, 0.0, 5.0) - 7.0).abs() < 1e-12);
        assert_eq!(length_in_circle(&pts, 0.0, 6.0, 5.0), 0.0);
    }
}
