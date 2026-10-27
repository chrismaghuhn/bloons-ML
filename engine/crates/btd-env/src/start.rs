//! Startzustände: ein Spielstand zwischen zwei Runden, aus dem eine Episode beginnen kann
//! (docs/design-start-states.md). Dazu Störungen (Bridge-Fälle: Cash weicht ab, Upgrade fehlt,
//! Turm steht woanders) und zufällige Builds innerhalb eines Budgets.

use btd_core::{Action, Game, Money, TowerId};
use btd_data::v1::PATHS;
use serde::{Deserialize, Serialize};

/// Ein Turm im Startzustand. Türme stehen in Bau-Reihenfolge (der Core iteriert danach).
/// TODO: Targeting (heute immer "First") und Ability-Cooldowns, sobald die Engine sie kennt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TowerState {
    pub tower: String,
    pub x: f64,
    pub y: f64,
    pub tiers: [u8; PATHS],
}

/// Spielstand vor der nächsten Runde: `round` = Anzahl geschaffter Runden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartState {
    pub round: u32,
    /// Cash in Dollar (Bruchteile erlaubt, wie im Spiel intern).
    pub cash: f64,
    pub lives: i32,
    pub towers: Vec<TowerState>,
}

impl StartState {
    /// Der aktuelle Stand eines Spiels (zwischen den Runden).
    pub fn from_game(game: &Game) -> Self {
        let data = game.data();
        StartState {
            round: game.round,
            cash: game.cash.as_f64(),
            lives: game.lives,
            towers: game
                .towers
                .iter()
                .map(|t| {
                    let (x, y) = t.pos();
                    TowerState { tower: data.towers[t.kind as usize].name.clone(), x, y, tiers: t.tiers }
                })
                .collect(),
        }
    }

    /// Baut den Zustand in `game` auf: Türme mit echten Preisen setzen und upgraden (der
    /// Verkaufswert stimmt so), danach Cash, Leben und Runde. Ungültige Zustände werden abgelehnt.
    pub fn build(&self, game: &mut Game) -> Result<(), String> {
        let rules = game.data().rules.clone();
        if self.round + 1 < rules.start_round || self.round >= rules.end_round {
            return Err(format!(
                "Runde {} außerhalb von {}..{}",
                self.round,
                rules.start_round - 1,
                rules.end_round
            ));
        }
        if self.lives <= 0 || !self.cash.is_finite() || self.cash < 0.0 {
            return Err("Leben müssen > 0 und Cash >= 0 sein".into());
        }
        game.reset();
        game.cash = Money(i64::MAX / 4); // Preise zählen für `spent`, nicht fürs Budget
        for t in &self.towers {
            let kind = game
                .data()
                .towers
                .iter()
                .position(|d| d.name == t.tower)
                .ok_or_else(|| format!("unbekannter Turm {}", t.tower))? as u8;
            let id = game
                .apply(Action::Place { kind, x: t.x, y: t.y })
                .map_err(|e| format!("{} bei ({}, {}): {e:?}", t.tower, t.x, t.y))?
                .expect("Place liefert eine ID");
            upgrade_to(game, id, t.tiers).map_err(|e| format!("{} {:?}: {e}", t.tower, t.tiers))?;
        }
        game.cash = Money::from_f64(self.cash);
        game.lives = self.lives;
        game.round = self.round;
        Ok(())
    }
}

/// Upgradet Turm `id` von 0-0-0 auf `tiers`; der höchste Pfad zuerst, damit jeder Zwischenstand
/// eine erlaubte Kombination ist (z.B. 2-0-5: erst 0-0-5, dann 2-0-5).
pub fn upgrade_to(game: &mut Game, id: TowerId, tiers: [u8; PATHS]) -> Result<(), String> {
    let mut order: Vec<usize> = (0..PATHS).collect();
    order.sort_by_key(|&p| std::cmp::Reverse(tiers[p]));
    for p in order {
        for _ in 0..tiers[p] {
            game.apply(Action::Upgrade { tower: id, path: p as u8 })
                .map_err(|e| format!("Pfad {p}: {e:?}"))?;
        }
    }
    Ok(())
}

/// Würfel für Störungen und Builds (die Env reicht ihren eigenen Zufall durch).
pub trait Dice {
    fn next_u64(&mut self) -> u64;
    /// Gleichverteilt in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Eine Störung eines Startzustands (vor dem Aufbau, außer Verkaufen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perturbation {
    Cash,
    Downgrade,
    Move,
    Sell,
}

const PERTURBATIONS: [Perturbation; 4] =
    [Perturbation::Cash, Perturbation::Downgrade, Perturbation::Move, Perturbation::Sell];

/// Ergebnis von `perturb`: der gestörte Zustand und ob die Leben verändert wurden.
pub struct Perturbed {
    pub state: StartState,
    /// Nach dem Aufbau zu verkaufender Turm (Index in `state.towers`).
    pub sell: Option<usize>,
    pub lives_perturbed: bool,
}

/// Würfelt 1–2 Störungen (Cash, Upgrade zurück mit Erstattung, Verschieben, Verkaufen) und in
/// einem Drittel der Fälle weniger Leben. `cell` ist die Rasterweite für das Verschieben.
/// Ob eine Verschiebung gültig ist, zeigt erst der Aufbau; der Aufrufer würfelt dann neu.
pub fn perturb(base: &StartState, game: &Game, cell: f64, dice: &mut impl Dice) -> Perturbed {
    let data = game.data();
    let mut s = base.clone();
    let mut sell = None;
    let n = 1 + dice.below(2);
    for _ in 0..n {
        match PERTURBATIONS[dice.below(PERTURBATIONS.len())] {
            Perturbation::Cash => {
                // meist kleine Abweichung (Bridge), selten große
                let amp = if dice.unit() < 0.8 { 0.05 + 0.05 * dice.unit() } else { 0.3 };
                let sign = if dice.below(2) == 0 { -1.0 } else { 1.0 };
                s.cash = (s.cash * (1.0 + sign * amp)).max(0.0);
            }
            Perturbation::Downgrade => {
                let upgraded: Vec<usize> =
                    (0..s.towers.len()).filter(|&i| s.towers[i].tiers.iter().any(|&t| t > 0)).collect();
                if upgraded.is_empty() {
                    continue;
                }
                let t = &mut s.towers[upgraded[dice.below(upgraded.len())]];
                let paths: Vec<usize> = (0..PATHS).filter(|&p| t.tiers[p] > 0).collect();
                let p = paths[dice.below(paths.len())];
                let Some(def) = data.towers.iter().find(|d| d.name == t.tower) else { continue };
                t.tiers[p] -= 1;
                // Klick verfehlt: das Geld für das Upgrade ist noch da
                s.cash += data.cost(def.upgrade_costs[p][t.tiers[p] as usize]) as f64;
            }
            Perturbation::Move => {
                if s.towers.is_empty() {
                    continue;
                }
                let i = dice.below(s.towers.len());
                let t = &mut s.towers[i];
                // Nachbarzelle oder leicht versetzt (nicht auf der Zellmitte)
                let (dx, dy) = if dice.below(2) == 0 {
                    let d = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)][dice.below(4)];
                    (d.0 * cell, d.1 * cell)
                } else {
                    ((dice.unit() - 0.5) * cell * 0.8, (dice.unit() - 0.5) * cell * 0.8)
                };
                t.x += dx;
                t.y += dy;
            }
            Perturbation::Sell => {
                if data.rules.selling && !s.towers.is_empty() && sell.is_none() {
                    sell = Some(dice.below(s.towers.len()));
                }
            }
        }
    }
    let lives_perturbed = dice.below(3) == 0;
    if lives_perturbed {
        s.lives = ((s.lives as f64 * (0.5 + 0.5 * dice.unit())).ceil() as i32).max(1);
    }
    Perturbed { state: s, sell, lives_perturbed }
}

/// Gemeinsame Einstellungen aller Envs für Startzustände (von Python gesetzt).
#[derive(Clone, Debug, Default)]
pub struct StartConfig {
    /// Basiszustände (Quellen A und B) und ihre Gewichte.
    pub pool: Vec<StartState>,
    pub weights: Vec<f64>,
    /// Anteil der Episoden, die nicht in Runde 1 beginnen.
    pub p_start: f64,
    /// Davon der Anteil zufälliger Builds (Quelle C).
    pub p_random: f64,
    /// Gesamtwert (Cash + Turmwert) je Anzahl geschaffter Runden, für Quelle C.
    pub budget: Vec<f64>,
    /// Streuung des Budgets (relativ), z.B. 0.3.
    pub budget_spread: f64,
}

impl StartConfig {
    /// Index eines Basiszustands, gewichtet gezogen (`None` bei leerem Pool / Gewichten 0).
    pub fn sample(&self, dice: &mut impl Dice) -> Option<usize> {
        let total: f64 = self.weights.iter().sum();
        if self.pool.is_empty() || total.is_nan() || total <= 0.0 {
            return None;
        }
        let mut x = dice.unit() * total;
        for (i, &w) in self.weights.iter().enumerate() {
            if x < w {
                return Some(i);
            }
            x -= w;
        }
        Some(self.weights.iter().rposition(|&w| w > 0.0).unwrap_or(0))
    }
}

/// Woher die laufende Episode kommt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartKind {
    #[default]
    Normal = 0,
    Pool = 1,
    Random = 2,
}

/// Ergebnis eines Starts aus dem Pool: hat die Episode die nächsten 3 Runden überlebt
/// (ein Sieg zählt als überlebt)?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartOutcome {
    pub kind: StartKind,
    /// Index im Pool (nur bei `StartKind::Pool`).
    pub base: Option<usize>,
    pub survived: bool,
    pub lives_perturbed: bool,
}

/// Runden, die ein Start überleben muss, um als "nicht sofort verloren" zu zählen.
pub const CHECK_ROUNDS: u32 = 3;
