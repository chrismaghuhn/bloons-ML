//! Türme: Position, Upgrade-Stand (Variante) und die daraus abgeleiteten Angriffswerte.

use std::collections::HashMap;
use std::sync::Arc;

use btd_data::v1::{
    Contact, Delivery, Emission, HitExtras, HitFilter, Props, TagFactor, TowerDefV1, VariantV1, PATHS,
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
}

pub fn tag_mask(table: &TagTable, names: &[String]) -> u32 {
    names.iter().filter_map(|n| table.tags.get(n)).fold(0, |m, b| m | b)
}

/// (Tag-Maske, Faktor)-Paare.
pub type TagFactors = SmallVec<[(u32, Real); 2]>;

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
    pub damage_vs: TagFactors,
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
            damage_vs: e
                .damage_vs
                .iter()
                .map(|b| (tag_mask(tags, std::slice::from_ref(&b.tag)), real(b.add)))
                .filter(|&(m, _)| m != 0)
                .collect(),
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
    pub extras: ExtrasRt,
    /// Kritischer Treffer: (Schaden, jeder n-te Schuss)
    pub crit: Option<(Real, u32)>,
    pub shots: u32,
    pub cd_left: Real,
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
        extras: ExtrasRt,
    },
    /// Splitter; Flug und eigene Verbrauchs-Effekte stecken in `def`.
    Shards {
        def: Arc<ProjectileDef>,
        pierce: u32,
        damage: Real,
        cannot_damage: Props,
        camo: bool,
        extras: ExtrasRt,
        inherit_hit: bool,
    },
}

fn contact_rt(c: &Contact, tags: &TagTable) -> ContactRt {
    let extras = ExtrasRt::new(c.extras(), tags);
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

#[derive(Clone, Debug, PartialEq)]
pub struct Tower {
    pub id: TowerId,
    pub kind: TowerKind,
    pub tiers: [u8; PATHS],
    pub x: Real,
    pub y: Real,
    pub footprint: f64,
    /// Insgesamt ausgegeben (Basis + Upgrades), Grundlage für den Verkaufswert.
    pub spent: Money,
    pub attacks: SmallVec<[AttackRt; 2]>,
    pub zones: SmallVec<[ZoneRt; 1]>,
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
        };
        t.set_variant(v, tags);
        t
    }

    /// Übernimmt die Werte einer Variante. Cooldowns laufen weiter, soweit es die Angriffe noch gibt.
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
                        Delivery::Area { radius } => AttackKind::Area { radius: radius.map_or(range, real) },
                        Delivery::Projectile {
                            speed,
                            lifespan,
                            radius,
                            emission,
                            on_contact,
                            path,
                            clear_hits,
                            on_exhaust,
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
                        })),
                    },
                    extras: ExtrasRt::new(&a.extras, tags),
                    crit: a.crit.map(|c| (real(c.damage), c.every.max(1))),
                    shots,
                    cd_left,
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
    }

    /// Nächste Upgrade-Kombination auf `path`, falls es dafür eine Variante gibt
    /// (die Crosspath-Regeln stecken in den Daten).
    pub fn next_tiers(&self, path: usize) -> Option<[u8; PATHS]> {
        let mut t = self.tiers;
        *t.get_mut(path)? += 1;
        Some(t)
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
