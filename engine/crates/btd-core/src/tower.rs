//! Türme, Upgrades (inkl. Crosspath-Regel) und effektive Werte.

use btd_data::{DamageType, Rules, TowerDef, TowerKind, UpgradeDef};

use crate::{real, Money, Real};

#[derive(Clone, Debug, PartialEq)]
pub struct Tower {
    pub kind: TowerKind,
    pub x: u16,
    pub y: u16,
    pub tiers: [u8; 2],
    /// Insgesamt ausgegeben (Basis + Upgrades), Grundlage für den Verkaufswert.
    pub spent: Money,
    pub cd_left: Real,
    // effektive Werte (durch Upgrades verändert)
    pub range: Real,
    pub cooldown: Real,
    pub damage: i32,
    pub pierce: u32,
    pub dmg_type: DamageType,
    pub blast: Real,
}

impl Tower {
    pub fn new(kind: TowerKind, def: &TowerDef, x: u16, y: u16) -> Self {
        Tower {
            kind,
            x,
            y,
            tiers: [0, 0],
            spent: Money::from_dollars(def.cost),
            cd_left: 0.0,
            range: real(def.range),
            cooldown: real(def.cooldown),
            damage: def.damage,
            pierce: def.pierce,
            dmg_type: def.dmg_type,
            blast: real(def.blast),
        }
    }

    /// Nächstes Upgrade auf `path`, falls erlaubt: max. `max_tier` Stufen und nur ein Pfad
    /// darf über `max_second_path` (wie BTD6 mit 5/2, hier 3/2).
    pub fn next_upgrade<'a>(&self, def: &'a TowerDef, rules: &Rules, path: usize) -> Option<&'a UpgradeDef> {
        let tier = self.tiers[path];
        if tier >= rules.max_tier {
            return None;
        }
        if tier + 1 > rules.max_second_path && self.tiers[1 - path] > rules.max_second_path {
            return None;
        }
        def.upgrades[path].get(tier as usize)
    }

    pub fn apply_upgrade(&mut self, path: usize, up: &UpgradeDef) {
        let fx = &up.fx;
        self.tiers[path] += 1;
        self.spent += Money::from_dollars(up.cost);
        self.pierce = (self.pierce as i64 + fx.pierce as i64).max(0) as u32;
        self.damage += fx.damage;
        self.range += real(fx.range);
        self.blast += real(fx.blast);
        self.cooldown *= real(fx.rate);
        if let Some(t) = fx.dmg_type {
            self.dmg_type = t;
        }
    }

    /// Verkaufswert in ganzen Dollar, abgeschnitten wie `int(spent * ratio)` in der Referenz.
    pub fn sell_value(&self, rules: &Rules) -> Money {
        Money::from_dollars((self.spent.dollars() as f64 * rules.sell_ratio) as i64)
    }
}
