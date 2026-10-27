//! Türme: Position, Upgrade-Stand (Variante) und die daraus abgeleiteten Angriffswerte.

use std::collections::HashMap;
use std::sync::Arc;

use btd_data::v1::{
    Bonus, Contact, Delivery, Emission, HitExtras, HitFilter, Props, TagFactor, Targeting, TimedEffect,
    TowerDefV1, VariantV1, PATHS,
};
use btd_data::TowerKind;
use smallvec::SmallVec;

use crate::{real, Money, Real};

pub type TowerId = u32;

/// Nachschlage-Tabellen aus den Daten: Tag-Name -> Bit (Bloon-Tags, höchstens 32) und
/// Slow-ID -> Slot für Verlangsamungen ohne Glue-Stufe (höchstens `CHILL_SLOTS`).
#[derive(Clone, Debug, Default)]
pub struct TagTable {
    pub tags: HashMap<String, u32>,
    pub slows: HashMap<String, u8>,
    /// Turmname -> Typ und Gruppe eines Zeit-Effekts -> Nummer (Alchemist-Tränke).
    pub towers: HashMap<String, TowerKind>,
    pub groups: HashMap<String, u16>,
}

pub fn tag_mask(table: &TagTable, names: &[String]) -> u32 {
    names.iter().filter_map(|n| table.tags.get(n)).fold(0, |m, b| m | b)
}

/// (Tag-Maske, Faktor)-Paare.
pub type TagFactors = SmallVec<[(u32, Real); 2]>;

/// Bonus-Schaden: (einer dieser Tags, zusätzlich alle diese Tags, Zuschlag).
pub type Bonuses = SmallVec<[(u32, u32, Real); 2]>;

fn bonuses(tags: &TagTable, list: &[Bonus]) -> Bonuses {
    list.iter()
        .map(|b| (tag_mask(tags, std::slice::from_ref(&b.tag)), tag_mask(tags, &b.all), real(b.add)))
        .filter(|&(m, _, _)| m != 0)
        .collect()
}

fn tag_factors(tags: &TagTable, list: &[TagFactor]) -> TagFactors {
    list.iter().map(|t| (tag_mask(tags, std::slice::from_ref(&t.tag)), real(t.factor))).collect()
}

/// Faktor für einen Bloon mit diesen Tags (erster Treffer), sonst `default`.
pub fn factor_for(list: &TagFactors, tags: u32, default: Real) -> Real {
    list.iter().find(|(m, _)| tags & m != 0).map_or(default, |&(_, k)| k)
}

/// Ziel-/Treffer-Filter im Sim-Typ.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilterRt {
    pub exclude_tags: u32,
    pub skip_props: Props,
    pub skip_frozen: bool,
    pub skip_glue_level: u8,
}

impl FilterRt {
    fn new(f: &HitFilter, tags: &TagTable) -> Self {
        FilterRt {
            exclude_tags: tag_mask(tags, &f.exclude_tags),
            skip_props: Props::parse(&f.skip_props, "Filter").unwrap_or_default(),
            skip_frozen: f.skip_frozen,
            skip_glue_level: f.skip_glue_level,
        }
    }
}

/// Treffer-Extras im Sim-Typ (Tags als Bitmasken).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtrasRt {
    pub filter: FilterRt,
    pub damage_vs: Bonuses,
    pub freeze: Option<FreezeRt>,
    pub slow: Option<SlowRt>,
    pub dot: Option<DotRt>,
    pub push: Option<PushRt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FreezeRt {
    pub seconds: Real,
    pub layers: u32,
    /// Dauer-Faktor je Tag
    pub scale: TagFactors,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlowRt {
    pub multiplier: Real,
    pub seconds: Real,
    pub level: u8,
    pub layers: u32,
    /// Slot für Verlangsamungen ohne Glue-Stufe (aus der Slow-ID)
    pub slot: u8,
    /// abweichender Geschwindigkeitsfaktor je Tag
    pub multiplier_for: TagFactors,
    /// abweichende Dauer je Tag
    pub seconds_for: TagFactors,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DotRt {
    pub damage: Real,
    pub interval: Real,
    pub seconds: Real,
    pub layers: u32,
    pub level: u8,
    /// Schadens-Faktor je Tag
    pub factor_for: TagFactors,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PushRt {
    pub amount: Real,
    pub factor_for: TagFactors,
}

impl ExtrasRt {
    pub fn new(e: &HitExtras, tags: &TagTable) -> Self {
        ExtrasRt {
            filter: FilterRt::new(&e.filter, tags),
            damage_vs: bonuses(tags, &e.damage_vs),
            freeze: e.freeze.as_ref().map(|f| FreezeRt {
                seconds: real(f.seconds),
                layers: f.layers,
                scale: tag_factors(tags, &f.scale),
            }),
            slow: e.slow.as_ref().map(|s| SlowRt {
                multiplier: real(s.multiplier),
                seconds: real(s.seconds),
                level: s.level,
                layers: s.layers,
                slot: tags.slows.get(&s.id).copied().unwrap_or(0),
                multiplier_for: tag_factors(tags, &s.multiplier_for),
                seconds_for: tag_factors(tags, &s.seconds_for),
            }),
            dot: e.dot.as_ref().map(|d| DotRt {
                damage: real(d.damage),
                interval: real(d.interval),
                seconds: real(d.seconds),
                layers: d.layers,
                level: d.level,
                factor_for: tag_factors(tags, &d.factor_for),
            }),
            push: e
                .push
                .as_ref()
                .map(|p| PushRt { amount: real(p.amount), factor_for: tag_factors(tags, &p.factor_for) }),
        }
    }

    /// Ob der Treffer Status-Effekte setzt, die auch ohne Schaden wirken.
    pub fn has_effect(&self) -> bool {
        self.freeze.is_some() || self.slow.is_some() || self.dot.is_some()
    }
}

/// Angriffswerte eines Turms im Sim-Typ, aus der aktuellen Variante übernommen.
#[derive(Clone, Debug, PartialEq)]
pub struct AttackRt {
    /// Reichweite zum Quadrat (wie `range ** 2` in der Referenz).
    pub range2: Real,
    pub cooldown: Real,
    pub damage: Real,
    pub pierce: u32,
    pub cannot_damage: Props,
    pub camo: bool,
    pub kind: AttackKind,
    /// Geteilt (Arc), weil jedes Projektil die Extras seines Angriffs mitnimmt.
    pub extras: Arc<ExtrasRt>,
    /// Kritischer Treffer: (Schaden, jeder n-te Schuss)
    pub crit: Option<(Real, u32)>,
    /// Würfelbereich für den Crit-Abstand (nur mit Zufall) und Schüsse bis zum nächsten Crit.
    pub crit_range: Option<(u32, u32)>,
    pub crit_next: u32,
    pub shots: u32,
    pub cd_left: Real,
    /// Salve: (Schüsse je Angriff, Abstand); offene Schüsse der laufenden Salve und Zeit bis zum nächsten.
    pub burst: Option<(u32, Real)>,
    pub burst_left: u32,
    pub burst_cd: Real,
    /// `Targeting::TrackBest`: Explosionsradius, zu dem `Game` den festen Zielpunkt kennt.
    pub track_best: Option<f64>,
}

/// Wie der Angriff trifft (Sim-Typ, aus `Delivery`).
#[derive(Clone, Debug, PartialEq)]
pub enum AttackKind {
    /// Sofort bis zu `pierce` Bloons in Reichweite (sim_v0).
    Hitscan,
    /// Sofort am vordersten Ziel explodieren, Radius zum Quadrat (sim_v0-Bomb).
    Splash { r2: Real },
    /// Sofortiger Flächentreffer um den Turm (Ice, Ring of Fire).
    Area { radius: Real },
    /// Fliegende Projektile.
    Projectile(Arc<ProjectileDef>),
    /// Trank auf einen befreundeten Turm (Zeit-Effekt, `Targeting::Friendly`).
    Potion(Arc<EffectRt>),
}

/// Zeit-Effekt im Sim-Typ (aus `TimedEffect`, docs/design-towers-step3.md §2.4).
#[derive(Clone, Debug, PartialEq)]
pub struct EffectRt {
    pub group: u16,
    pub rank: u32,
    pub seconds: Option<Real>,
    pub shots: u32,
    pub block: Real,
    pub damage: Real,
    pub pierce: u32,
    pub rate_frac: f64,
    pub range_frac: f64,
    pub damage_vs: Bonuses,
    pub exclude: SmallVec<[TowerKind; 4]>,
}

impl EffectRt {
    fn new(e: &TimedEffect, tags: &TagTable) -> Self {
        EffectRt {
            group: tags.groups.get(&e.group).copied().unwrap_or(0),
            rank: e.rank,
            seconds: e.seconds.map(real),
            shots: e.shots,
            block: real(e.block),
            damage: real(e.damage),
            pierce: e.pierce,
            rate_frac: e.rate_frac,
            range_frac: e.range_frac,
            damage_vs: bonuses(tags, &e.damage_vs),
            exclude: e.exclude.iter().filter_map(|n| tags.towers.get(n).copied()).collect(),
        }
    }
}

/// Laufender Zeit-Effekt auf einem Turm: verbleibende Ticks (ohne = bis Rundenende) und Schüsse.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedRt {
    pub def: Arc<EffectRt>,
    pub ticks_left: Option<u32>,
    pub shots_left: u32,
}

/// Summe der laufenden Zeit-Effekte eines Turms (verschiedene Gruppen addieren sich).
#[derive(Clone, Debug, Default, PartialEq)]
struct TimedMods {
    damage: Real,
    pierce: u32,
    rate_frac: f64,
    range_frac: f64,
    damage_vs: Bonuses,
}

impl TimedMods {
    fn of(timed: &[TimedRt]) -> Self {
        let mut m = TimedMods::default();
        for t in timed {
            let d = &t.def;
            m.damage += d.damage;
            m.pierce = m.pierce.saturating_add(d.pierce);
            m.rate_frac += d.rate_frac;
            m.range_frac += d.range_frac;
            m.damage_vs.extend(d.damage_vs.iter().copied());
        }
        m
    }

    fn active(&self) -> bool {
        self.damage != 0.0 || !self.damage_vs.is_empty()
    }

    /// Schaden mit Zuschlag (nur für Treffer, die schon Schaden machen).
    fn damage_of(&self, d: Real) -> Real {
        if self.damage != 0.0 && d > 0.0 {
            d + self.damage
        } else {
            d
        }
    }

    /// Extras mit Bonus-Schaden (nur für Treffer mit Schaden > 0, sonst bekäme z.B. Glue Schaden).
    fn extras_of(&self, e: &Arc<ExtrasRt>, damage: Real) -> Arc<ExtrasRt> {
        if self.damage_vs.is_empty() || damage <= 0.0 {
            return Arc::clone(e);
        }
        let mut x = (**e).clone();
        x.damage_vs.extend(self.damage_vs.iter().copied());
        Arc::new(x)
    }
}

/// Unveränderliche Daten eines Projektil-Angriffs (geteilt von allen Projektilen eines Schusses).
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectileDef {
    pub speed: Real,
    pub lifespan: Real,
    pub radius: Real,
    pub emission: EmissionRt,
    pub contacts: Vec<ContactRt>,
    pub on_exhaust: Vec<ContactRt>,
    pub path: Option<PathRt>,
    pub clear_hits: Option<Real>,
    /// Kollidiert mit Bloons (nur Splitter können es abschalten)
    pub collide: bool,
    pub homing: Option<HomingRt>,
    /// Wurf: fliegt zum Zielpunkt und löst dort `on_exhaust` aus (`collide` ist dann aus).
    pub lob: Option<LobRt>,
}

/// Wurf im Sim-Typ (aus `Delivery::Lob`): feste Flugzeit oder Tempo, Streuung um den Zielpunkt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LobRt {
    pub time: Option<Real>,
    pub speed: Option<Real>,
    pub spread: Real,
}

/// Zielsuche im Sim-Typ: Drehung pro Sekunde (Radiant), Suchradius², cos des halben Kegels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HomingRt {
    pub turn: Real,
    pub dist2: Real,
    pub cos_half: Real,
    pub retarget: bool,
}

/// Kurvenbahn (Boomerang) in lokalen Koordinaten, +y = Richtung zum Ziel.
#[derive(Clone, Debug, PartialEq)]
pub struct PathRt {
    pub xs: Vec<Real>,
    pub ys: Vec<Real>,
    pub cum: Vec<Real>,
}

impl PathRt {
    fn new(points: &[[f64; 2]]) -> Self {
        let xs: Vec<Real> = points.iter().map(|p| real(p[0])).collect();
        let ys: Vec<Real> = points.iter().map(|p| real(p[1])).collect();
        let mut cum = vec![0.0 as Real];
        for i in 1..xs.len() {
            let (dx, dy) = (xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
            cum.push(cum[i - 1] + (dx * dx + dy * dy).sqrt());
        }
        PathRt { xs, ys, cum }
    }

    pub fn length(&self) -> Real {
        *self.cum.last().unwrap()
    }

    /// Lokale Position nach Strecke `s` (lineare Interpolation; Null-Segmente werden übersprungen).
    pub fn at(&self, s: Real) -> (Real, Real) {
        let j = self.cum.partition_point(|&c| c <= s).clamp(1, self.cum.len() - 1);
        let seg = self.cum[j] - self.cum[j - 1];
        let t = if seg > 0.0 { ((s - self.cum[j - 1]) / seg).clamp(0.0, 1.0) } else { 1.0 };
        (
            self.xs[j - 1] + (self.xs[j] - self.xs[j - 1]) * t,
            self.ys[j - 1] + (self.ys[j] - self.ys[j - 1]) * t,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmissionRt {
    pub count: u32,
    /// Gesamtwinkel in Grad (360 = rundum), Versatz in Grad.
    pub angle: Real,
    pub offset: Real,
    /// Richtung unabhängig vom Ziel (Turm dreht sich nicht).
    pub fixed: bool,
    /// > 0: parallele Projektile mit diesem Abstand quer zur Schussrichtung
    pub spread: Real,
}

/// Effekt bei Kontakt oder Ablauf (Sim-Typ, aus `Contact`).
#[derive(Clone, Debug, PartialEq)]
pub enum ContactRt {
    Explosion {
        radius: Real,
        pierce: u32,
        damage: Real,
        cannot_damage: Props,
        camo: bool,
        extras: Arc<ExtrasRt>,
    },
    /// Splitter; Flug und eigene Verbrauchs-Effekte stecken in `def`.
    Shards {
        def: Arc<ProjectileDef>,
        pierce: u32,
        damage: Real,
        cannot_damage: Props,
        camo: bool,
        extras: Arc<ExtrasRt>,
        inherit_hit: bool,
    },
}

fn contact_rt(c: &Contact, tags: &TagTable) -> ContactRt {
    let extras = Arc::new(ExtrasRt::new(c.extras(), tags));
    match c {
        Contact::Explosion { radius, pierce, damage, camo, .. } => ContactRt::Explosion {
            radius: real(*radius),
            pierce: *pierce,
            damage: real(*damage),
            cannot_damage: c.cannot_damage_props(),
            camo: *camo,
            extras,
        },
        Contact::Shards {
            emission,
            speed,
            lifespan,
            radius,
            pierce,
            damage,
            camo,
            inherit_hit,
            on_exhaust,
            collide,
            ..
        } => ContactRt::Shards {
            def: Arc::new(ProjectileDef {
                speed: real(*speed),
                lifespan: real(*lifespan),
                radius: real(*radius),
                emission: emission_rt(emission),
                contacts: Vec::new(),
                on_exhaust: on_exhaust.iter().map(|c| contact_rt(c, tags)).collect(),
                path: None,
                clear_hits: None,
                collide: *collide,
                homing: None,
                lob: None,
            }),
            pierce: *pierce,
            damage: real(*damage),
            cannot_damage: c.cannot_damage_props(),
            camo: *camo,
            extras,
            inherit_hit: *inherit_hit,
        },
    }
}

/// Projektil-Def mit Auren: Camo und "alle Schadensarten" auch für Kontakt- und Verbrauchs-Effekte
/// (z.B. die Explosion der Bomb), das Tempo nur für das Projektil selbst (`top`). Zeit-Effekte geben
/// Schaden und Bonus-Schaden auch an Kontakte weiter.
fn buffed_def(def: &ProjectileDef, b: Buffs, t: &TimedMods, top: bool) -> ProjectileDef {
    let contact = |c: &ContactRt| match c {
        ContactRt::Explosion { radius, pierce, damage, cannot_damage, camo, extras } => {
            ContactRt::Explosion {
                radius: *radius,
                pierce: *pierce,
                damage: t.damage_of(*damage),
                cannot_damage: if b.damage_all { Props(0) } else { *cannot_damage },
                camo: *camo || b.camo,
                extras: t.extras_of(extras, *damage),
            }
        }
        ContactRt::Shards { def, pierce, damage, cannot_damage, camo, extras, inherit_hit } => {
            ContactRt::Shards {
                def: Arc::new(buffed_def(def, b, t, false)),
                pierce: *pierce,
                damage: t.damage_of(*damage),
                cannot_damage: if b.damage_all { Props(0) } else { *cannot_damage },
                camo: *camo || b.camo,
                extras: t.extras_of(extras, *damage),
                inherit_hit: *inherit_hit,
            }
        }
    };
    ProjectileDef {
        speed: if top && b.speed_frac != 0.0 { def.speed * real(1.0 + b.speed_frac) } else { def.speed },
        contacts: def.contacts.iter().map(contact).collect(),
        on_exhaust: def.on_exhaust.iter().map(contact).collect(),
        ..def.clone()
    }
}

fn emission_rt(e: &Emission) -> EmissionRt {
    match e {
        Emission::Single {} => EmissionRt { count: 1, angle: 0.0, offset: 0.0, fixed: false, spread: 0.0 },
        Emission::Arc { count, angle, offset, fixed } => EmissionRt {
            count: *count,
            angle: real(*angle),
            offset: real(*offset),
            fixed: *fixed,
            spread: 0.0,
        },
        Emission::Parallel { count, spread } => {
            EmissionRt { count: *count, angle: 0.0, offset: 0.0, fixed: false, spread: real(*spread) }
        }
    }
}

/// Verlangsamungs-Zone um den Turm (Arctic Wind).
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneRt {
    pub radius: Real,
    pub speed_scale: Real,
    pub camo: bool,
    pub filter: FilterRt,
}

/// Wirksame Auren anderer Türme auf einen Turm (docs/design-towers-step2.md §2.1), berechnet von
/// `Game::refresh_buffs`. `Buffs::NONE` = keine Aura.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buffs {
    /// Reichweite × (1 + `range_frac`) + `range_add`.
    pub range_frac: f64,
    pub range_add: f64,
    /// Cooldown × `rate`.
    pub rate: f64,
    pub camo: bool,
    /// Zusätzlicher Pierce für fliegende Projektile.
    pub pierce: u32,
    /// Projektiltempo × (1 + `speed_frac`).
    pub speed_frac: f64,
    /// Trifft alle Bloon-Arten (MIB).
    pub damage_all: bool,
}

impl Buffs {
    pub const NONE: Buffs = Buffs {
        range_frac: 0.0,
        range_add: 0.0,
        rate: 1.0,
        camo: false,
        pierce: 0,
        speed_frac: 0.0,
        damage_all: false,
    };
}

impl Default for Buffs {
    fn default() -> Self {
        Buffs::NONE
    }
}

/// Die Werte eines Angriffs ohne Auren und Zeit-Effekte: daraus baut `Tower::rebuild` die wirksamen Werte.
#[derive(Clone, Debug, PartialEq)]
struct BaseAttack {
    /// Reichweite und Cooldown in f64 (für die Rechnung mit Auren).
    range: f64,
    cooldown: f64,
    /// Radius eines Flächentreffers, der an die Reichweite gekoppelt ist (wächst mit ihr mit).
    area_radius: Option<f64>,
    range2: Real,
    cooldown_rt: Real,
    damage: Real,
    crit: Option<(Real, u32)>,
    extras: Arc<ExtrasRt>,
    pierce: u32,
    cannot_damage: Props,
    camo: bool,
    kind: AttackKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tower {
    pub id: TowerId,
    pub kind: TowerKind,
    pub tiers: [u8; PATHS],
    pub x: Real,
    pub y: Real,
    pub footprint: f64,
    /// Insgesamt ausgegeben (Basis + Upgrades, mit Rabatt), Grundlage für den Verkaufswert.
    pub spent: Money,
    /// Wirksame Angriffswerte (mit Auren).
    pub attacks: SmallVec<[AttackRt; 2]>,
    pub zones: SmallVec<[ZoneRt; 1]>,
    /// Auren, die gerade auf den Turm wirken. Nur lesen: `Game::refresh_buffs` hält sie zusammen mit
    /// `attacks` und `reach` aktuell.
    pub buffs: Buffs,
    /// Reichweite mit Auren: Radius der eigenen Auren und Rabatt-Zonen (`Game::refresh_buffs`).
    pub reach: f64,
    base: SmallVec<[BaseAttack; 2]>,
    /// Laufende Zeit-Effekte (je Gruppe höchstens einer) und gesperrte Gruppen mit verbleibenden Ticks;
    /// nur innerhalb einer Runde (`Game::play_round` löscht sie am Anfang und Ende).
    pub timed: SmallVec<[TimedRt; 1]>,
    pub blocked: SmallVec<[(u16, u32); 1]>,
}

impl Tower {
    pub fn new(
        id: TowerId,
        kind: TowerKind,
        def: &TowerDefV1,
        tags: &TagTable,
        x: f64,
        y: f64,
        cost: Money,
    ) -> Self {
        let v = def.variant([0; PATHS]).expect("Basisvariante (validiert)");
        let mut t = Tower {
            id,
            kind,
            tiers: [0; PATHS],
            x: real(x),
            y: real(y),
            footprint: v.footprint,
            spent: cost,
            attacks: SmallVec::new(),
            zones: SmallVec::new(),
            buffs: Buffs::NONE,
            reach: v.range,
            base: SmallVec::new(),
            timed: SmallVec::new(),
            blocked: SmallVec::new(),
        };
        t.set_variant(v, tags);
        t
    }

    /// Übernimmt die Werte einer Variante. Cooldowns laufen weiter, soweit es die Angriffe noch gibt.
    /// Die aktuellen Auren (`buffs`) bleiben wirksam.
    pub fn set_variant(&mut self, v: &VariantV1, tags: &TagTable) {
        self.tiers = v.tiers;
        self.footprint = v.footprint;
        let old: SmallVec<[(Real, u32); 2]> = self.attacks.iter().map(|a| (a.cd_left, a.shots)).collect();
        self.attacks = v
            .attacks
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let range = real(a.range.unwrap_or(v.range));
                let (cd_left, shots) = old.get(i).copied().unwrap_or((0.0, 0));
                AttackRt {
                    range2: range * range,
                    cooldown: real(a.cooldown),
                    damage: real(a.damage),
                    pierce: a.pierce,
                    cannot_damage: a.cannot_damage,
                    camo: a.camo,
                    kind: match &a.delivery {
                        Delivery::Hitscan {} => AttackKind::Hitscan,
                        Delivery::HitscanSplash { radius } => {
                            let r = real(*radius);
                            AttackKind::Splash { r2: r * r }
                        }
                        // `radius_offset` zählt erst mit Reichweiten-Buffs (Schritt 2 der Türme)
                        Delivery::Area { radius, .. } => {
                            AttackKind::Area { radius: radius.map_or(range, real) }
                        }
                        Delivery::Projectile {
                            speed,
                            lifespan,
                            radius,
                            emission,
                            on_contact,
                            path,
                            clear_hits,
                            on_exhaust,
                            homing,
                        } => AttackKind::Projectile(Arc::new(ProjectileDef {
                            speed: real(*speed),
                            lifespan: real(*lifespan),
                            radius: real(*radius),
                            emission: emission_rt(emission),
                            contacts: on_contact.iter().map(|c| contact_rt(c, tags)).collect(),
                            on_exhaust: on_exhaust.iter().map(|c| contact_rt(c, tags)).collect(),
                            path: path.as_ref().map(|p| PathRt::new(p)),
                            clear_hits: clear_hits.map(real),
                            collide: true,
                            homing: homing.map(|h| HomingRt {
                                turn: real(h.turn_rate.to_radians()),
                                dist2: real(h.distance * h.distance),
                                cos_half: real(libm::cos((h.max_angle / 2.0).to_radians())),
                                retarget: h.retarget,
                            }),
                            lob: None,
                        })),
                        Delivery::Lob { time, speed, spread, on_arrive } => {
                            AttackKind::Projectile(Arc::new(ProjectileDef {
                                speed: 0.0,
                                lifespan: 0.0,
                                radius: 0.0,
                                emission: emission_rt(&Emission::Single {}),
                                contacts: Vec::new(),
                                on_exhaust: on_arrive.iter().map(|c| contact_rt(c, tags)).collect(),
                                path: None,
                                clear_hits: None,
                                collide: false,
                                homing: None,
                                lob: Some(LobRt {
                                    time: time.map(real),
                                    speed: speed.map(real),
                                    spread: real(*spread),
                                }),
                            }))
                        }
                        Delivery::Potion { effect } => {
                            AttackKind::Potion(Arc::new(EffectRt::new(effect, tags)))
                        }
                    },
                    extras: Arc::new(ExtrasRt::new(&a.extras, tags)),
                    crit: a.crit.map(|c| (real(c.damage), c.every.max(1))),
                    crit_range: a
                        .crit
                        .and_then(|c| c.every_range)
                        .map(|[lo, hi]| (lo.max(1), hi.max(lo.max(1)))),
                    crit_next: a.crit.map_or(0, |c| c.every.max(1)),
                    shots,
                    cd_left,
                    burst: a.burst.map(|b| (b.count, real(b.interval))),
                    burst_left: 0,
                    burst_cd: 0.0,
                    track_best: (a.targeting == Targeting::TrackBest)
                        .then(|| a.delivery.track_best_radius())
                        .flatten(),
                }
            })
            .collect();
        self.zones = v
            .zones
            .iter()
            .map(|z| ZoneRt {
                radius: real(z.radius),
                speed_scale: real(z.speed_scale),
                camo: z.camo,
                filter: FilterRt::new(&z.filter, tags),
            })
            .collect();
        self.base = v
            .attacks
            .iter()
            .zip(&self.attacks)
            .map(|(a, rt)| BaseAttack {
                range: a.range.unwrap_or(v.range),
                cooldown: a.cooldown,
                area_radius: match a.delivery {
                    Delivery::Area { radius: None, .. } => Some(a.range.unwrap_or(v.range)),
                    Delivery::Area { radius: Some(r), radius_offset: Some(_) } => Some(r),
                    _ => None,
                },
                range2: rt.range2,
                cooldown_rt: rt.cooldown,
                damage: rt.damage,
                crit: rt.crit,
                extras: Arc::clone(&rt.extras),
                pierce: rt.pierce,
                cannot_damage: rt.cannot_damage,
                camo: rt.camo,
                kind: rt.kind.clone(),
            })
            .collect();
        self.reach = v.range * (1.0 + self.buffs.range_frac) + self.buffs.range_add;
        if self.buffs != Buffs::NONE || !self.timed.is_empty() {
            self.rebuild();
        }
    }

    /// Für `Game::refresh_buffs`; ein direkter Aufruf wird beim nächsten Refresh nicht unbedingt
    /// zurückgesetzt. Setzt die Auren und schreibt die wirksamen Angriffswerte neu (`rebuild`: Reichweite,
    /// Cooldown, Camo, Schadensarten, Pierce, Schaden, Crit, Extras, Flächenradius, Projektil-Def).
    /// Laufzeit-Zustand (Cooldown, Salve, Crit-Zähler) bleibt unverändert, auch mitten in der Runde. Mit
    /// `Buffs::NONE` und ohne Zeit-Effekte sind die Werte bitgleich zur Variante.
    pub fn apply_buffs(&mut self, b: Buffs) {
        self.buffs = b;
        self.rebuild();
    }

    /// Wirksame Angriffswerte aus Basis, Auren (`buffs`) und Zeit-Effekten (`timed`): Reichweiten-
    /// Anteile und Pierce addieren sich, das Tempo der Zeit-Effekte teilt den Cooldown [KAL]. Ohne
    /// Auren und Zeit-Effekte bitgleich zur Variante.
    fn rebuild(&mut self) {
        let b = self.buffs;
        let t = TimedMods::of(&self.timed);
        let ranged = b.range_frac != 0.0 || b.range_add != 0.0 || t.range_frac != 0.0;
        for (a, base) in self.attacks.iter_mut().zip(&self.base) {
            let range = base.range * (1.0 + b.range_frac + t.range_frac) + b.range_add;
            a.range2 = if ranged { real(range) * real(range) } else { base.range2 };
            a.cooldown = if b.rate != 1.0 || t.rate_frac != 0.0 {
                real(base.cooldown * b.rate / (1.0 + t.rate_frac))
            } else {
                base.cooldown_rt
            };
            a.camo = base.camo || b.camo;
            a.cannot_damage = if b.damage_all { Props(0) } else { base.cannot_damage };
            // Pierce-Aura nur für fliegende Projektile (nicht Sofort-Treffer und Flächen) [KAL]
            let flies = matches!(base.kind, AttackKind::Projectile(_));
            let pierce = b.pierce.saturating_add(t.pierce);
            a.pierce = if flies { base.pierce.saturating_add(pierce) } else { base.pierce };
            a.damage = t.damage_of(base.damage);
            a.crit = base.crit.map(|(d, n)| (t.damage_of(d), n));
            a.extras = t.extras_of(&base.extras, base.damage);
            a.kind = match (&base.kind, base.area_radius) {
                // gekoppelter Radius wächst um denselben Betrag wie die Reichweite
                (AttackKind::Area { .. }, Some(r)) if ranged => {
                    AttackKind::Area { radius: real(r + (range - base.range)) }
                }
                (AttackKind::Projectile(def), _) => {
                    if b.camo || b.damage_all || b.speed_frac != 0.0 || t.active() {
                        AttackKind::Projectile(Arc::new(buffed_def(def, b, &t, true)))
                    } else {
                        AttackKind::Projectile(Arc::clone(def))
                    }
                }
                (k, _) => k.clone(),
            };
        }
    }

    /// Ob der Turm den Zeit-Effekt bekommen darf: hat einen Angriff, ist nicht ausgeschlossen, die
    /// Gruppe ist nicht gesperrt und es läuft kein gleich starker oder stärkerer Effekt der Gruppe.
    pub fn can_receive(&self, e: &EffectRt) -> bool {
        !self.attacks.is_empty()
            && !e.exclude.contains(&self.kind)
            && !self.blocked.iter().any(|&(g, _)| g == e.group)
            && !self.timed.iter().any(|t| t.def.group == e.group && t.def.rank >= e.rank)
    }

    /// Setzt einen Zeit-Effekt (ersetzt einen schwächeren derselben Gruppe).
    pub fn give(&mut self, e: &Arc<EffectRt>, ticks_of: impl Fn(Real) -> u32) {
        self.timed.retain(|t| t.def.group != e.group);
        self.timed.push(TimedRt {
            def: Arc::clone(e),
            ticks_left: e.seconds.map(&ticks_of),
            shots_left: e.shots,
        });
        self.rebuild();
    }

    /// Ein Tick vergeht: Sperren laufen ab, Zeit-Effekte ohne Restzeit enden (dann gesperrt).
    pub fn tick_timed(&mut self, ticks_of: impl Fn(Real) -> u32) {
        self.blocked.retain(|b| {
            b.1 -= 1;
            b.1 > 0
        });
        for t in &mut self.timed {
            if let Some(n) = &mut t.ticks_left {
                *n = n.saturating_sub(1);
            }
        }
        self.expire(ticks_of);
    }

    /// Ein Schuss irgendeines Angriffs: zählt die Rest-Schüsse aller Zeit-Effekte herunter [KAL].
    pub fn on_shot(&mut self, ticks_of: impl Fn(Real) -> u32) {
        for t in &mut self.timed {
            t.shots_left = t.shots_left.saturating_sub(1);
        }
        self.expire(ticks_of);
    }

    fn expire(&mut self, ticks_of: impl Fn(Real) -> u32) {
        let before = self.timed.len();
        let blocked = &mut self.blocked;
        self.timed.retain(|t| {
            let alive = t.shots_left > 0 && t.ticks_left != Some(0);
            if !alive && t.def.block > 0.0 {
                blocked.push((t.def.group, ticks_of(t.def.block)));
            }
            alive
        });
        if self.timed.len() != before {
            self.rebuild();
        }
    }

    /// Löscht Zeit-Effekte und Sperren (Rundenbeginn und -ende).
    pub fn clear_timed(&mut self) {
        self.blocked.clear();
        if !self.timed.is_empty() {
            self.timed.clear();
            self.rebuild();
        }
    }

    /// Nächste Upgrade-Kombination auf `path`, falls es dafür eine Variante gibt
    /// (die Crosspath-Regeln stecken in den Daten).
    pub fn next_tiers(&self, path: usize) -> Option<[u8; PATHS]> {
        let mut t = self.tiers;
        *t.get_mut(path)? += 1;
        Some(t)
    }

    /// Ob (x, y) höchstens `radius` vom Turmmittelpunkt entfernt ist (Auren, Rabatt-Zonen; in f64).
    pub fn reaches(&self, x: f64, y: f64, radius: f64) -> bool {
        let (tx, ty) = self.pos();
        let (dx, dy) = (x - tx, y - ty);
        dx * dx + dy * dy <= radius * radius
    }

    /// Position in f64 (unabhängig vom Float-Build der Sim).
    #[allow(clippy::unnecessary_cast)] // im f64-Build ist `Real` schon f64
    pub fn pos(&self) -> (f64, f64) {
        (self.x as f64, self.y as f64)
    }

    /// Ob der Punkt im Footprint liegt (zum Auswählen eines Turms über eine Position).
    pub fn covers(&self, x: f64, y: f64) -> bool {
        let (tx, ty) = self.pos();
        let (dx, dy) = (x - tx, y - ty);
        dx * dx + dy * dy <= self.footprint * self.footprint
    }

    /// Verkaufswert in ganzen Dollar, abgeschnitten wie `int(spent * ratio)` in der Referenz.
    pub fn sell_value(&self, ratio: f64) -> Money {
        Money::from_dollars((self.spent.dollars() as f64 * ratio) as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_interpolation_skips_zero_segments() {
        let p = PathRt::new(&[[0.0, 0.0], [0.0, 10.0], [0.0, 10.0], [10.0, 10.0]]);
        assert_eq!(p.length(), 20.0);
        assert_eq!(p.at(5.0), (0.0, 5.0));
        assert_eq!(p.at(10.0), (0.0, 10.0));
        assert_eq!(p.at(15.0), (5.0, 10.0));
        assert_eq!(p.at(99.0), (10.0, 10.0));
    }
}
