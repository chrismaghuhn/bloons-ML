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
        if self.round.saturating_add(1) < rules.start_round || self.round >= rules.end_round {
            return Err(format!(
                "Runde {} außerhalb von {}..{}",
                self.round,
                rules.start_round - 1,
                rules.end_round
            ));
        }
        if self.lives <= 0 || !(0.0..=MAX_CASH).contains(&self.cash) {
            return Err(format!("Leben müssen > 0 und Cash in 0..={MAX_CASH} sein"));
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

/// Obergrenze für Cash und Budgets (schützt die Fixed-Point-Rechnung vor Überlauf).
pub const MAX_CASH: f64 = 1e9;

/// Footprint-Radius eines Turms mit diesen Stufen (die Kollision im Core nutzt den aktuellen).
fn footprint(game: &Game, t: &TowerState) -> Option<f64> {
    let d = game.data().towers.iter().find(|d| d.name == t.tower)?;
    Some(d.variant(t.tiers)?.footprint)
}

/// Ob Turm `i` von `s` bei (x, y) stehen dürfte (Karte + die anderen Türme des Zustands).
fn can_move(game: &Game, s: &StartState, i: usize, x: f64, y: f64) -> bool {
    let t = &s.towers[i];
    let Some(kind) = game.data().towers.iter().position(|d| d.name == t.tower) else { return false };
    // beim Platzieren zählt der Basis-Footprint, danach der der aktuellen Stufen: beide prüfen
    let base = TowerState { tiers: [0; PATHS], ..t.clone() };
    let (Some(r), Some(r0)) = (footprint(game, t), footprint(game, &base)) else { return false };
    let r = r.max(r0);
    game.map_allows(kind as u8, x, y)
        && s.towers.iter().enumerate().filter(|&(j, _)| j != i).all(|(_, o)| {
            let ro = footprint(game, o).unwrap_or(0.0);
            let (dx, dy) = (x - o.x, y - o.y);
            dx * dx + dy * dy >= (r + ro) * (r + ro)
        })
}

/// Upgradet Turm `id` von 0-0-0 auf `tiers` über erlaubte Zwischenstufen. Welche Reihenfolge
/// geht, hängt von den Daten ab (nicht unterstützte Kombinationen fehlen dort): Breitensuche über
/// die Stufen-Kombinationen, je Schritt ein Pfad +1, jede Zwischenstufe muss eine Variante haben
/// (z.B. Wizard 1-0-2 über 1-0-0 und 1-0-1, wenn 0-0-1 gesperrt ist). Bei mehreren Wegen gewinnt
/// der mit dem kleinsten Pfad zuerst (deterministisch).
pub fn upgrade_to(game: &mut Game, id: TowerId, tiers: [u8; PATHS]) -> Result<(), String> {
    let kind = game.tower(id).ok_or("kein Turm")?.kind as usize;
    let def = &game.data().towers[kind];
    let mut prev: std::collections::HashMap<[u8; PATHS], ([u8; PATHS], usize)> = Default::default();
    let mut queue = std::collections::VecDeque::from([[0u8; PATHS]]);
    while let Some(t) = queue.pop_front() {
        if t == tiers {
            break;
        }
        for p in 0..PATHS {
            if t[p] >= tiers[p] {
                continue;
            }
            let mut n = t;
            n[p] += 1;
            if def.variant(n).is_some() && !prev.contains_key(&n) {
                prev.insert(n, (t, p));
                queue.push_back(n);
            }
        }
    }
    if tiers != [0; PATHS] && !prev.contains_key(&tiers) {
        return Err(format!("kein erlaubter Upgrade-Weg zu {tiers:?}"));
    }
    let mut steps = Vec::new();
    let mut t = tiers;
    while t != [0; PATHS] {
        let (from, p) = prev[&t];
        steps.push(p);
        t = from;
    }
    for p in steps.into_iter().rev() {
        game.apply(Action::Upgrade { tower: id, path: p as u8 }).map_err(|e| format!("Pfad {p}: {e:?}"))?;
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

/// Würfelt 1–2 Störungen (Cash, Upgrade zurück mit Erstattung, Verschieben, Verkaufen; nur
/// anwendbare werden gezogen) und in einem Drittel der Fälle weniger Leben. Verschieben sucht eine
/// bebaubare, freie Position (Nachbarzelle oder leicht versetzt); findet es keine, entfällt es.
/// `cell` ist die Rasterweite.
pub fn perturb(base: &StartState, game: &Game, cell: f64, dice: &mut impl Dice) -> Perturbed {
    let data = game.data();
    let mut s = base.clone();
    let mut sell = None;
    let n = 1 + dice.below(2);
    for _ in 0..n {
        let upgraded: Vec<usize> =
            (0..s.towers.len()).filter(|&i| s.towers[i].tiers.iter().any(|&t| t > 0)).collect();
        let options: Vec<Perturbation> = PERTURBATIONS
            .into_iter()
            .filter(|p| match p {
                Perturbation::Cash => true,
                Perturbation::Downgrade => !upgraded.is_empty(),
                Perturbation::Move => !s.towers.is_empty(),
                Perturbation::Sell => data.rules.selling && !s.towers.is_empty() && sell.is_none(),
            })
            .collect();
        match options[dice.below(options.len())] {
            Perturbation::Cash => {
                // meist kleine Abweichung (Bridge), selten große
                let amp = if dice.unit() < 0.8 { 0.05 + 0.05 * dice.unit() } else { 0.3 };
                let sign = if dice.below(2) == 0 { -1.0 } else { 1.0 };
                s.cash = (s.cash * (1.0 + sign * amp)).clamp(0.0, MAX_CASH);
            }
            Perturbation::Downgrade => {
                let t = &mut s.towers[upgraded[dice.below(upgraded.len())]];
                let paths: Vec<usize> = (0..PATHS).filter(|&p| t.tiers[p] > 0).collect();
                let p = paths[dice.below(paths.len())];
                let Some(def) = data.towers.iter().find(|d| d.name == t.tower) else { continue };
                t.tiers[p] -= 1;
                // Klick verfehlt: das Geld für das Upgrade ist noch da
                s.cash += data.cost(def.upgrade_costs[p][t.tiers[p] as usize]) as f64;
            }
            Perturbation::Move => {
                let i = dice.below(s.towers.len());
                for _ in 0..6 {
                    let (dx, dy) = if dice.below(2) == 0 {
                        let d = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)][dice.below(4)];
                        (d.0 * cell, d.1 * cell)
                    } else {
                        ((dice.unit() - 0.5) * cell * 0.8, (dice.unit() - 0.5) * cell * 0.8)
                    };
                    let (x, y) = (s.towers[i].x + dx, s.towers[i].y + dy);
                    if can_move(game, &s, i, x, y) {
                        (s.towers[i].x, s.towers[i].y) = (x, y);
                        break;
                    }
                }
            }
            Perturbation::Sell => sell = Some(dice.below(s.towers.len())),
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
    /// Basiszustände (Quellen A und B), ihre Gewichte und stabile IDs (für die Ergebnisse; der
    /// Pool kann sich ändern, während Episoden aus dem alten noch laufen). Ohne IDs gilt der Index.
    pub pool: Vec<StartState>,
    pub weights: Vec<f64>,
    pub ids: Vec<u64>,
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
    /// Prüft die Einstellungen: jeder Pool-Zustand muss sich aufbauen lassen (in einer Kopie von
    /// `game`), Gewichte/IDs passen zum Pool, Budgets sind endlich und begrenzt.
    pub fn validate(&self, game: &Game) -> Result<(), String> {
        if self.weights.len() != self.pool.len()
            || !(self.ids.is_empty() || self.ids.len() == self.pool.len())
        {
            return Err("pool, weights (und ids) brauchen gleich viele Einträge".into());
        }
        if self.weights.iter().any(|w| !(w.is_finite() && *w >= 0.0)) {
            return Err("Gewichte müssen endlich und >= 0 sein".into());
        }
        if self.budget.iter().any(|b| !(0.0..=MAX_CASH).contains(b)) || !self.budget_spread.is_finite() {
            return Err(format!("Budgets müssen in 0..={MAX_CASH} liegen"));
        }
        if self.p_start > 0.0 && self.p_random > 0.0 && self.budget.is_empty() {
            return Err("zufällige Builds (p_random > 0) brauchen eine Budgetkurve".into());
        }
        let mut g = game.clone();
        for (i, s) in self.pool.iter().enumerate() {
            s.build(&mut g).map_err(|e| format!("Zustand {i}: {e}"))?;
        }
        Ok(())
    }

    /// Stabile ID des Pool-Eintrags `i`.
    pub fn id(&self, i: usize) -> u64 {
        self.ids.get(i).copied().unwrap_or(i as u64)
    }

    /// Index eines Basiszustands, gewichtet gezogen (`None` bei leerem Pool / Gewichten 0).
    pub fn sample(&self, dice: &mut impl Dice) -> Option<usize> {
        let total: f64 = self.weights.iter().take(self.pool.len()).sum();
        if self.pool.is_empty() || total.is_nan() || total <= 0.0 {
            return None;
        }
        let mut x = dice.unit() * total;
        for (i, &w) in self.weights.iter().take(self.pool.len()).enumerate() {
            if x < w {
                return Some(i);
            }
            x -= w;
        }
        Some(self.weights.iter().take(self.pool.len()).rposition(|&w| w > 0.0).unwrap_or(0))
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
    /// ID des Basiszustands (`StartConfig::id`; nur bei Starts aus dem Pool, nicht bei `reset_from`).
    pub base: Option<u64>,
    pub survived: bool,
    pub lives_perturbed: bool,
}

/// Runden, die ein Start überleben muss, um als "nicht sofort verloren" zu zählen.
pub const CHECK_ROUNDS: u32 = 3;
