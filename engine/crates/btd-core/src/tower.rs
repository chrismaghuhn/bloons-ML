//! Türme: Position, Upgrade-Stand (Variante) und die daraus abgeleiteten Angriffswerte.

use btd_data::v1::{Contact, Delivery, Emission, Props, TowerDefV1, VariantV1, PATHS};
use btd_data::TowerKind;
use smallvec::SmallVec;

use crate::{real, Money, Real};

pub type TowerId = u32;

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
    pub cd_left: Real,
}

/// Wie der Angriff trifft (Sim-Typ, aus `Delivery`).
#[derive(Clone, Debug, PartialEq)]
pub enum AttackKind {
    /// Sofort bis zu `pierce` Bloons in Reichweite (sim_v0).
    Hitscan,
    /// Sofort am vordersten Ziel explodieren, Radius zum Quadrat (sim_v0-Bomb).
    Splash { r2: Real },
    /// Fliegende Projektile.
    Projectile {
        speed: Real,
        lifespan: Real,
        radius: Real,
        emission: EmissionRt,
        contacts: SmallVec<[ContactRt; 2]>,
    },
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContactRt {
    Explosion {
        radius: Real,
        pierce: u32,
        damage: Real,
        cannot_damage: Props,
        camo: bool,
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
    },
}

fn contact_rt(c: &Contact) -> ContactRt {
    match c {
        Contact::Explosion { radius, pierce, damage, camo, .. } => ContactRt::Explosion {
            radius: real(*radius),
            pierce: *pierce,
            damage: real(*damage),
            cannot_damage: c.cannot_damage_props(),
            camo: *camo,
        },
        Contact::Shards { emission, speed, lifespan, radius, pierce, damage, camo, .. } => {
            ContactRt::Shards {
                emission: emission_rt(emission),
                speed: real(*speed),
                lifespan: real(*lifespan),
                radius: real(*radius),
                pierce: *pierce,
                damage: real(*damage),
                cannot_damage: c.cannot_damage_props(),
                camo: *camo,
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
    pub fn new(id: TowerId, kind: TowerKind, def: &TowerDefV1, x: f64, y: f64, cost: Money) -> Self {
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
        t.set_variant(v);
        t
    }

    /// Übernimmt die Werte einer Variante. Cooldowns laufen weiter, soweit es die Angriffe noch gibt.
    pub fn set_variant(&mut self, v: &VariantV1) {
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
                        Delivery::Projectile { speed, lifespan, radius, emission, on_contact } => {
                            AttackKind::Projectile {
                                speed: real(*speed),
                                lifespan: real(*lifespan),
                                radius: real(*radius),
                                emission: emission_rt(emission),
                                contacts: on_contact.iter().map(contact_rt).collect(),
                            }
                        }
                    },
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
