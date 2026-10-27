//! Spielzustand, Aktionen und Runden-Simulation.

use std::sync::Arc;

use btd_data::v1::{Flags, GameV1, SpawnGroup, PATHS};
use btd_data::{DataError, TowerKind};
use xxhash_rust::xxh3::Xxh3;

use crate::bloon::{hit, BloonStore};
use crate::map::{placeable, Track};
use crate::tower::{Tower, TowerId};
use crate::{real, Money, Real};

/// Fester Seed für `state_hash`, damit Hashes über Läufe und Versionen vergleichbar bleiben.
const HASH_SEED: u64 = 0xB7D6_2026;

/// Aktionen zwischen den Runden. Positionen in Spiel-Einheiten der Karte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Place { kind: TowerKind, x: f64, y: f64 },
    Upgrade { tower: TowerId, path: u8 },
    Sell { tower: TowerId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    UnknownTower,
    NotPlaceable,
    Occupied,
    NotEnoughCash,
    NoTower,
    UpgradeUnavailable,
    SellingDisabled,
    GameOver,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoundReport {
    pub leaked: i32,
    pub pops: u32,
    pub seconds: f64,
    pub ticks: u32,
}

#[derive(Clone, Debug)]
pub struct Game {
    data: Arc<GameV1>,
    track: Track,
    speed_dt: Vec<Real>,
    pub cash: Money,
    pub lives: i32,
    /// Zuletzt geschaffte Runde (startet bei `start_round - 1`).
    pub round: u32,
    pub towers: Vec<Tower>,
    next_id: TowerId,
    bloons: BloonStore,
}

/// Spawn-Zeitpunkte einer Runde: `count` Bloons je Gruppe gleichmäßig auf [start, end],
/// alle Gruppen stabil nach Zeit sortiert (bei Gleichstand Gruppen-Reihenfolge).
fn spawn_schedule(groups: &[SpawnGroup]) -> Vec<(f64, usize)> {
    let mut out = Vec::with_capacity(groups.iter().map(|g| g.count as usize).sum());
    for (gi, g) in groups.iter().enumerate() {
        for i in 0..g.count {
            let t = if g.count == 1 {
                g.start
            } else {
                g.start + (g.end - g.start) * i as f64 / (g.count - 1) as f64
            };
            out.push((t, gi));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

impl Game {
    /// Prüft die Daten (`GameV1::validate`) und legt ein Spiel im Startzustand an.
    pub fn new(data: Arc<GameV1>) -> Result<Self, DataError> {
        data.validate()?;
        let track = Track::new(&data.map);
        let dt = real(data.rules.dt());
        let speed_dt = data.bloons.iter().map(|b| real(b.speed) * dt).collect();
        let mut g = Game {
            track,
            speed_dt,
            cash: Money::ZERO,
            lives: 0,
            round: 0,
            towers: Vec::new(),
            next_id: 0,
            bloons: BloonStore::default(),
            data,
        };
        g.reset();
        Ok(g)
    }

    pub fn reset(&mut self) {
        let r = &self.data.rules;
        self.cash = Money::from_dollars(r.start_cash);
        self.lives = r.start_lives;
        self.round = r.start_round - 1;
        self.towers.clear();
        self.next_id = 0;
        self.bloons.clear();
    }

    pub fn data(&self) -> &Arc<GameV1> {
        &self.data
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    pub fn max_round(&self) -> u32 {
        self.data.rules.end_round
    }

    pub fn over(&self) -> bool {
        self.lives <= 0 || self.round >= self.max_round()
    }

    pub fn won(&self) -> bool {
        self.lives > 0 && self.round >= self.max_round()
    }

    pub fn tower(&self, id: TowerId) -> Option<&Tower> {
        self.towers.iter().find(|t| t.id == id)
    }

    /// Turm, dessen Footprint den Punkt enthält.
    pub fn tower_at(&self, x: f64, y: f64) -> Option<&Tower> {
        self.towers.iter().find(|t| t.covers(x, y))
    }

    /// Preis eines Turms bzw. Upgrades nach den Regeln (Schwierigkeit).
    pub fn price(&self, medium: i64) -> Money {
        Money::from_dollars(self.data.cost(medium))
    }

    /// Ob dort ein Turm dieser Art stehen könnte (Karte + andere Türme, ohne Cash).
    pub fn can_place_at(&self, kind: TowerKind, x: f64, y: f64) -> bool {
        let Some(def) = self.data.towers.get(kind as usize) else { return false };
        let r = def.variants[def.variant_id([0; PATHS]).unwrap() as usize].footprint;
        placeable(&self.data.map, x, y, r) && !self.collides(x, y, r)
    }

    fn collides(&self, x: f64, y: f64, r: f64) -> bool {
        self.towers.iter().any(|t| {
            let (tx, ty) = t.pos();
            let (dx, dy) = (x - tx, y - ty);
            let min = r + t.footprint;
            dx * dx + dy * dy < min * min
        })
    }

    // ------------------------------------------------------------ Aktionen

    pub fn can_apply(&self, a: &Action) -> bool {
        self.check(a).is_ok()
    }

    fn check(&self, a: &Action) -> Result<(), ActionError> {
        if self.over() {
            return Err(ActionError::GameOver);
        }
        match *a {
            Action::Place { kind, x, y } => {
                let def = self.data.towers.get(kind as usize).ok_or(ActionError::UnknownTower)?;
                let r = def.variants[def.variant_id([0; PATHS]).unwrap() as usize].footprint;
                if !placeable(&self.data.map, x, y, r) {
                    return Err(ActionError::NotPlaceable);
                }
                if self.collides(x, y, r) {
                    return Err(ActionError::Occupied);
                }
                if self.cash < self.price(def.cost) {
                    return Err(ActionError::NotEnoughCash);
                }
            }
            Action::Upgrade { tower, path } => {
                let t = self.tower(tower).ok_or(ActionError::NoTower)?;
                let def = &self.data.towers[t.kind as usize];
                let tiers = t.next_tiers(path as usize).ok_or(ActionError::UpgradeUnavailable)?;
                def.variant(tiers).ok_or(ActionError::UpgradeUnavailable)?;
                let cost = def.upgrade_costs[path as usize][t.tiers[path as usize] as usize];
                if self.cash < self.price(cost) {
                    return Err(ActionError::NotEnoughCash);
                }
            }
            Action::Sell { tower } => {
                if !self.data.rules.selling {
                    return Err(ActionError::SellingDisabled);
                }
                self.tower(tower).ok_or(ActionError::NoTower)?;
            }
        }
        Ok(())
    }

    /// Führt die Aktion aus. `Place` liefert die ID des neuen Turms.
    pub fn apply(&mut self, a: Action) -> Result<Option<TowerId>, ActionError> {
        self.check(&a)?;
        let data = Arc::clone(&self.data);
        match a {
            Action::Place { kind, x, y } => {
                let def = &data.towers[kind as usize];
                let price = self.price(def.cost);
                self.cash -= price;
                let id = self.next_id;
                self.next_id += 1;
                self.towers.push(Tower::new(id, kind, def, x, y, price));
                return Ok(Some(id));
            }
            Action::Upgrade { tower, path } => {
                let p = path as usize;
                let t = self.towers.iter_mut().find(|t| t.id == tower).unwrap();
                let def = &data.towers[t.kind as usize];
                let price = Money::from_dollars(data.cost(def.upgrade_costs[p][t.tiers[p] as usize]));
                let v = def.variant(t.next_tiers(p).unwrap()).unwrap();
                self.cash -= price;
                t.spent += price;
                t.set_variant(v);
            }
            Action::Sell { tower } => {
                let i = self.towers.iter().position(|t| t.id == tower).unwrap();
                let t = self.towers.remove(i); // remove statt swap_remove: Bau-Reihenfolge bleibt
                self.cash += t.sell_value(data.rules.sell_ratio);
            }
        }
        Ok(None)
    }

    // ------------------------------------------------------------ Runde

    /// Spielt die nächste Runde komplett durch (Ablauf identisch zu `btd_sim.Game.play_round`).
    pub fn play_round(&mut self) -> RoundReport {
        if self.over() {
            return RoundReport::default();
        }
        let data = Arc::clone(&self.data);
        let rules = &data.rules;
        let groups = &data.rounds[self.round as usize];
        let income = data.income[self.round as usize];
        let spawns = spawn_schedule(groups);
        let dt = real(rules.dt());
        let eps = real(rules.cd_eps);
        let spacing = real(rules.child_spacing);

        let lives_before = self.lives;
        let mut pops_total: u32 = 0;
        let mut si = 0;
        // Rundenuhr immer in f64 (auch im f32-Build), sonst driften Spawn-Zeitpunkte über
        // Tausende Additionen von dt. Die Bloon-Physik selbst läuft in `Real`.
        let mut t: f64 = 0.0;
        let mut ticks = 0;
        self.bloons.clear();
        for tw in &mut self.towers {
            tw.attacks.iter_mut().for_each(|a| a.cd_left = 0.0);
        }
        let mut in_range: Vec<usize> = Vec::new();
        let mut hit_list: Vec<usize> = Vec::new();

        while t < rules.max_round_seconds {
            // Spawnen
            while si < spawns.len() && spawns[si].0 <= t {
                let g = &groups[spawns[si].1];
                self.bloons.push(g.kind, g.flags, real(data.bloons[g.kind as usize].hp), 0.0, 0);
                si += 1;
            }

            // Bewegen + Durchgelassene
            if !self.bloons.is_empty() {
                let b = &mut self.bloons;
                let mut leaked = 0;
                for i in 0..b.len() {
                    b.dist[i] += self.speed_dt[b.kind[i] as usize];
                    if b.dist[i] >= self.track.length {
                        b.alive[i] = false;
                        leaked += data.bloons[b.kind[i] as usize].leak;
                    } else {
                        b.seg[i] = self.track.segment(b.dist[i], b.seg[i] as usize) as u16;
                    }
                }
                if leaked > 0 {
                    b.compact();
                    self.lives -= leaked;
                    if self.lives <= 0 {
                        self.lives = 0;
                        break;
                    }
                }
            }

            // Türme schießen. Cooldowns laufen jeden Tick, der Überschuss wird übertragen.
            for tw in &mut self.towers {
                tw.attacks.iter_mut().for_each(|a| a.cd_left -= dt);
            }
            if !self.bloons.is_empty() {
                // Lebende Bloons in diesem Tick. Sind alle weg, werden die restlichen Angriffe
                // übersprungen und ihr Cooldown bleibt unangetastet (wie in der Referenz).
                let mut alive = self.bloons.len();
                'towers: for ti in 0..self.towers.len() {
                    for ai in 0..self.towers[ti].attacks.len() {
                        if alive == 0 {
                            break 'towers;
                        }
                        let tw = &self.towers[ti];
                        let atk = &tw.attacks[ai];
                        if atk.cd_left > eps {
                            continue;
                        }
                        let (cx, cy) = (tw.x, tw.y);
                        let b = &self.bloons;
                        in_range.clear();
                        let mut first = usize::MAX;
                        for i in 0..b.len() {
                            if !b.alive[i] || (!atk.camo && b.flags[i].has(Flags::CAMO)) {
                                continue;
                            }
                            let (x, y) = self.track.pos(b.dist[i], b.seg[i] as usize);
                            if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= atk.range2 {
                                if first == usize::MAX || b.dist[i] > b.dist[first] {
                                    first = i; // Targeting "First"; bei Gleichstand der kleinere Index
                                }
                                in_range.push(i);
                            }
                        }
                        if in_range.is_empty() {
                            self.towers[ti].attacks[ai].cd_left = 0.0; // bereit, aber kein Ziel
                            continue;
                        }
                        hit_list.clear();
                        if let Some(br2) = atk.splash2 {
                            let (fx, fy) = self.track.pos(b.dist[first], b.seg[first] as usize);
                            for i in 0..b.len() {
                                if !b.alive[i] {
                                    continue;
                                }
                                let (x, y) = self.track.pos(b.dist[i], b.seg[i] as usize);
                                if (x - fx) * (x - fx) + (y - fy) * (y - fy) <= br2 {
                                    hit_list.push(i);
                                }
                            }
                        } else {
                            hit_list.extend_from_slice(&in_range);
                        }
                        // stabil absteigend nach Distanz (wie argsort(-dist, kind="stable"))
                        hit_list.sort_by(|&a, &c| b.dist[c].total_cmp(&b.dist[a]));
                        hit_list.truncate(atk.pierce as usize);

                        let (damage, cannot, cooldown) = (atk.damage, atk.cannot_damage, atk.cooldown);
                        let cd = &mut self.towers[ti].attacks[ai].cd_left;
                        *cd = cd.max(-dt) + cooldown;

                        for &i in &hit_list {
                            let b = &mut self.bloons;
                            let res = hit(&data.bloons, b.kind[i], b.hp[i], damage, cannot);
                            if !res.changed {
                                continue;
                            }
                            if res.pops == 0 {
                                b.hp[i] = res.rest[0].1; // Teilschaden: Bloon bleibt an seinem Platz
                                continue;
                            }
                            b.alive[i] = false;
                            alive -= 1;
                            alive += res.rest.len();
                            pops_total += res.pops;
                            let (d, seg, flags) = (b.dist[i], b.seg[i] as usize, b.flags[i]);
                            for (j, &(child, hp)) in res.rest.iter().enumerate() {
                                let cd = d - spacing * j as Real;
                                let cseg = self.track.segment(cd, seg) as u16;
                                b.push(child, flags, hp, cd, cseg); // Kinder erben die Flags [KAL]
                            }
                        }
                    }
                }
                self.bloons.compact();
            }

            t += rules.dt();
            ticks += 1;
            if si >= spawns.len() && self.bloons.is_empty() {
                break;
            }
        }

        self.cash += Money::from_f64(pops_total as f64 * income.cash_per_pop);
        if self.lives > 0 {
            self.round += 1;
            self.cash += Money::from_f64(income.end_bonus);
        }
        self.bloons.clear();
        RoundReport { leaked: lives_before - self.lives, pops: pops_total, seconds: t, ticks }
    }

    /// Hash über den kompletten Spielzustand zwischen den Runden (xxh3, fester Seed,
    /// Floats über `to_bits`). Gleicher Seed + gleiche Aktionen ⇒ gleicher Hash.
    /// Cooldowns fehlen bewusst: sie werden zu Rundenbeginn ohnehin zurückgesetzt.
    pub fn state_hash(&self) -> u64 {
        let mut h = Xxh3::with_seed(HASH_SEED);
        h.update(&self.cash.0.to_le_bytes());
        h.update(&self.lives.to_le_bytes());
        h.update(&self.round.to_le_bytes());
        for t in &self.towers {
            h.update(&t.id.to_le_bytes());
            h.update(&[t.kind]);
            h.update(&t.tiers);
            h.update(&t.x.to_bits().to_le_bytes());
            h.update(&t.y.to_bits().to_le_bytes());
            h.update(&t.spent.0.to_le_bytes());
        }
        h.digest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_spreads_groups_and_sorts_stably() {
        let g = |count, start, end| SpawnGroup { kind: 0, flags: Flags::default(), count, start, end };
        let s = spawn_schedule(&[g(3, 0.0, 1.0), g(1, 0.5, 0.5), g(1, 0.0, 0.0)]);
        let times: Vec<f64> = s.iter().map(|x| x.0).collect();
        assert_eq!(times, vec![0.0, 0.0, 0.5, 0.5, 1.0]);
        assert_eq!(
            s.iter().map(|x| x.1).collect::<Vec<_>>(),
            vec![0, 2, 0, 1, 0],
            "Gleichstand: Gruppen-Reihenfolge"
        );
    }
}
