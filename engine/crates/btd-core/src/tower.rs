//! Türme: Position, Upgrade-Stand (Variante) und die daraus abgeleiteten Angriffswerte.

use std::collections::HashMap;
use std::sync::Arc;

use btd_data::v1::{Contact, Delivery, Emission, HitExtras, Props, TowerDefV1, VariantV1, PATHS};
use btd_data::TowerKind;
use smallvec::SmallVec;

use crate::{real, Money, Real};

pub type TowerId = u32;

/// Tag-Name -> Bit (aus den Bloon-Tags der Daten, höchstens 32).
pub type TagTable = HashMap<String, u32>;

pub fn tag_mask(table: &TagTable, names: &[String]) -> u32 {
    names.iter().filter_map(|n| table.get(n)).fold(0, |m, b| m | b)
}

/// Treffer-Extras im Sim-Typ (Tags als Bitmasken).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtrasRt {
    pub exclude_tags: u32,
    pub skip_immune: bool,
    pub skip_frozen: bool,
    pub skip_glue_level: u8,
    pub damage_vs: SmallVec<[(u32, Real); 2]>,
    pub freeze: Option<FreezeRt>,
    pub slow: Option<SlowRt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FreezeRt {
    pub seconds: Real,
    pub layers: u32,
    /// (Tag-Maske, Dauer-Faktor)
    pub scale: SmallVec<[(u32, Real); 1]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlowRt {
    pub multiplier: Real,
    pub seconds: Real,
    pub level: u8,
    pub layers: u32,
    /// (Tag-Maske, abweichender Faktor)
    pub multiplier_for: SmallVec<[(u32, Real); 1]>,
}

impl ExtrasRt {
    pub fn new(e: &HitExtras, tags: &TagTable) -> Self {
        ExtrasRt {
            exclude_tags: tag_mask(tags, &e.filter.exclude_tags),
            skip_immune: e.filter.skip_immune,
            skip_frozen: e.filter.skip_frozen,
            skip_glue_level: e.filter.skip_glue_level,
            damage_vs: e
                .damage_vs
                .iter()
                .map(|b| (tag_mask(tags, std::slice::from_ref(&b.tag)), real(b.add)))
                .filter(|&(m, _)| m != 0)
                .collect(),
            freeze: e.freeze.as_ref().map(|f| FreezeRt {
                seconds: real(f.seconds),
                layers: f.layers,
                scale: f
                    .scale
                    .iter()
                    .map(|t| (tag_mask(tags, std::slice::from_ref(&t.tag)), real(t.factor)))
                    .collect(),
            }),
            slow: e.slow.as_ref().map(|s| SlowRt {
                multiplier: real(s.multiplier),
                seconds: real(s.seconds),
                level: s.level,
                layers: s.layers,
                multiplier_for: s
                    .multiplier_for
                    .iter()
                    .map(|t| (tag_mask(tags, std::slice::from_ref(&t.tag)), real(t.factor)))
                    .collect(),
            }),
        }
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
    pub cd_left: Real,
}

/// Wie der Angriff trifft (Sim-Typ, aus `Delivery`).
#[derive(Clone, Debug, PartialEq)]
pub enum AttackKind {
    /// Sofort bis zu `pierce` Bloons in Reichweite (sim_v0).
    Hitscan,
    /// Sofort am vordersten Ziel explodieren, Radius zum Quadrat (sim_v0-Bomb).
    Splash { r2: Real },
    /// Sofortiger Flächentreffer um den Turm (Ice), Radius = Reichweite.
    Area { range: Real },
    /// Fliegende Projektile.
    Projectile {
        speed: Real,
        lifespan: Real,
        radius: Real,
        emission: EmissionRt,
        /// geteilt: jedes Projektil eines Schusses verweist auf dieselbe Liste
        contacts: Arc<[ContactRt]>,
        path: Option<Arc<PathRt>>,
    },
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
}

/// Effekt bei Kontakt (Sim-Typ, aus `Contact`).
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
    Shards {
        emission: EmissionRt,
        speed: Real,
        lifespan: Real,
        radius: Real,
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
        Contact::Shards { emission, speed, lifespan, radius, pierce, damage, camo, inherit_hit, .. } => {
            ContactRt::Shards {
                emission: emission_rt(emission),
                speed: real(*speed),
                lifespan: real(*lifespan),
                radius: real(*radius),
                pierce: *pierce,
                damage: real(*damage),
                cannot_damage: c.cannot_damage_props(),
                camo: *camo,
                extras,
                inherit_hit: *inherit_hit,
            }
        }
    }
}

fn emission_rt(e: &Emission) -> EmissionRt {
    match e {
        Emission::Single {} => EmissionRt { count: 1, angle: 0.0, offset: 0.0, fixed: false },
        Emission::Arc { count, angle, offset, fixed } => {
            EmissionRt { count: *count, angle: real(*angle), offset: real(*offset), fixed: *fixed }
        }
    }
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
        };
        t.set_variant(v, tags);
        t
    }

    /// Übernimmt die Werte einer Variante. Cooldowns laufen weiter, soweit es die Angriffe noch gibt.
    pub fn set_variant(&mut self, v: &VariantV1, tags: &TagTable) {
        self.tiers = v.tiers;
        self.footprint = v.footprint;
        let old: SmallVec<[Real; 2]> = self.attacks.iter().map(|a| a.cd_left).collect();
        self.attacks = v
            .attacks
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let range = real(a.range.unwrap_or(v.range));
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
                        Delivery::Area {} => AttackKind::Area { range },
                        Delivery::Projectile { speed, lifespan, radius, emission, on_contact, path } => {
                            AttackKind::Projectile {
                                speed: real(*speed),
                                lifespan: real(*lifespan),
                                radius: real(*radius),
                                emission: emission_rt(emission),
                                contacts: on_contact
                                    .iter()
                                    .map(|c| contact_rt(c, tags))
                                    .collect::<Vec<_>>()
                                    .into(),
                                path: path.as_ref().map(|p| Arc::new(PathRt::new(p))),
                            }
                        }
                    },
                    extras: ExtrasRt::new(&a.extras, tags),
                    cd_left: old.get(i).copied().unwrap_or(0.0),
                }
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
