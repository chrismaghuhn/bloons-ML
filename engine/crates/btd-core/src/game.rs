//! Spielzustand, Aktionen und Runden-Simulation.

use std::sync::Arc;

use btd_data::{GameData, TowerKind};
use xxhash_rust::xxh3::Xxh3;

use crate::bloon::{pop, BloonStore};
use crate::map::Track;
use crate::tower::Tower;
use crate::{real, Money, Real};

/// Fester Seed für `state_hash`, damit Hashes über Läufe und Versionen vergleichbar bleiben.
const HASH_SEED: u64 = 0xB7D6_2026;

/// M1: Platzierung auf Grid-Zellen wie in der Python-Referenz. Ab M2 kontinuierlich (Weltkoordinaten).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Place { kind: TowerKind, x: u16, y: u16 },
    Upgrade { x: u16, y: u16, path: u8 },
    Sell { x: u16, y: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    OutOfBounds,
    UnknownTower,
    NotBuildable,
    Occupied,
    NotEnoughCash,
    NoTower,
    UpgradeUnavailable,
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
    data: Arc<GameData>,
    track: Track,
    speed_dt: Vec<Real>,
    pub cash: Money,
    pub lives: i32,
    /// Anzahl geschaffter Runden.
    pub round: u32,
    pub towers: Vec<Tower>,
    occupied: Vec<bool>,
    bloons: BloonStore,
}

impl Game {
    pub fn new(data: Arc<GameData>) -> Self {
        let track = Track::new(&data.map);
        let dt = real(data.rules.dt);
        let speed_dt = data.bloons.iter().map(|b| real(b.speed) * dt).collect();
        let cells = data.map.grid_w * data.map.grid_h;
        let mut g = Game {
            track,
            speed_dt,
            cash: Money::ZERO,
            lives: 0,
            round: 0,
            towers: Vec::new(),
            occupied: vec![false; cells],
            bloons: BloonStore::default(),
            data,
        };
        g.reset();
        g
    }

    pub fn reset(&mut self) {
        self.cash = Money::from_dollars(self.data.rules.start_cash);
        self.lives = self.data.rules.start_lives;
        self.round = 0;
        self.towers.clear();
        self.occupied.iter_mut().for_each(|o| *o = false);
        self.bloons.clear();
    }

    pub fn data(&self) -> &Arc<GameData> {
        &self.data
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    pub fn max_round(&self) -> u32 {
        self.data.rounds.len() as u32
    }

    pub fn over(&self) -> bool {
        self.lives <= 0 || self.round >= self.max_round()
    }

    pub fn won(&self) -> bool {
        self.lives > 0 && self.round >= self.max_round()
    }

    pub fn is_occupied(&self, x: u16, y: u16) -> bool {
        self.cell(x, y).is_some_and(|c| self.occupied[c])
    }

    pub fn tower_at(&self, x: u16, y: u16) -> Option<&Tower> {
        self.towers.iter().find(|t| t.x == x && t.y == y)
    }

    fn cell(&self, x: u16, y: u16) -> Option<usize> {
        let m = &self.data.map;
        ((x as usize) < m.grid_w && (y as usize) < m.grid_h).then(|| y as usize * m.grid_w + x as usize)
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
                let c = self.cell(x, y).ok_or(ActionError::OutOfBounds)?;
                if !self.data.map.buildable[c] {
                    return Err(ActionError::NotBuildable);
                }
                if self.occupied[c] {
                    return Err(ActionError::Occupied);
                }
                if self.cash < Money::from_dollars(def.cost) {
                    return Err(ActionError::NotEnoughCash);
                }
            }
            Action::Upgrade { x, y, path } => {
                let t = self.tower_at(x, y).ok_or(ActionError::NoTower)?;
                if path > 1 {
                    return Err(ActionError::UpgradeUnavailable);
                }
                let def = &self.data.towers[t.kind as usize];
                let up = t
                    .next_upgrade(def, &self.data.rules, path as usize)
                    .ok_or(ActionError::UpgradeUnavailable)?;
                if self.cash < Money::from_dollars(up.cost) {
                    return Err(ActionError::NotEnoughCash);
                }
            }
            Action::Sell { x, y } => {
                self.tower_at(x, y).ok_or(ActionError::NoTower)?;
            }
        }
        Ok(())
    }

    pub fn apply(&mut self, a: Action) -> Result<(), ActionError> {
        self.check(&a)?;
        let data = Arc::clone(&self.data);
        match a {
            Action::Place { kind, x, y } => {
                let def = &data.towers[kind as usize];
                self.cash -= Money::from_dollars(def.cost);
                self.towers.push(Tower::new(kind, def, x, y));
                let c = self.cell(x, y).unwrap();
                self.occupied[c] = true;
            }
            Action::Upgrade { x, y, path } => {
                let t = self.towers.iter_mut().find(|t| t.x == x && t.y == y).unwrap();
                let up = t.next_upgrade(&data.towers[t.kind as usize], &data.rules, path as usize).unwrap();
                self.cash -= Money::from_dollars(up.cost);
                t.apply_upgrade(path as usize, up);
            }
            Action::Sell { x, y } => {
                let i = self.towers.iter().position(|t| t.x == x && t.y == y).unwrap();
                let t = self.towers.remove(i); // remove statt swap_remove: Bau-Reihenfolge bleibt
                self.cash += t.sell_value(&data.rules);
                let c = self.cell(x, y).unwrap();
                self.occupied[c] = false;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ Runde

    /// Spielt die nächste Runde komplett durch (Ablauf identisch zu `btd_sim.Game.play_round`).
    pub fn play_round(&mut self) -> RoundReport {
        if self.over() {
            return RoundReport::default();
        }
        let data = Arc::clone(&self.data);
        let rules = &data.rules;
        let spawns = &data.rounds[self.round as usize];
        let dt = real(rules.dt);
        let eps = real(rules.cd_eps);
        let spacing = real(rules.child_spacing);
        let max_t = real(rules.max_round_seconds);

        let lives_before = self.lives;
        let mut pops_total: u32 = 0;
        let mut si = 0;
        let mut t: Real = 0.0;
        let mut ticks = 0;
        self.bloons.clear();
        for tw in &mut self.towers {
            tw.cd_left = 0.0;
        }
        let mut in_range: Vec<usize> = Vec::new();
        let mut hit: Vec<usize> = Vec::new();

        while t < max_t {
            // Spawnen
            while si < spawns.len() && real(spawns[si].time) <= t {
                self.bloons.push(spawns[si].kind, 0.0, 0);
                si += 1;
            }

            // Bewegen + Durchgelassene
            if !self.bloons.is_empty() {
                let b = &mut self.bloons;
                let mut leaked_rbe = 0;
                for i in 0..b.len() {
                    b.dist[i] += self.speed_dt[b.kind[i] as usize];
                    if b.dist[i] >= self.track.length {
                        b.alive[i] = false;
                        leaked_rbe += data.bloons[b.kind[i] as usize].rbe;
                    } else {
                        b.seg[i] = self.track.segment(b.dist[i], b.seg[i] as usize) as u16;
                    }
                }
                if leaked_rbe > 0 {
                    b.compact();
                    self.lives -= leaked_rbe;
                    if self.lives <= 0 {
                        self.lives = 0;
                        break;
                    }
                }
            }

            // Türme schießen. Cooldowns laufen jeden Tick, der Überschuss wird übertragen.
            for tw in &mut self.towers {
                tw.cd_left -= dt;
            }
            if !self.bloons.is_empty() {
                for ti in 0..self.towers.len() {
                    let tw = &self.towers[ti];
                    if tw.cd_left > eps {
                        continue;
                    }
                    let (cx, cy) = (tw.x as Real + 0.5, tw.y as Real + 0.5);
                    let r2 = tw.range * tw.range;
                    let b = &self.bloons;
                    in_range.clear();
                    let mut first = usize::MAX;
                    for i in 0..b.len() {
                        if !b.alive[i] {
                            continue;
                        }
                        let (x, y) = self.track.pos(b.dist[i], b.seg[i] as usize);
                        if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r2 {
                            if first == usize::MAX || b.dist[i] > b.dist[first] {
                                first = i; // Targeting "First"; bei Gleichstand der kleinere Index
                            }
                            in_range.push(i);
                        }
                    }
                    if in_range.is_empty() {
                        self.towers[ti].cd_left = 0.0; // bereit, aber kein Ziel: nicht ansparen
                        continue;
                    }
                    hit.clear();
                    if tw.blast > 0.0 {
                        let (fx, fy) = self.track.pos(b.dist[first], b.seg[first] as usize);
                        let br2 = tw.blast * tw.blast;
                        for i in 0..b.len() {
                            if !b.alive[i] {
                                continue;
                            }
                            let (x, y) = self.track.pos(b.dist[i], b.seg[i] as usize);
                            if (x - fx) * (x - fx) + (y - fy) * (y - fy) <= br2 {
                                hit.push(i);
                            }
                        }
                    } else {
                        hit.extend_from_slice(&in_range);
                    }
                    // stabil absteigend nach Distanz (wie argsort(-dist, kind="stable"))
                    hit.sort_by(|&a, &c| b.dist[c].partial_cmp(&b.dist[a]).unwrap());
                    hit.truncate(tw.pierce as usize);

                    let (damage, dmg_type, cooldown) = (tw.damage, tw.dmg_type, tw.cooldown);
                    let cd = &mut self.towers[ti].cd_left;
                    *cd = cd.max(-dt) + cooldown;

                    for &i in &hit {
                        let res = pop(&data.bloons, self.bloons.kind[i], damage, dmg_type);
                        if res.pops == 0 {
                            continue;
                        }
                        self.bloons.alive[i] = false;
                        pops_total += res.pops;
                        let (d, seg) = (self.bloons.dist[i], self.bloons.seg[i] as usize);
                        for (j, &child) in res.rest.iter().enumerate() {
                            let cd = d - spacing * j as Real;
                            let cseg = self.track.segment(cd, seg) as u16;
                            self.bloons.push(child, cd, cseg);
                        }
                    }
                }
                self.bloons.compact();
            }

            t += dt;
            ticks += 1;
            if si >= spawns.len() && self.bloons.is_empty() {
                break;
            }
        }

        self.cash += Money::from_dollars(pops_total as i64);
        if self.lives > 0 {
            self.round += 1;
            self.cash += Money::from_dollars(rules.round_bonus_base + self.round as i64);
        }
        self.bloons.clear();
        RoundReport { leaked: lives_before - self.lives, pops: pops_total, seconds: t as f64, ticks }
    }

    /// Hash über den kompletten Spielzustand zwischen den Runden (xxh3, fester Seed,
    /// Floats über `to_bits`). Gleicher Seed + gleiche Aktionen ⇒ gleicher Hash.
    pub fn state_hash(&self) -> u64 {
        let mut h = Xxh3::with_seed(HASH_SEED);
        h.update(&self.cash.0.to_le_bytes());
        h.update(&self.lives.to_le_bytes());
        h.update(&self.round.to_le_bytes());
        for t in &self.towers {
            h.update(&[t.kind, t.tiers[0], t.tiers[1], t.dmg_type as u8]);
            h.update(&t.x.to_le_bytes());
            h.update(&t.y.to_le_bytes());
            h.update(&t.spent.0.to_le_bytes());
            for v in [t.cd_left, t.range, t.cooldown, t.blast] {
                h.update(&v.to_bits().to_le_bytes());
            }
            h.update(&t.damage.to_le_bytes());
            h.update(&t.pierce.to_le_bytes());
        }
        h.digest()
    }
}
