//! Spielzustand, Aktionen und Runden-Simulation.

use std::sync::Arc;

use btd_data::v1::{BloonDefV1, Flags, GameV1, Props, SpawnGroup, PATHS};
use btd_data::{DataError, TowerKind};
use smallvec::SmallVec;
use xxhash_rust::xxh3::Xxh3;

use crate::bloon::{hit, BloonStore, Chill, CHILL_SLOTS};
use crate::map::{placeable, Track};
use crate::tower::{
    factor_for, AttackKind, ContactRt, EmissionRt, ExtrasRt, FilterRt, ProjectileDef, TagTable, Tower,
    TowerId,
};
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
    /// Tag-Name -> Bit und die Tag-Maske je Bloon-Typ.
    tags: TagTable,
    bloon_tags: Vec<u32>,
    pub cash: Money,
    pub lives: i32,
    /// Zuletzt geschaffte Runde (startet bei `start_round - 1`).
    pub round: u32,
    pub towers: Vec<Tower>,
    next_id: TowerId,
    bloons: BloonStore,
    projectiles: Vec<Projectile>,
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

/// Ein fliegendes Projektil.
#[derive(Clone, Debug)]
struct Projectile {
    x: Real,
    y: Real,
    vx: Real,
    vy: Real,
    /// Verbleibende Ticks (ganzzahlig, damit die Reichweite nicht an Float-Rundung hängt).
    ttl: u32,
    pierce: Real,
    radius: Real,
    damage: Real,
    cannot_damage: Props,
    camo: bool,
    extras: Arc<ExtrasRt>,
    /// Kontakt-/Ablauf-Effekte, Kurve und Treffer-Reset des Angriffs (Splitter haben keine).
    def: Option<Arc<ProjectileDef>>,
    /// Kurvenbahn: Ursprung, Drehung (cos, sin) und zurückgelegte Strecke.
    path: Option<(Real, Real, Real, Real, Real)>,
    /// Treffer-Set leeren alle `clear_every` Ticks (0 = nie); Ticks bis zum nächsten Leeren.
    clear_every: u32,
    clear_left: u32,
    /// IDs der Bloons, die dieses Projektil schon getroffen hat (inkl. Kindern davon).
    hit: SmallVec<[u32; 8]>,
}

/// Wirkung eines Treffers: Pops, entfernte Bloons (0/1) und neue Kinder.
struct HitOutcome {
    pops: u32,
    removed: usize,
    children: usize,
}

/// Unveränderliche Daten, die jeder Treffer braucht.
struct World<'a> {
    defs: &'a [BloonDefV1],
    tags: &'a [u32],
    track: &'a Track,
    spacing: Real,
}

impl World<'_> {
    fn is_camo(&self, b: &BloonStore, i: usize) -> bool {
        b.flags[i].has(Flags::CAMO) || self.defs[b.kind[i] as usize].camo
    }

    /// Eigenschaften inkl. Frozen, solange der Bloon eingefroren ist.
    fn props(&self, b: &BloonStore, i: usize) -> Props {
        let p = self.defs[b.kind[i] as usize].props;
        if b.frozen[i] > 0.0 {
            Props(p.0 | Props::FROZEN)
        } else {
            p
        }
    }

    /// Ob ein Angriff diesen Bloon anvisieren bzw. treffen darf (Camo, Tags, Status, Immunität).
    fn eligible(&self, b: &BloonStore, i: usize, camo: bool, ex: &ExtrasRt) -> bool {
        b.alive[i] && (camo || !self.is_camo(b, i)) && self.passes(b, i, &ex.filter)
    }

    /// Tag-, Status- und Eigenschafts-Filter.
    fn passes(&self, b: &BloonStore, i: usize, f: &FilterRt) -> bool {
        self.tags[b.kind[i] as usize] & f.exclude_tags == 0
            && (f.skip_props.0 == 0 || !f.skip_props.intersects(self.props(b, i)))
            && (!f.skip_frozen || b.frozen[i] <= 0.0)
            && (f.skip_glue_level == 0 || b.glue[i] < f.skip_glue_level)
    }

    /// Wendet einen Treffer auf Bloon `i` an. Kinder werden hinten angehängt (leicht versetzt,
    /// erben Flags und Status-Effekte mit einer Schicht weniger), danach wirken die Effekte des
    /// Treffers (Freeze, Slow) auf den überlebenden Bloon bzw. seine Kinder.
    /// `None`, wenn der Treffer nichts bewirkt.
    fn apply_hit(
        &self,
        b: &mut BloonStore,
        next_id: &mut u32,
        i: usize,
        damage: Real,
        cannot: Props,
        ex: &ExtrasRt,
    ) -> Option<HitOutcome> {
        if !b.alive[i] {
            return None; // schon geplatzt (z.B. durch die Explosion desselben Projektils)
        }
        let kind = b.kind[i];
        let immune = cannot.intersects(self.props(b, i));
        let mut dmg = damage;
        for &(mask, add) in &ex.damage_vs {
            if self.tags[kind as usize] & mask != 0 {
                dmg += add;
            }
        }
        let has_effect = ex.has_effect();
        // eingefrorene Bloons sind für Angriffe mit Frozen-Immunität unverwundbar
        let frozen_block = b.frozen[i] > 0.0 && cannot.0 & Props::FROZEN != 0;
        let res = if frozen_block { None } else { Some(hit(self.defs, kind, b.hp[i], dmg, cannot)) };
        let Some(res) = res.filter(|r| r.changed) else {
            // Effekte ohne Schaden (Glue) wirken, solange der Bloon nicht immun ist
            if has_effect && !immune {
                apply_effects(b, i, ex, self.tags[kind as usize]); // ohne Schaden kein Push
                return Some(HitOutcome { pops: 0, removed: 0, children: 0 });
            }
            return None;
        };
        if res.pops == 0 {
            b.hp[i] = res.rest[0].1; // Teilschaden: Bloon bleibt an seinem Platz
            apply_effects(b, i, ex, self.tags[kind as usize]);
            self.push_back(b, i, ex);
            return Some(HitOutcome { pops: 0, removed: 0, children: 0 });
        }
        b.alive[i] = false;
        let (d, seg, flags) = (b.dist[i], b.seg[i] as usize, b.flags[i]);
        let parent = Status::of(b, i);
        for (j, &(child, hp)) in res.rest.iter().enumerate() {
            let cd = d - self.spacing * j as Real;
            let cseg = self.track.segment(cd, seg) as u16;
            let cpos = self.track.pos(cd, cseg as usize);
            b.push(*next_id, child, flags, hp, cd, cseg, cpos); // Kinder erben die Flags [KAL]
            *next_id += 1;
            let c = b.len() - 1;
            parent.inherit(b, c);
            apply_effects(b, c, ex, self.tags[child as usize]);
            self.push_back(b, c, ex);
        }
        Some(HitOutcome { pops: res.pops, removed: 1, children: res.rest.len() })
    }

    /// Zurückstoßen entlang des Pfads (nicht hinter den Start).
    fn push_back(&self, b: &mut BloonStore, i: usize, ex: &ExtrasRt) {
        if let Some(p) = &ex.push {
            let d = p.amount * factor_for(&p.factor_for, self.tags[b.kind[i] as usize], 1.0);
            if d > 0.0 {
                b.dist[i] = (b.dist[i] - d).max(0.0);
                b.seg[i] = self.track.segment(b.dist[i], b.seg[i] as usize) as u16;
                (b.x[i], b.y[i]) = self.track.pos(b.dist[i], b.seg[i] as usize);
            }
        }
    }

    /// Schaden über Zeit für alle Bloons mit laufendem Effekt (vor dem Tick angelegte; Kinder
    /// erben den Effekt mit dem Rest-Timer). Gibt (Pops, ob ein Bloon platzte) zurück.
    fn tick_dots(&self, b: &mut BloonStore, next_id: &mut u32, dt: Real, none: &ExtrasRt) -> (u32, bool) {
        let (mut pops, mut removed) = (0, false);
        for i in 0..b.len() {
            if !b.alive[i] || b.dot_left[i] <= 0.0 {
                continue;
            }
            b.dot_left[i] -= dt;
            b.dot_timer[i] -= dt;
            if b.dot_timer[i] <= 0.0 {
                b.dot_timer[i] += b.dot_interval[i];
                // Schaden über Zeit trifft alle Bloon-Eigenschaften (Spiel: DamageType "Normal") [KAL]
                if let Some(h) = self.apply_hit(b, next_id, i, b.dot_damage[i], Props::default(), none) {
                    pops += h.pops;
                    removed |= h.removed > 0;
                }
            }
            if b.alive[i] && b.dot_left[i] <= 0.0 {
                b.dot_left[i] = 0.0;
                b.dot_layers[i] = 0;
            }
        }
        (pops, removed)
    }
}

/// Speed-Faktor durch Verlangsamungs-Zonen (der stärkste zählt).
fn zone_scale(w: &World, b: &BloonStore, i: usize, zones: &[(Real, Real, crate::tower::ZoneRt)]) -> Real {
    let (x, y) = (b.x[i], b.y[i]);
    let br = real(w.defs[b.kind[i] as usize].radius);
    let mut s: Real = 1.0;
    for (cx, cy, z) in zones {
        let rr = z.radius + br;
        if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= rr * rr
            && (z.camo || !w.is_camo(b, i))
            && w.passes(b, i, &z.filter)
        {
            s = s.min(z.speed_scale);
        }
    }
    s
}

/// Status-Effekte eines Bloons, die beim Platzen an die Kinder gehen.
struct Status {
    frozen: Real,
    freeze_layers: u32,
    slow_mul: Real,
    slow_left: Real,
    glue: u8,
    glue_layers: u32,
    chill: [Chill; CHILL_SLOTS],
    dot: (Real, Real, Real, Real, u32),
}

impl Status {
    fn of(b: &BloonStore, i: usize) -> Self {
        Status {
            frozen: b.frozen[i],
            freeze_layers: b.freeze_layers[i],
            slow_mul: b.slow_mul[i],
            slow_left: b.slow_left[i],
            glue: b.glue[i],
            glue_layers: b.glue_layers[i],
            chill: b.chill[i],
            dot: (b.dot_damage[i], b.dot_interval[i], b.dot_timer[i], b.dot_left[i], b.dot_layers[i]),
        }
    }

    /// Effekte überleben das Platzen, solange noch Schichten übrig sind.
    fn inherit(&self, b: &mut BloonStore, c: usize) {
        if self.freeze_layers > 1 && self.frozen > 0.0 {
            b.frozen[c] = self.frozen;
            b.freeze_layers[c] = self.freeze_layers - 1;
        }
        if self.glue_layers > 1 && self.slow_left > 0.0 {
            b.slow_mul[c] = self.slow_mul;
            b.slow_left[c] = self.slow_left;
            b.glue[c] = self.glue;
            b.glue_layers[c] = self.glue_layers - 1;
        }
        for (k, ch) in self.chill.iter().enumerate() {
            if ch.layers > 1 && ch.left > 0.0 {
                b.chill[c][k] = Chill { layers: ch.layers - 1, ..*ch };
            }
        }
        let (dmg, interval, timer, left, layers) = self.dot;
        if layers > 1 && left > 0.0 {
            (b.dot_damage[c], b.dot_interval[c], b.dot_timer[c], b.dot_left[c]) =
                (dmg, interval, timer, left);
            b.dot_layers[c] = layers - 1;
        }
    }
}

fn apply_effects(b: &mut BloonStore, i: usize, ex: &ExtrasRt, tags: u32) {
    if let Some(f) = &ex.freeze {
        let factor = f.scale.iter().find(|(m, _)| tags & m != 0).map_or(1.0, |&(_, k)| k);
        b.frozen[i] = b.frozen[i].max(f.seconds * factor);
        b.freeze_layers[i] = f.layers;
    }
    if let Some(sl) = &ex.slow {
        let mul = factor_for(&sl.multiplier_for, tags, sl.multiplier);
        let secs = factor_for(&sl.seconds_for, tags, sl.seconds);
        if sl.level == 0 {
            // Verlangsamung ohne Glue (Permafrost, Stun): Slot je ID, wirkt zusätzlich [KAL: Stapeln]
            b.chill[i][sl.slot as usize] = Chill { mul, left: secs, layers: sl.layers };
        } else if sl.level >= b.glue[i] {
            b.slow_mul[i] = mul;
            b.slow_left[i] = secs;
            b.glue[i] = sl.level;
            b.glue_layers[i] = sl.layers;
        }
    }
    if let Some(d) = &ex.dot {
        if d.level >= b.glue[i] || b.dot_left[i] <= 0.0 {
            b.dot_damage[i] = d.damage * factor_for(&d.factor_for, tags, 1.0);
            b.dot_interval[i] = d.interval;
            if b.dot_left[i] <= 0.0 {
                b.dot_timer[i] = d.interval; // erster Schadens-Tick nach einem Intervall
            }
            b.dot_left[i] = d.seconds;
            b.dot_layers[i] = d.layers;
            if d.level > b.glue[i] {
                b.glue[i] = d.level;
            }
        }
    }
}

/// Lebensdauer in ganzen Ticks (mindestens 1).
fn ticks_of(lifespan: Real, dt: Real) -> u32 {
    ((lifespan / dt).round() as u32).max(1)
}

/// Richtung des k-ten Projektils relativ zur Basisrichtung, in Grad.
fn emission_angle(e: &EmissionRt, k: u32) -> Real {
    if e.count <= 1 {
        return e.offset;
    }
    if e.angle >= 360.0 {
        e.offset + e.angle * k as Real / e.count as Real
    } else {
        e.offset - e.angle / 2.0 + e.angle * k as Real / (e.count - 1) as Real
    }
}

// Transzendente Funktionen über libm (plattformstabil, docs/engine-design.md §3)
#[cfg(feature = "f64")]
fn atan2(y: Real, x: Real) -> Real {
    libm::atan2(y, x)
}
#[cfg(feature = "f64")]
fn cos(a: Real) -> Real {
    libm::cos(a)
}
#[cfg(feature = "f64")]
fn sin(a: Real) -> Real {
    libm::sin(a)
}
#[cfg(not(feature = "f64"))]
fn atan2(y: Real, x: Real) -> Real {
    libm::atan2f(y, x)
}
#[cfg(not(feature = "f64"))]
fn cos(a: Real) -> Real {
    libm::cosf(a)
}
#[cfg(not(feature = "f64"))]
fn sin(a: Real) -> Real {
    libm::sinf(a)
}

impl Game {
    /// Prüft die Daten (`GameV1::validate`) und legt ein Spiel im Startzustand an.
    pub fn new(data: Arc<GameV1>) -> Result<Self, DataError> {
        data.validate()?;
        let track = Track::new(&data.map);
        let dt = real(data.rules.dt());
        let speed_dt = data.bloons.iter().map(|b| real(b.speed) * dt).collect();
        let mut names: Vec<&String> = data.bloons.iter().flat_map(|b| &b.tags).collect();
        names.sort();
        names.dedup();
        if names.len() > 32 {
            return Err(DataError::Invalid(format!(
                "höchstens 32 verschiedene Bloon-Tags, sind {}",
                names.len()
            )));
        }
        let mut slow_ids: Vec<&String> = data
            .all_extras()
            .into_iter()
            .filter_map(|e| e.slow.as_ref())
            .filter(|s| s.level == 0)
            .map(|s| &s.id)
            .collect();
        slow_ids.sort();
        slow_ids.dedup();
        if slow_ids.len() > CHILL_SLOTS {
            return Err(DataError::Invalid(format!(
                "höchstens {CHILL_SLOTS} verschiedene Verlangsamungen ohne Glue-Stufe, sind {}",
                slow_ids.len()
            )));
        }
        let tags = TagTable {
            tags: names.iter().enumerate().map(|(i, n)| ((*n).clone(), 1u32 << i)).collect(),
            slows: slow_ids.iter().enumerate().map(|(i, n)| ((*n).clone(), i as u8)).collect(),
        };
        let bloon_tags = data.bloons.iter().map(|b| crate::tower::tag_mask(&tags, &b.tags)).collect();
        let mut g = Game {
            track,
            speed_dt,
            tags,
            bloon_tags,
            cash: Money::ZERO,
            lives: 0,
            round: 0,
            towers: Vec::new(),
            next_id: 0,
            bloons: BloonStore::default(),
            projectiles: Vec::new(),
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
        self.projectiles.clear();
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
        self.map_allows(kind, x, y) && !self.occupied(kind, x, y)
    }

    /// Nur der Karten-Teil von `can_place_at`; hängt nicht vom Spielzustand ab und lässt sich
    /// daher vorberechnen.
    pub fn map_allows(&self, kind: TowerKind, x: f64, y: f64) -> bool {
        self.footprint(kind).is_some_and(|r| placeable(&self.data.map, x, y, r))
    }

    /// Nur der Turm-Teil von `can_place_at`: ein anderer Turm steht im Weg.
    pub fn occupied(&self, kind: TowerKind, x: f64, y: f64) -> bool {
        self.footprint(kind).map_or(true, |r| self.collides(x, y, r))
    }

    fn footprint(&self, kind: TowerKind) -> Option<f64> {
        let def = self.data.towers.get(kind as usize)?;
        Some(def.variants[def.variant_id([0; PATHS]).unwrap() as usize].footprint)
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
                self.towers.push(Tower::new(id, kind, def, &self.tags, x, y, price));
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
                t.set_variant(v, &self.tags);
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
        let defs = &data.bloons;

        let lives_before = self.lives;
        let mut pops_total: u32 = 0;
        let mut si = 0;
        // Rundenuhr immer in f64 (auch im f32-Build), sonst driften Spawn-Zeitpunkte über
        // Tausende Additionen von dt. Die Bloon-Physik selbst läuft in `Real`.
        let mut t: f64 = 0.0;
        let mut ticks = 0;
        self.bloons.clear();
        self.projectiles.clear();
        let mut next_id: u32 = 0;
        for tw in &mut self.towers {
            tw.attacks.iter_mut().for_each(|a| (a.cd_left, a.shots, a.burst_left) = (0.0, 0, 0));
        }
        let mut in_range: Vec<usize> = Vec::new();
        let mut hit_list: Vec<usize> = Vec::new();
        // Zonen ändern sich während der Runde nicht
        let zones: Vec<(Real, Real, crate::tower::ZoneRt)> =
            self.towers.iter().flat_map(|t| t.zones.iter().map(|z| (t.x, t.y, z.clone()))).collect();
        let no_extras = ExtrasRt::default();

        while t < rules.max_round_seconds {
            let w = World {
                defs,
                tags: &self.bloon_tags,
                track: &self.track,
                spacing: real(rules.child_spacing),
            };

            // Spawnen
            while si < spawns.len() && spawns[si].0 <= t {
                let g = &groups[spawns[si].1];
                let pos = self.track.pos(0.0, 0);
                self.bloons.push(next_id, g.kind, g.flags, real(defs[g.kind as usize].hp), 0.0, 0, pos);
                next_id += 1;
                si += 1;
            }

            // Bewegen + Durchgelassene (Eingefrorene stehen, Verlangsamte bewegen sich langsamer)
            if !self.bloons.is_empty() {
                let b = &mut self.bloons;
                let mut leaked = 0;
                for i in 0..b.len() {
                    if b.frozen[i] > 0.0 {
                        b.frozen[i] = (b.frozen[i] - dt).max(0.0);
                        continue;
                    }
                    let mut mul: Real = 1.0;
                    if b.slow_left[i] > 0.0 {
                        mul = b.slow_mul[i];
                        b.slow_left[i] -= dt;
                        if b.slow_left[i] <= 0.0 {
                            (b.slow_left[i], b.slow_mul[i], b.glue[i], b.glue_layers[i]) = (0.0, 1.0, 0, 0);
                        }
                    }
                    for ch in b.chill[i].iter_mut().filter(|ch| ch.left > 0.0) {
                        mul *= ch.mul;
                        ch.left -= dt;
                        if ch.left <= 0.0 {
                            *ch = Chill::default();
                        }
                    }
                    if !zones.is_empty() {
                        mul *= zone_scale(&w, b, i, &zones);
                    }
                    b.dist[i] += self.speed_dt[b.kind[i] as usize] * mul;
                    if b.dist[i] >= self.track.length {
                        b.alive[i] = false;
                        leaked += defs[b.kind[i] as usize].leak;
                    } else {
                        b.seg[i] = self.track.segment(b.dist[i], b.seg[i] as usize) as u16;
                        (b.x[i], b.y[i]) = self.track.pos(b.dist[i], b.seg[i] as usize);
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

            // Schaden über Zeit
            if !self.bloons.is_empty() {
                let (pops, removed) = w.tick_dots(&mut self.bloons, &mut next_id, dt, &no_extras);
                pops_total += pops;
                if removed {
                    self.bloons.compact();
                }
            }

            // Türme schießen. Cooldowns laufen jeden Tick, der Überschuss wird übertragen.
            for tw in &mut self.towers {
                for a in &mut tw.attacks {
                    a.cd_left -= dt;
                    if a.burst_left > 0 {
                        a.burst_cd -= dt;
                    }
                }
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
                        // nächster Schuss einer laufenden Salve (Cooldown läuft unabhängig weiter)
                        let burst_shot = atk.burst_left > 0 && atk.burst_cd <= eps;
                        if atk.cd_left > eps && !burst_shot {
                            continue;
                        }
                        let (cx, cy) = (tw.x, tw.y);
                        let b = &self.bloons;
                        in_range.clear();
                        let mut first = usize::MAX;
                        for i in 0..b.len() {
                            let (x, y) = (b.x[i], b.y[i]);
                            if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= atk.range2
                                && w.eligible(b, i, atk.camo, &atk.extras)
                            {
                                if first == usize::MAX || b.dist[i] > b.dist[first] {
                                    first = i; // Targeting "First"; bei Gleichstand der kleinere Index
                                }
                                in_range.push(i);
                            }
                        }
                        if in_range.is_empty() {
                            let a = &mut self.towers[ti].attacks[ai];
                            a.burst_left = 0; // Salve ohne Ziel bricht ab [KAL]
                            if a.cd_left <= eps {
                                a.cd_left = 0.0; // bereit, aber kein Ziel
                            }
                            continue;
                        }
                        // jeder n-te Schuss ist kritisch
                        let shots = atk.shots + 1;
                        let (damage, shots) = match atk.crit {
                            Some((crit, every)) if shots % every == 0 => (crit, 0),
                            Some(_) => (atk.damage, shots),
                            None => (atk.damage, 0),
                        };
                        let (cannot, cooldown, pierce) = (atk.cannot_damage, atk.cooldown, atk.pierce);
                        let extras = atk.extras.clone();
                        let (fx, fy) = (b.x[first], b.y[first]);
                        hit_list.clear();
                        match &atk.kind {
                            AttackKind::Projectile(def) => {
                                let em = &def.emission;
                                let base = if em.fixed { 0.0 } else { atan2(fy - cy, fx - cx) };
                                let clear_every = def.clear_hits.map_or(0, |c| ticks_of(c, dt));
                                for k in 0..em.count {
                                    let a = base + emission_angle(em, k).to_radians();
                                    let (ca, sa) = (cos(a), sin(a));
                                    // parallele Projektile: quer zur Schussrichtung versetzt
                                    let off = em.spread * (k as Real - (em.count - 1) as Real / 2.0);
                                    let (ox, oy) = (cx - off * sa, cy + off * ca);
                                    // lokale +y-Achse der Kurve zeigt in Schussrichtung: Drehung um a - 90°
                                    let rot = a - std::f64::consts::FRAC_PI_2 as Real;
                                    self.projectiles.push(Projectile {
                                        x: ox,
                                        y: oy,
                                        vx: def.speed * ca,
                                        vy: def.speed * sa,
                                        ttl: ticks_of(def.lifespan, dt),
                                        pierce: pierce as Real,
                                        radius: def.radius,
                                        damage,
                                        cannot_damage: cannot,
                                        camo: atk.camo,
                                        extras: extras.clone(),
                                        path: def.path.as_ref().map(|_| (ox, oy, cos(rot), sin(rot), 0.0)),
                                        def: Some(Arc::clone(def)),
                                        clear_every,
                                        clear_left: clear_every,
                                        hit: SmallVec::new(),
                                    });
                                }
                            }
                            AttackKind::Splash { r2 } => {
                                for i in 0..b.len() {
                                    if !b.alive[i] {
                                        continue;
                                    }
                                    let (x, y) = (b.x[i], b.y[i]);
                                    if (x - fx) * (x - fx) + (y - fy) * (y - fy) <= *r2 {
                                        hit_list.push(i);
                                    }
                                }
                            }
                            AttackKind::Area { radius } => {
                                for i in 0..b.len() {
                                    let (x, y) = (b.x[i], b.y[i]);
                                    let rr = *radius + real(defs[b.kind[i] as usize].radius);
                                    if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= rr * rr
                                        && w.eligible(b, i, atk.camo, &extras)
                                    {
                                        hit_list.push(i);
                                    }
                                }
                            }
                            AttackKind::Hitscan => hit_list.extend_from_slice(&in_range),
                        }
                        // stabil absteigend nach Distanz (wie argsort(-dist, kind="stable"))
                        hit_list.sort_by(|&a, &c| b.dist[c].total_cmp(&b.dist[a]));
                        hit_list.truncate(pierce as usize);

                        let a = &mut self.towers[ti].attacks[ai];
                        if burst_shot {
                            a.burst_left -= 1;
                            a.burst_cd = a.burst_cd.max(-dt) + a.burst.map_or(0.0, |b| b.1);
                        } else {
                            a.cd_left = a.cd_left.max(-dt) + cooldown;
                            if let Some((count, interval)) = a.burst.filter(|b| b.0 > 1) {
                                (a.burst_left, a.burst_cd) = (count - 1, interval);
                            }
                        }
                        a.shots = shots;

                        for &i in &hit_list {
                            if let Some(h) =
                                w.apply_hit(&mut self.bloons, &mut next_id, i, damage, cannot, &extras)
                            {
                                pops_total += h.pops;
                                alive = alive - h.removed + h.children;
                            }
                        }
                    }
                }
            }

            // Projektile fliegen und treffen (auch ohne Bloons, damit sie verfallen)
            if !self.projectiles.is_empty() {
                let mut projectiles = std::mem::take(&mut self.projectiles);
                pops_total += step_projectiles(&w, &mut self.bloons, &mut projectiles, dt, &mut next_id);
                self.projectiles = projectiles;
            }
            if !self.bloons.is_empty() {
                self.bloons.compact();
            }

            t += rules.dt();
            ticks += 1;
            if si >= spawns.len() && self.bloons.is_empty() && self.projectiles.is_empty() {
                break;
            }
        }

        self.cash += Money::from_f64(pops_total as f64 * income.cash_per_pop);
        if self.lives > 0 {
            self.round += 1;
            self.cash += Money::from_f64(income.end_bonus);
        }
        self.bloons.clear();
        self.projectiles.clear();
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

/// Ein Tick für alle Projektile: bewegen, Kollisionen in Bloon-Reihenfolge, Kontakt-Effekte,
/// bei verbrauchtem Pierce die Verbrauchs-Effekte. Gibt die Pops zurück.
fn step_projectiles(
    w: &World,
    bloons: &mut BloonStore,
    projectiles: &mut Vec<Projectile>,
    dt: Real,
    next_id: &mut u32,
) -> u32 {
    let mut pops = 0;
    let mut fx = Effects { targets: Vec::new(), spawned: Vec::new() };
    for p in projectiles.iter_mut() {
        p.ttl = p.ttl.saturating_sub(1);
        if p.clear_every > 0 {
            p.clear_left -= 1;
            if p.clear_left == 0 {
                p.hit.clear(); // darf dieselben Bloons erneut treffen
                p.clear_left = p.clear_every;
            }
        }
        if let (Some((ox, oy, rc, rs, s)), Some(path)) =
            (&mut p.path, p.def.as_ref().and_then(|d| d.path.as_ref()))
        {
            *s += (p.vx * p.vx + p.vy * p.vy).sqrt() * dt; // sqrt ist IEEE-exakt, hypot nicht
            if *s >= path.length() {
                p.ttl = 0; // Ende der Kurve
            }
            let (lx, ly) = path.at(*s);
            p.x = *ox + lx * *rc - ly * *rs;
            p.y = *oy + lx * *rs + ly * *rc;
        } else {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
        let def = p.def.clone();
        let contacts = def.as_ref().map_or(&[][..], |d| &d.contacts[..]);
        // Projektile ohne Kollision (Cluster-Splitter) fliegen nur und lösen am Ende aus
        let n = if def.as_ref().map_or(true, |d| d.collide) { bloons.len() } else { 0 };
        for i in 0..n {
            if p.pierce <= 0.0 {
                break;
            }
            // billigster Test zuerst; alle drei sind reine Prüfungen
            let (x, y) = (bloons.x[i], bloons.y[i]);
            let rr = p.radius + real(w.defs[bloons.kind[i] as usize].radius);
            if (x - p.x) * (x - p.x) + (y - p.y) * (y - p.y) > rr * rr
                || p.hit.contains(&bloons.id[i])
                || !w.eligible(bloons, i, p.camo, &p.extras)
            {
                continue;
            }
            p.hit.push(bloons.id[i]);
            p.pierce -= 1.0; // auch ein immuner Bloon verbraucht Pierce [KAL]
            pops += fx.trigger(w, bloons, next_id, contacts, p, dt);
            if p.damage > 0.0 || p.extras.has_effect() || p.extras.push.is_some() {
                let first_child = bloons.len();
                if let Some(h) = w.apply_hit(bloons, next_id, i, p.damage, p.cannot_damage, &p.extras) {
                    pops += h.pops;
                    // Kinder des getroffenen Bloons hat dieses Projektil "schon getroffen"
                    p.hit.extend(bloons.id[first_child..].iter().copied());
                }
            }
        }
        // verbraucht oder abgelaufen [KAL: ob das Spiel auch beim Ablaufen auslöst]
        if p.pierce <= 0.0 || p.ttl == 0 {
            if let Some(d) = def.as_ref().filter(|d| !d.on_exhaust.is_empty()) {
                pops += fx.trigger(w, bloons, next_id, &d.on_exhaust, p, dt);
            }
        }
    }
    projectiles.retain(|p| p.pierce > 0.0 && p.ttl > 0);
    projectiles.extend(fx.spawned); // fliegen ab dem nächsten Tick
    pops
}

/// Puffer für Kontakt- und Verbrauchs-Effekte eines Ticks.
struct Effects {
    targets: Vec<usize>,
    spawned: Vec<Projectile>,
}

impl Effects {
    /// Löst die Effekte an der Position des Projektils aus. Gibt die Pops zurück.
    fn trigger(
        &mut self,
        w: &World,
        bloons: &mut BloonStore,
        next_id: &mut u32,
        contacts: &[ContactRt],
        p: &Projectile,
        dt: Real,
    ) -> u32 {
        let mut pops = 0;
        for c in contacts {
            match c {
                ContactRt::Explosion { radius, pierce, damage, cannot_damage, camo, extras } => {
                    // sofortiger Flächentreffer an der Kontaktstelle, weiteste Bloons zuerst
                    let targets = &mut self.targets;
                    targets.clear();
                    for j in 0..bloons.len() {
                        let (x, y) = (bloons.x[j], bloons.y[j]);
                        let rr = *radius + real(w.defs[bloons.kind[j] as usize].radius);
                        if (x - p.x) * (x - p.x) + (y - p.y) * (y - p.y) <= rr * rr
                            && w.eligible(bloons, j, *camo, extras)
                        {
                            targets.push(j);
                        }
                    }
                    targets.sort_by(|&a, &c| bloons.dist[c].total_cmp(&bloons.dist[a]));
                    targets.truncate(*pierce as usize);
                    for &j in targets.iter() {
                        if let Some(h) = w.apply_hit(bloons, next_id, j, *damage, *cannot_damage, extras) {
                            pops += h.pops;
                        }
                    }
                }
                ContactRt::Shards { def, pierce, damage, cannot_damage, camo, extras, inherit_hit } => {
                    let emission = &def.emission;
                    // neue Projektile ab der Kontaktstelle, Richtungen relativ zur Flugrichtung
                    let base = if emission.fixed { 0.0 } else { atan2(p.vy, p.vx) };
                    for k in 0..emission.count {
                        let a = base + emission_angle(emission, k).to_radians();
                        self.spawned.push(Projectile {
                            x: p.x,
                            y: p.y,
                            vx: def.speed * cos(a),
                            vy: def.speed * sin(a),
                            ttl: ticks_of(def.lifespan, dt),
                            pierce: *pierce as Real,
                            radius: def.radius,
                            damage: *damage,
                            cannot_damage: *cannot_damage,
                            camo: *camo,
                            extras: extras.clone(),
                            def: Some(Arc::clone(def)),
                            path: None,
                            clear_every: 0,
                            clear_left: 0,
                            // Spiel: passOnCollidedWith -> Splitter treffen nichts, was das Projektil schon traf
                            hit: if *inherit_hit { p.hit.clone() } else { SmallVec::new() },
                        });
                    }
                }
            }
        }
        pops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emission_angles() {
        let e = |count, angle| EmissionRt { count, angle, offset: 0.0, fixed: false, spread: 0.0 };
        let a = |em: EmissionRt| (0..em.count).map(|k| emission_angle(&em, k)).collect::<Vec<Real>>();
        assert_eq!(a(e(3, 30.0)), vec![-15.0, 0.0, 15.0], "Teilfächer um die Zielrichtung");
        assert_eq!(a(e(4, 360.0)), vec![0.0, 90.0, 180.0, 270.0], "Rundum ohne doppelte Richtung");
        assert_eq!(a(e(1, 90.0)), vec![0.0]);
    }

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
