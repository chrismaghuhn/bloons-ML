//! Datenformate und Laden/Validieren. Keine Spiellogik.
//!
//! M1 liest `data/sim_v0.json` (Export der Python-Referenz-Sim, `tools/export_sim_data.py`).
//! Namen werden beim Laden in Indizes aufgelöst, sodass der Core nur mit IDs arbeitet.

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

pub type BloonKind = u8;
pub type TowerKind = u8;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("Datei {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ungültige Daten: {0}")]
    Invalid(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DamageType {
    Sharp,
    Explosive,
    Normal,
}

impl DamageType {
    pub const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Bitmaske über [`DamageType::bit`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Immunities(pub u8);

impl Immunities {
    pub fn contains(self, t: DamageType) -> bool {
        self.0 & t.bit() != 0
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Rules {
    pub dt: f64,
    pub cd_eps: f64,
    pub max_round_seconds: f64,
    pub child_spacing: f64,
    pub start_cash: i64,
    pub start_lives: i32,
    pub sell_ratio: f64,
    pub round_bonus_base: i64,
    pub max_tier: u8,
    pub max_second_path: u8,
}

#[derive(Clone, Debug)]
pub struct MapDef {
    pub name: String,
    pub grid_w: usize,
    pub grid_h: usize,
    /// Pfad-Polylinie in Zellkoordinaten.
    pub points: Vec<[f64; 2]>,
    /// Zeilenweise, Index = y * grid_w + x.
    pub path_mask: Vec<bool>,
    pub buildable: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct BloonDef {
    pub name: String,
    pub children: Vec<BloonKind>,
    /// Zellen pro Sekunde.
    pub speed: f64,
    pub rbe: i32,
    pub immune: Immunities,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpgradeEffects {
    pub pierce: i32,
    pub damage: i32,
    pub range: f64,
    pub blast: f64,
    /// Multiplikator auf den Cooldown (1.0 = unverändert).
    pub rate: f64,
    pub dmg_type: Option<DamageType>,
}

#[derive(Clone, Debug)]
pub struct UpgradeDef {
    pub cost: i64,
    pub fx: UpgradeEffects,
}

#[derive(Clone, Debug)]
pub struct TowerDef {
    pub name: String,
    pub cost: i64,
    pub range: f64,
    pub cooldown: f64,
    pub damage: i32,
    pub pierce: u32,
    pub dmg_type: DamageType,
    pub blast: f64,
    /// Zwei Upgrade-Pfade mit je `rules.max_tier` Stufen.
    pub upgrades: [Vec<UpgradeDef>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawn {
    pub time: f64,
    pub kind: BloonKind,
}

#[derive(Clone, Debug)]
pub struct GameData {
    pub rules: Rules,
    pub map: MapDef,
    pub bloons: Vec<BloonDef>,
    pub towers: Vec<TowerDef>,
    pub rounds: Vec<Vec<Spawn>>,
}

// ---------------------------------------------------------------- Rohformat (serde)

#[derive(Deserialize)]
struct RawData {
    format: String,
    rules: Rules,
    map: RawMap,
    bloons: Vec<RawBloon>,
    towers: Vec<RawTower>,
    rounds: Vec<Vec<(f64, String)>>,
}

#[derive(Deserialize)]
struct RawMap {
    name: String,
    grid_w: usize,
    grid_h: usize,
    points: Vec<[f64; 2]>,
    path_mask: Vec<String>,
    buildable: Vec<String>,
}

#[derive(Deserialize)]
struct RawBloon {
    name: String,
    children: Vec<String>,
    speed: f64,
    rbe: i32,
    immune: Vec<DamageType>,
}

#[derive(Deserialize)]
struct RawTower {
    name: String,
    cost: i64,
    range: f64,
    cooldown: f64,
    damage: i32,
    pierce: u32,
    dmg_type: DamageType,
    blast: f64,
    upgrades: Vec<Vec<RawUpgrade>>,
}

#[derive(Deserialize)]
struct RawUpgrade {
    cost: i64,
    effects: RawEffects,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEffects {
    #[serde(default)]
    pierce: i32,
    #[serde(default)]
    damage: i32,
    #[serde(default)]
    range: f64,
    #[serde(default)]
    blast: f64,
    #[serde(default = "one")]
    rate: f64,
    #[serde(default)]
    dmg_type: Option<DamageType>,
}

fn one() -> f64 {
    1.0
}

fn invalid(msg: impl Into<String>) -> DataError {
    DataError::Invalid(msg.into())
}

fn upgrade_path(path: &[RawUpgrade]) -> Vec<UpgradeDef> {
    path.iter()
        .map(|u| UpgradeDef {
            cost: u.cost,
            fx: UpgradeEffects {
                pierce: u.effects.pierce,
                damage: u.effects.damage,
                range: u.effects.range,
                blast: u.effects.blast,
                rate: u.effects.rate,
                dmg_type: u.effects.dmg_type,
            },
        })
        .collect()
}

fn parse_grid(rows: &[String], w: usize, h: usize, what: &str) -> Result<Vec<bool>, DataError> {
    if rows.len() != h || rows.iter().any(|r| r.chars().count() != w) {
        return Err(invalid(format!("{what}: erwartet {h} Zeilen à {w} Zeichen")));
    }
    rows.iter()
        .flat_map(|r| r.chars())
        .map(|c| match c {
            '#' => Ok(true),
            '.' => Ok(false),
            other => Err(invalid(format!("{what}: unbekanntes Zeichen {other:?}"))),
        })
        .collect()
}

impl GameData {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DataError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|source| DataError::Io { path: path.display().to_string(), source })?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> Result<Self, DataError> {
        let raw: RawData = serde_json::from_str(text)?;
        if raw.format != "sim_v0" {
            return Err(invalid(format!("Format {:?} nicht unterstützt", raw.format)));
        }
        if raw.bloons.len() > BloonKind::MAX as usize || raw.towers.len() > TowerKind::MAX as usize {
            return Err(invalid("zu viele Bloon- oder Turmtypen"));
        }
        let bloon_id = |name: &str| -> Result<BloonKind, DataError> {
            raw.bloons
                .iter()
                .position(|b| b.name == name)
                .map(|i| i as BloonKind)
                .ok_or_else(|| invalid(format!("unbekannter Bloon {name:?}")))
        };

        let bloons = raw
            .bloons
            .iter()
            .map(|b| {
                Ok(BloonDef {
                    name: b.name.clone(),
                    children: b.children.iter().map(|c| bloon_id(c)).collect::<Result<_, _>>()?,
                    speed: b.speed,
                    rbe: b.rbe,
                    immune: Immunities(b.immune.iter().fold(0, |m, t| m | t.bit())),
                })
            })
            .collect::<Result<Vec<_>, DataError>>()?;

        let towers = raw
            .towers
            .iter()
            .map(|t| {
                let [p0, p1] = t.upgrades.as_slice() else {
                    return Err(invalid(format!("{}: genau 2 Upgrade-Pfade erwartet", t.name)));
                };
                Ok(TowerDef {
                    name: t.name.clone(),
                    cost: t.cost,
                    range: t.range,
                    cooldown: t.cooldown,
                    damage: t.damage,
                    pierce: t.pierce,
                    dmg_type: t.dmg_type,
                    blast: t.blast,
                    upgrades: [upgrade_path(p0), upgrade_path(p1)],
                })
            })
            .collect::<Result<Vec<_>, DataError>>()?;

        let rounds = raw
            .rounds
            .iter()
            .map(|r| {
                r.iter()
                    .map(|(time, name)| Ok(Spawn { time: *time, kind: bloon_id(name)? }))
                    .collect::<Result<Vec<_>, DataError>>()
            })
            .collect::<Result<Vec<_>, DataError>>()?;

        let m = &raw.map;
        let map = MapDef {
            name: m.name.clone(),
            grid_w: m.grid_w,
            grid_h: m.grid_h,
            points: m.points.clone(),
            path_mask: parse_grid(&m.path_mask, m.grid_w, m.grid_h, "path_mask")?,
            buildable: parse_grid(&m.buildable, m.grid_w, m.grid_h, "buildable")?,
        };

        let data = GameData { rules: raw.rules, map, bloons, towers, rounds };
        data.validate()?;
        Ok(data)
    }

    pub fn validate(&self) -> Result<(), DataError> {
        let r = &self.rules;
        if r.dt.is_nan() || r.dt <= 0.0 || r.max_round_seconds.is_nan() || r.max_round_seconds <= 0.0 {
            return Err(invalid("rules: dt und max_round_seconds müssen > 0 sein"));
        }
        if r.max_second_path > r.max_tier {
            return Err(invalid("rules: max_second_path > max_tier"));
        }
        if self.map.points.len() < 2 {
            return Err(invalid("map: Pfad braucht mindestens 2 Punkte"));
        }
        for b in &self.bloons {
            if b.speed <= 0.0 || b.rbe <= 0 {
                return Err(invalid(format!("Bloon {}: speed und rbe müssen > 0 sein", b.name)));
            }
        }
        for t in &self.towers {
            if t.cost <= 0 || t.cooldown <= 0.0 || t.range <= 0.0 {
                return Err(invalid(format!("Turm {}: cost/cooldown/range müssen > 0 sein", t.name)));
            }
            for (p, path) in t.upgrades.iter().enumerate() {
                if path.len() != r.max_tier as usize {
                    return Err(invalid(format!("Turm {} Pfad {p}: {} Stufen erwartet", t.name, r.max_tier)));
                }
                if path.iter().any(|u| u.cost <= 0 || u.fx.rate <= 0.0) {
                    return Err(invalid(format!("Turm {} Pfad {p}: Kosten/rate müssen > 0 sein", t.name)));
                }
            }
        }
        for (i, round) in self.rounds.iter().enumerate() {
            if round.windows(2).any(|w| w[1].time < w[0].time) {
                return Err(invalid(format!("Runde {}: Spawns nicht nach Zeit sortiert", i + 1)));
            }
        }
        Ok(())
    }
}
