//! Bloons als Structure-of-Arrays, Schaden und Splitting.

use btd_data::v1::{BloonDefV1, Flags, Props};
use btd_data::BloonKind;
use smallvec::SmallVec;

use crate::{real, Real};

/// Ergebnis eines Treffers auf einen Bloon.
pub struct HitResult {
    /// Übrig gebliebene Bloons mit Rest-HP. Bei `pops == 0` und `changed` ist das der getroffene
    /// Bloon selbst mit weniger HP; bei `pops > 0` sind es seine Kinder (leer = komplett zerplatzt).
    pub rest: SmallVec<[(BloonKind, Real); 4]>,
    /// Anzahl geplatzter Schichten (Grundlage für Cash).
    pub pops: u32,
    /// Hat der Treffer überhaupt etwas bewirkt (sonst immun oder 0 Schaden)?
    pub changed: bool,
}

/// Wendet `damage` auf einen Bloon mit `hp` an. Überschuss-Schaden geht an die Kinder weiter,
/// jedes Kind bekommt ihn voll (wie `btd_sim.engine.pop`, dort mit 1 HP pro Schicht).
pub fn hit(defs: &[BloonDefV1], kind: BloonKind, hp: Real, damage: Real, cannot: Props) -> HitResult {
    let mut res = HitResult { rest: SmallVec::new(), pops: 0, changed: false };
    hit_into(defs, kind, hp, damage, cannot, &mut res);
    res
}

fn hit_into(
    defs: &[BloonDefV1],
    kind: BloonKind,
    hp: Real,
    damage: Real,
    cannot: Props,
    out: &mut HitResult,
) {
    let def = &defs[kind as usize];
    if damage <= 0.0 || cannot.intersects(def.props) {
        out.rest.push((kind, hp));
        return;
    }
    out.changed = true;
    if damage < hp {
        out.rest.push((kind, hp - damage));
        return;
    }
    out.pops += 1;
    let overflow = damage - hp;
    for &(child, count) in &def.children {
        let child_hp = real(defs[child as usize].hp);
        for _ in 0..count {
            hit_into(defs, child, child_hp, overflow, cannot, out);
        }
    }
}

/// Höchstzahl verschiedener Verlangsamungen ohne Glue-Stufe in einer Konfiguration.
pub const CHILL_SLOTS: usize = 4;

/// Eine Verlangsamung ohne Glue-Stufe; aktiv solange `left > 0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chill {
    pub mul: Real,
    pub left: Real,
    pub layers: u32,
}

/// Alle Bloons auf dem Track. Tote werden im Tick nur markiert und am Tick-Ende stabil
/// entfernt, damit die Reihenfolge (= Spawn-Reihenfolge) erhalten bleibt.
#[derive(Clone, Debug, Default)]
pub struct BloonStore {
    /// Laufende ID pro Runde (für Treffer-Sets der Projektile).
    pub id: Vec<u32>,
    pub kind: Vec<BloonKind>,
    pub flags: Vec<Flags>,
    pub hp: Vec<Real>,
    pub dist: Vec<Real>,
    pub seg: Vec<u16>,
    /// Position auf dem Track, zwischengespeichert: immer `track.pos(dist, seg)`.
    pub x: Vec<Real>,
    pub y: Vec<Real>,
    pub alive: Vec<bool>,
    /// Status-Effekte: eingefroren (Sekunden), Schichten, die das Einfrieren überlebt.
    pub frozen: Vec<Real>,
    pub freeze_layers: Vec<u32>,
    /// Verlangsamung: Faktor, Restdauer, Glue-Stufe, Schichten.
    pub slow_mul: Vec<Real>,
    pub slow_left: Vec<Real>,
    pub glue: Vec<u8>,
    pub glue_layers: Vec<u32>,
    /// Slots für Verlangsamungen ohne Glue-Stufe (Permafrost, Stun), einer je Effekt-ID; alle
    /// aktiven wirken zusätzlich zum Glue (multiplikativ).
    pub chill: Vec<[Chill; CHILL_SLOTS]>,
    /// Schaden über Zeit: Schaden pro Tick, Intervall, Zeit bis zum nächsten Tick, Restdauer, Schichten.
    pub dot_damage: Vec<Real>,
    pub dot_interval: Vec<Real>,
    pub dot_timer: Vec<Real>,
    pub dot_left: Vec<Real>,
    pub dot_layers: Vec<u32>,
}

impl BloonStore {
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        id: u32,
        kind: BloonKind,
        flags: Flags,
        hp: Real,
        dist: Real,
        seg: u16,
        (x, y): (Real, Real),
    ) {
        self.id.push(id);
        self.kind.push(kind);
        self.flags.push(flags);
        self.hp.push(hp);
        self.dist.push(dist);
        self.seg.push(seg);
        self.x.push(x);
        self.y.push(y);
        self.alive.push(true);
        self.frozen.push(0.0);
        self.freeze_layers.push(0);
        self.slow_mul.push(1.0);
        self.slow_left.push(0.0);
        self.glue.push(0);
        self.glue_layers.push(0);
        self.chill.push([Chill::default(); CHILL_SLOTS]);
        self.dot_damage.push(0.0);
        self.dot_interval.push(0.0);
        self.dot_timer.push(0.0);
        self.dot_left.push(0.0);
        self.dot_layers.push(0);
    }

    pub fn clear(&mut self) {
        self.id.clear();
        self.kind.clear();
        self.flags.clear();
        self.hp.clear();
        self.dist.clear();
        self.seg.clear();
        self.x.clear();
        self.y.clear();
        self.alive.clear();
        self.frozen.clear();
        self.freeze_layers.clear();
        self.slow_mul.clear();
        self.slow_left.clear();
        self.glue.clear();
        self.glue_layers.clear();
        self.chill.clear();
        self.dot_damage.clear();
        self.dot_interval.clear();
        self.dot_timer.clear();
        self.dot_left.clear();
        self.dot_layers.clear();
    }

    /// Entfernt alle `alive == false`, Reihenfolge bleibt erhalten. O(n).
    pub fn compact(&mut self) {
        // bis zum ersten toten Bloon bleibt alles, wo es ist
        let Some(first_dead) = self.alive.iter().position(|&a| !a) else { return };
        let mut w = first_dead;
        for r in first_dead..self.kind.len() {
            if self.alive[r] {
                self.id[w] = self.id[r];
                self.kind[w] = self.kind[r];
                self.flags[w] = self.flags[r];
                self.hp[w] = self.hp[r];
                self.dist[w] = self.dist[r];
                self.seg[w] = self.seg[r];
                self.x[w] = self.x[r];
                self.y[w] = self.y[r];
                self.alive[w] = true;
                self.frozen[w] = self.frozen[r];
                self.freeze_layers[w] = self.freeze_layers[r];
                self.slow_mul[w] = self.slow_mul[r];
                self.slow_left[w] = self.slow_left[r];
                self.glue[w] = self.glue[r];
                self.glue_layers[w] = self.glue_layers[r];
                self.chill[w] = self.chill[r];
                self.dot_damage[w] = self.dot_damage[r];
                self.dot_interval[w] = self.dot_interval[r];
                self.dot_timer[w] = self.dot_timer[r];
                self.dot_left[w] = self.dot_left[r];
                self.dot_layers[w] = self.dot_layers[r];
                w += 1;
            }
        }
        self.id.truncate(w);
        self.kind.truncate(w);
        self.flags.truncate(w);
        self.hp.truncate(w);
        self.dist.truncate(w);
        self.seg.truncate(w);
        self.x.truncate(w);
        self.y.truncate(w);
        self.alive.truncate(w);
        self.frozen.truncate(w);
        self.freeze_layers.truncate(w);
        self.slow_mul.truncate(w);
        self.slow_left.truncate(w);
        self.glue.truncate(w);
        self.glue_layers.truncate(w);
        self.chill.truncate(w);
        self.dot_damage.truncate(w);
        self.dot_interval.truncate(w);
        self.dot_timer.truncate(w);
        self.dot_left.truncate(w);
        self.dot_layers.truncate(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use btd_data::v1::Classes;

    fn defs() -> Vec<BloonDefV1> {
        let b = |name: &str, hp: f64, children: Vec<(u8, u16)>, props: u8| BloonDefV1 {
            name: name.into(),
            hp,
            speed: 1.0,
            leak: 1,
            children,
            props: Props(props),
            classes: Classes::default(),
            camo: false,
            regrow: false,
            radius: 0.0,
            tags: vec![],
        };
        vec![
            b("red", 1.0, vec![], 0),                    // 0
            b("blue", 1.0, vec![(0, 1)], 0),             // 1
            b("pink", 1.0, vec![(1, 1)], 0),             // 2
            b("black", 1.0, vec![(2, 2)], Props::BLACK), // 3
            b("lead", 1.0, vec![(3, 2)], Props::LEAD),   // 4
            b("ceramic", 10.0, vec![(2, 2)], 0),         // 5
        ]
    }

    const SHARP: Props = Props(Props::LEAD);
    const EXPLOSIVE: Props = Props(Props::BLACK);

    fn kinds(r: &HitResult) -> Vec<u8> {
        r.rest.iter().map(|&(k, _)| k).collect()
    }

    #[test]
    fn single_layer_and_overkill() {
        let d = defs();
        let r = hit(&d, 1, 1.0, 1.0, SHARP);
        assert_eq!((kinds(&r), r.pops), (vec![0], 1));
        let r = hit(&d, 1, 1.0, 5.0, SHARP);
        assert_eq!((r.rest.len(), r.pops), (0, 2));
    }

    #[test]
    fn splitting_with_excess_damage() {
        let d = defs();
        let r = hit(&d, 3, 1.0, 1.0, SHARP);
        assert_eq!((kinds(&r), r.pops), (vec![2, 2], 1));
        let r = hit(&d, 3, 1.0, 2.0, SHARP);
        assert_eq!((kinds(&r), r.pops), (vec![1, 1], 3));
    }

    #[test]
    fn immunities() {
        let d = defs();
        let r = hit(&d, 4, 1.0, 3.0, SHARP);
        assert_eq!((kinds(&r), r.pops, r.changed), (vec![4], 0, false));
        let r = hit(&d, 3, 1.0, 3.0, EXPLOSIVE);
        assert!(!r.changed);
        // Lead -> 2x Black per Explosion, die Blacks sind gegen den Überschuss immun
        let r = hit(&d, 4, 1.0, 2.0, EXPLOSIVE);
        assert_eq!((kinds(&r), r.pops), (vec![3, 3], 1));
    }

    #[test]
    fn multi_hp_bloons_take_partial_damage() {
        let d = defs();
        let r = hit(&d, 5, 10.0, 3.0, SHARP);
        assert_eq!((r.rest.as_slice(), r.pops, r.changed), (&[(5u8, 7.0 as Real)][..], 0, true));
        // 12 Schaden auf 10 HP: Ceramic platzt, 2 Überschuss gehen an beide Pinks
        let r = hit(&d, 5, 10.0, 12.0, SHARP);
        assert_eq!((kinds(&r), r.pops), (vec![0, 0], 5));
    }

    #[test]
    fn compact_keeps_order() {
        let mut s = BloonStore::default();
        for k in 0..5 {
            s.push(k as u32, k, Flags::default(), 1.0, k as Real, 0, (k as Real, 0.0));
        }
        s.alive[1] = false;
        s.alive[3] = false;
        s.compact();
        assert_eq!(s.kind, vec![0, 2, 4]);
        assert_eq!(s.dist, vec![0.0, 2.0, 4.0]);
    }
}
