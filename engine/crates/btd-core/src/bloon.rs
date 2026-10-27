//! Bloons als Structure-of-Arrays, Schaden und Splitting.

use btd_data::{BloonDef, BloonKind, DamageType};
use smallvec::SmallVec;

use crate::Real;

/// Ergebnis eines Treffers auf einen Bloon.
pub struct PopResult {
    /// Übrig gebliebene Bloons (leer = komplett zerplatzt). Unverändert = immun.
    pub rest: SmallVec<[BloonKind; 4]>,
    /// Anzahl geplatzter Schichten (= Cash).
    pub pops: u32,
}

/// Wendet `damage` auf einen Bloon an. Überschuss-Schaden geht rekursiv an die Kinder
/// (wie `btd_sim.engine.pop`).
pub fn pop(defs: &[BloonDef], kind: BloonKind, damage: i32, dmg_type: DamageType) -> PopResult {
    let mut res = PopResult { rest: SmallVec::new(), pops: 0 };
    pop_into(defs, kind, damage, dmg_type, &mut res);
    res
}

fn pop_into(defs: &[BloonDef], kind: BloonKind, damage: i32, dmg_type: DamageType, out: &mut PopResult) {
    let def = &defs[kind as usize];
    if damage <= 0 || def.immune.contains(dmg_type) {
        out.rest.push(kind);
        return;
    }
    out.pops += 1;
    for &child in &def.children {
        pop_into(defs, child, damage - 1, dmg_type, out);
    }
}

/// Alle Bloons auf dem Track. Tote werden im Tick nur markiert und am Tick-Ende
/// stabil entfernt, damit die Reihenfolge (= Spawn-Reihenfolge) erhalten bleibt.
#[derive(Clone, Debug, Default)]
pub struct BloonStore {
    pub kind: Vec<BloonKind>,
    pub dist: Vec<Real>,
    pub seg: Vec<u16>,
    pub alive: Vec<bool>,
}

impl BloonStore {
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    pub fn push(&mut self, kind: BloonKind, dist: Real, seg: u16) {
        self.kind.push(kind);
        self.dist.push(dist);
        self.seg.push(seg);
        self.alive.push(true);
    }

    pub fn clear(&mut self) {
        self.kind.clear();
        self.dist.clear();
        self.seg.clear();
        self.alive.clear();
    }

    /// Entfernt alle `alive == false`, Reihenfolge bleibt erhalten. O(n).
    pub fn compact(&mut self) {
        let mut w = 0;
        for r in 0..self.kind.len() {
            if self.alive[r] {
                self.kind[w] = self.kind[r];
                self.dist[w] = self.dist[r];
                self.seg[w] = self.seg[r];
                self.alive[w] = true;
                w += 1;
            }
        }
        self.kind.truncate(w);
        self.dist.truncate(w);
        self.seg.truncate(w);
        self.alive.truncate(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use btd_data::Immunities;

    fn defs() -> Vec<BloonDef> {
        let b = |name: &str, children: Vec<u8>, immune: u8| BloonDef {
            name: name.into(),
            children,
            speed: 1.0,
            rbe: 1,
            immune: Immunities(immune),
        };
        vec![
            b("red", vec![], 0),                                 // 0
            b("blue", vec![0], 0),                               // 1
            b("pink", vec![1], 0),                               // 2
            b("black", vec![2, 2], DamageType::Explosive.bit()), // 3
            b("lead", vec![3, 3], DamageType::Sharp.bit()),      // 4
        ]
    }

    #[test]
    fn single_layer_and_overkill() {
        let d = defs();
        let r = pop(&d, 1, 1, DamageType::Sharp);
        assert_eq!((r.rest.as_slice(), r.pops), (&[0u8][..], 1));
        let r = pop(&d, 1, 5, DamageType::Sharp);
        assert_eq!((r.rest.len(), r.pops), (0, 2));
    }

    #[test]
    fn splitting_with_excess_damage() {
        let d = defs();
        let r = pop(&d, 3, 1, DamageType::Sharp);
        assert_eq!((r.rest.as_slice(), r.pops), (&[2u8, 2][..], 1));
        let r = pop(&d, 3, 2, DamageType::Sharp);
        assert_eq!((r.rest.as_slice(), r.pops), (&[1u8, 1][..], 3));
    }

    #[test]
    fn immunities() {
        let d = defs();
        let r = pop(&d, 4, 3, DamageType::Sharp);
        assert_eq!((r.rest.as_slice(), r.pops), (&[4u8][..], 0));
        let r = pop(&d, 3, 3, DamageType::Explosive);
        assert_eq!(r.pops, 0);
        // Lead -> 2x Black per Explosion, Blacks sind gegen den Überschuss immun
        let r = pop(&d, 4, 2, DamageType::Explosive);
        assert_eq!((r.rest.as_slice(), r.pops), (&[3u8, 3][..], 1));
    }

    #[test]
    fn compact_keeps_order() {
        let mut s = BloonStore::default();
        for k in 0..5 {
            s.push(k, k as Real, 0);
        }
        s.alive[1] = false;
        s.alive[3] = false;
        s.compact();
        assert_eq!(s.kind, vec![0, 2, 4]);
        assert_eq!(s.dist, vec![0.0, 2.0, 4.0]);
    }
}
