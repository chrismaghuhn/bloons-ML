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
    /// Kumulierte Pfadlänge je Punkt (`cum[0] == 0`). Kommt aus den Daten statt neu berechnet
    /// zu werden, damit alle Engines exakt dieselben Längen nutzen.
    pub cum: Vec<f64>,
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
    cum: Vec<f64>,
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
        // vor dem Auflösen prüfen, sonst gewinnt bei doppelten Namen stillschweigend der erste
        check_unique(raw.bloons.iter().map(|b| b.name.as_str()), "Bloon")?;
        check_unique(raw.towers.iter().map(|t| t.name.as_str()), "Turm")?;
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
            cum: m.cum.clone(),
            path_mask: parse_grid(&m.path_mask, m.grid_w, m.grid_h, "path_mask")?,
            buildable: parse_grid(&m.buildable, m.grid_w, m.grid_h, "buildable")?,
        };

        let data = GameData { rules: raw.rules, map, bloons, towers, rounds };
        data.validate()?;
        Ok(data)
    }

    /// Prüft alles, worauf sich der Core verlässt. Wird von `from_json` und von `Game::new`
    /// aufgerufen, damit auch von Hand gebaute Daten geprüft sind.
    pub fn validate(&self) -> Result<(), DataError> {
        let r = &self.rules;
        let pos = |x: f64| x.is_finite() && x > 0.0;
        let non_neg = |x: f64| x.is_finite() && x >= 0.0;
        if !pos(r.dt) || !pos(r.max_round_seconds) {
            return Err(invalid("rules: dt und max_round_seconds müssen > 0 sein"));
        }
        if !non_neg(r.cd_eps) || !non_neg(r.child_spacing) {
            return Err(invalid("rules: cd_eps und child_spacing müssen >= 0 sein"));
        }
        if !(r.sell_ratio.is_finite() && (0.0..=1.0).contains(&r.sell_ratio)) {
            return Err(invalid("rules: sell_ratio muss in [0, 1] liegen"));
        }
        if r.start_lives <= 0 || r.start_cash < 0 {
            return Err(invalid("rules: start_lives > 0 und start_cash >= 0 erwartet"));
        }
        if r.max_second_path > r.max_tier {
            return Err(invalid("rules: max_second_path > max_tier"));
        }

        let m = &self.map;
        if m.points.len() < 2 || m.points.len() > u16::MAX as usize {
            return Err(invalid("map: Pfad braucht 2 bis 65535 Punkte"));
        }
        if m.cum.len() != m.points.len()
            || m.cum[0] != 0.0
            || m.cum.iter().any(|c| !c.is_finite())
            || m.cum.windows(2).any(|w| w[1] < w[0])
            || !pos(*m.cum.last().unwrap())
        {
            return Err(invalid("map: cum muss bei 0 beginnen, monoton steigen und zu points passen"));
        }
        if m.points.iter().flatten().any(|v| !v.is_finite()) {
            return Err(invalid("map: points müssen endlich sein"));
        }
        let cells = m.grid_w * m.grid_h;
        if m.path_mask.len() != cells || m.buildable.len() != cells {
            return Err(invalid("map: Raster passen nicht zu grid_w x grid_h"));
        }

        check_unique(self.bloons.iter().map(|b| b.name.as_str()), "Bloon")?;
        check_unique(self.towers.iter().map(|t| t.name.as_str()), "Turm")?;
        for b in &self.bloons {
            if !pos(b.speed) || b.rbe <= 0 {
                return Err(invalid(format!("Bloon {}: speed und rbe müssen > 0 sein", b.name)));
            }
            if b.children.iter().any(|&c| c as usize >= self.bloons.len()) {
                return Err(invalid(format!("Bloon {}: unbekanntes Kind", b.name)));
            }
        }
        self.check_child_cycles()?;

        for t in &self.towers {
            if t.cost <= 0 || !pos(t.cooldown) || !pos(t.range) || !non_neg(t.blast) {
                return Err(invalid(format!(
                    "Turm {}: cost/cooldown/range > 0 und blast >= 0 erwartet",
                    t.name
                )));
            }
            for (p, path) in t.upgrades.iter().enumerate() {
                if path.len() != r.max_tier as usize {
                    return Err(invalid(format!("Turm {} Pfad {p}: {} Stufen erwartet", t.name, r.max_tier)));
                }
                for u in path {
                    let fx = &u.fx;
                    // negative Deltas würden zwischen den Engines unterschiedlich behandelt
                    // (Python-Slicing vs. Klemmen), deshalb gar nicht erst zulassen
                    if u.cost <= 0 || !pos(fx.rate) || fx.pierce < 0 || fx.damage < 0 {
                        return Err(invalid(format!(
                            "Turm {} Pfad {p}: Kosten/rate > 0, pierce/damage-Delta >= 0 erwartet",
                            t.name
                        )));
                    }
                    if !non_neg(fx.range) || !non_neg(fx.blast) {
                        return Err(invalid(format!(
                            "Turm {} Pfad {p}: range/blast-Delta >= 0 erwartet",
                            t.name
                        )));
                    }
                }
            }
        }
        for (i, round) in self.rounds.iter().enumerate() {
            if round.iter().any(|s| !non_neg(s.time) || s.kind as usize >= self.bloons.len()) {
                return Err(invalid(format!("Runde {}: ungültiger Spawn", i + 1)));
            }
            if round.windows(2).any(|w| w[1].time < w[0].time) {
                return Err(invalid(format!("Runde {}: Spawns nicht nach Zeit sortiert", i + 1)));
            }
        }
        Ok(())
    }

    /// Ein Zyklus im Kinder-Graph (z.B. red -> red) würde beim ersten Treffer endlos rekursieren.
    fn check_child_cycles(&self) -> Result<(), DataError> {
        // 0 = unbesucht, 1 = auf dem Stack, 2 = fertig
        fn visit(defs: &[BloonDef], i: usize, state: &mut [u8]) -> bool {
            match state[i] {
                1 => return false,
                2 => return true,
                _ => {}
            }
            state[i] = 1;
            let ok = defs[i].children.iter().all(|&c| visit(defs, c as usize, state));
            state[i] = 2;
            ok
        }
        let mut state = vec![0u8; self.bloons.len()];
        for i in 0..self.bloons.len() {
            if !visit(&self.bloons, i, &mut state) {
                return Err(invalid(format!("Bloon {}: Zyklus in den Kindern", self.bloons[i].name)));
            }
        }
        Ok(())
    }
}

fn check_unique<'a>(names: impl Iterator<Item = &'a str>, what: &str) -> Result<(), DataError> {
    let mut seen = std::collections::HashSet::new();
    for n in names {
        if !seen.insert(n) {
            return Err(invalid(format!("{what} {n:?} ist doppelt")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn minimal() -> Value {
        let up = json!({"cost": 10, "effects": {"pierce": 1}});
        json!({
            "format": "sim_v0",
            "rules": {"dt": 0.05, "cd_eps": 1e-9, "max_round_seconds": 300, "child_spacing": 0.15,
                      "start_cash": 650, "start_lives": 150, "sell_ratio": 0.7, "round_bonus_base": 100,
                      "max_tier": 1, "max_second_path": 1},
            "map": {"name": "m", "grid_w": 2, "grid_h": 1, "points": [[0.0, 0.5], [2.0, 0.5]],
                    "cum": [0.0, 2.0], "path_mask": ["#."], "buildable": [".#"]},
            "bloons": [{"name": "red", "children": [], "speed": 2.5, "rbe": 1, "immune": []},
                       {"name": "blue", "children": ["red"], "speed": 3.5, "rbe": 2, "immune": ["sharp"]}],
            "towers": [{"name": "dart", "cost": 200, "range": 3.0, "cooldown": 1.0, "damage": 1, "pierce": 2,
                        "dmg_type": "sharp", "blast": 0.0, "upgrades": [[up], [up]]}],
            "rounds": [[[0.0, "red"], [0.5, "blue"]]]
        })
    }

    fn load(v: &Value) -> Result<GameData, DataError> {
        GameData::from_json(&v.to_string())
    }

    fn rejects(f: impl FnOnce(&mut Value), needle: &str) {
        let mut v = minimal();
        f(&mut v);
        match load(&v) {
            Err(e) => assert!(e.to_string().contains(needle), "{e} enthält nicht {needle:?}"),
            Ok(_) => panic!("sollte abgelehnt werden: {needle}"),
        }
    }

    #[test]
    fn loads_and_resolves_names() {
        let d = load(&minimal()).unwrap();
        assert_eq!(d.bloons[1].children, vec![0]);
        assert!(d.bloons[1].immune.contains(DamageType::Sharp));
        assert!(!d.bloons[1].immune.contains(DamageType::Explosive));
        assert_eq!(d.rounds[0][1], Spawn { time: 0.5, kind: 1 });
        assert_eq!(d.map.path_mask, vec![true, false]);
        assert_eq!(d.towers[0].upgrades[0][0].fx.rate, 1.0); // Standardwert
    }

    #[test]
    fn floats_roundtrip_exactly() {
        // 0.9843489761028231 wird ohne serde_json/float_roundtrip um 1 ULP falsch gelesen
        let mut v = minimal();
        v["rounds"][0][1][0] = json!(0.9843489761028231);
        let d = load(&v).unwrap();
        assert_eq!(d.rounds[0][1].time.to_bits(), 0.9843489761028231f64.to_bits());
    }

    #[test]
    fn rejects_invalid_data() {
        rejects(|v| v["format"] = json!("x"), "Format");
        rejects(|v| v["bloons"][0]["children"] = json!(["green"]), "unbekannter Bloon");
        rejects(|v| v["bloons"][0]["children"] = json!(["blue"]), "Zyklus");
        rejects(|v| v["bloons"][1]["name"] = json!("red"), "doppelt");
        rejects(|v| v["map"]["path_mask"] = json!(["#"]), "path_mask");
        rejects(|v| v["map"]["buildable"] = json!(["x."]), "unbekanntes Zeichen");
        rejects(|v| v["map"]["cum"] = json!([0.0, 1.0, 2.0]), "cum");
        rejects(|v| v["map"]["cum"] = json!([0.0, 0.0]), "cum");
        rejects(|v| v["rules"]["dt"] = json!(0.0), "dt");
        rejects(|v| v["rules"]["sell_ratio"] = json!(1.5), "sell_ratio");
        rejects(|v| v["rules"]["max_second_path"] = json!(2), "max_second_path");
        rejects(|v| v["towers"][0]["upgrades"] = json!([[]]), "2 Upgrade-Pfade");
        rejects(|v| v["towers"][0]["upgrades"][0] = json!([]), "Stufen");
        rejects(|v| v["towers"][0]["upgrades"][0][0]["effects"] = json!({"pierce": -1}), "Delta");
        rejects(|v| v["towers"][0]["upgrades"][0][0]["effects"] = json!({"speed": 1}), "unknown field");
        rejects(|v| v["rounds"][0] = json!([[1.0, "red"], [0.5, "red"]]), "sortiert");
    }
}
