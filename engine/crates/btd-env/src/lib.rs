//! RL-Schicht über `btd-core`: Aktionsraum, Masken, Observation, Rewards, Vektor-Env und
//! Spielprotokolle (Replays).
//!
//! Mit dem sim_v0-Datensatz sind Aktionsraum, Observation und Rewards identisch zu
//! `btd_sim/env.py`, damit Python- und Rust-Env austauschbar sind (Paritätstest in
//! `tests/test_rust_env.py`). Die Runden-Vorschau (`EnvConfig::preview_rounds`) ist nur für die
//! Spieldaten gedacht und erweitert die Observation; ohne sie bleibt die Parität bestehen.

pub mod preview;
pub mod replay;
pub mod start;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use btd_core::{Action, Buffs, Game, Money, RoundReport};
use btd_data::v1::GameV1;
use btd_data::DataError;
use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_64_with_seed;

pub use preview::RoundPreview;
pub use replay::{Recorder, Replay, ReplayError};
pub use start::{StartConfig, StartKind, StartOutcome, StartState};

use std::collections::VecDeque;

use start::{Dice, CHECK_ROUNDS};

/// Einkommen je Runde, bei dem der Einkommens-Skalar 1 erreicht.
const INCOME_NORM: f64 = 20_000.0;

/// Seed für den Hash der Datendatei im Replay-Header.
const DATA_HASH_SEED: u64 = 0xDA7A;

/// Geladene Spieldaten plus Hash der Quelldatei (damit Replays nicht stillschweigend mit
/// anderen Daten abgespielt werden).
#[derive(Clone, Debug)]
pub struct EnvSpec {
    pub data: Arc<GameV1>,
    pub data_hash: u64,
    pub data_name: String,
}

impl EnvSpec {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DataError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|source| DataError::Io { path: path.display().to_string(), source })?;
        Ok(EnvSpec {
            data: Arc::new(GameV1::from_json(&text)?),
            // Zeilenenden normalisieren, damit der Hash auf Windows (autocrlf) gleich ist
            data_hash: xxh3_64_with_seed(text.replace("\r\n", "\n").as_bytes(), DATA_HASH_SEED),
            data_name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct EnvConfig {
    pub max_builds_per_round: u32,
    pub reward_round: f64,
    pub reward_life: f64,
    pub reward_win: f64,
    pub reward_loss: f64,
    /// Reward je gebautem Turm (negativ = Strafe für viele Türme; 0 = aus, wie die Python-Env).
    pub reward_tower: f64,
    /// Spielprotokoll mitschreiben (`Env::replay`, `Env::last_replay`).
    pub record: bool,
    /// Runden-Vorschau in der Observation: so viele kommende Runden plus "Runden bis zum
    /// nächsten Spezial-Bloon" (0 = aus, wie die Python-Env).
    pub preview_rounds: usize,
    /// Bis zu so viele Turmtypen werden je Episode zufällig gesperrt (0 = aus, wie die
    /// Python-Env). Die Observation bekommt dann je Turm einen Wert "verfügbar".
    pub max_banned_towers: usize,
    /// Seed für die Sperren (VecEnv leitet daraus je Env einen eigenen ab).
    pub seed: u64,
    /// Zufall im Spiel (z.B. Crit-Abstände): jede Episode bekommt einen eigenen Seed aus dem
    /// Env-Zufall. Aus = Mittelwerte (deterministisch, Parität zur Python-Env).
    pub randomness: bool,
}

impl Default for EnvConfig {
    fn default() -> Self {
        // Werte wie in btd_sim/env.py
        EnvConfig {
            max_builds_per_round: 6,
            reward_round: 1.0,
            reward_life: -0.02,
            reward_win: 10.0,
            reward_loss: -5.0,
            reward_tower: 0.0,
            record: false,
            preview_rounds: 0,
            max_banned_towers: 0,
            seed: 0,
            randomness: false,
        }
    }
}

/// Aktionsraum (Discrete), N = Rasterzellen der Karte, T = Turmtypen, P = Upgrade-Pfade:
/// `0` Runde starten, `1 + k*N + c` Turm k in Zelle c bauen,
/// `1 + T*N + p*N + c` Pfad p des Turms in Zelle c upgraden, `1 + T*N + P*N + c` Verkaufen.
/// Eine Zelle steht für ihren Mittelpunkt in Weltkoordinaten.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub grid_w: usize,
    pub grid_h: usize,
    pub cells: usize,
    pub towers: usize,
    /// Anzahl Upgrade-Pfade, die mindestens ein Turm hat (sim_v0: 2, Spiel: 3).
    pub paths: usize,
    /// Höchste Upgrade-Stufe (für die Normierung in der Observation).
    pub max_tier: usize,
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub a_place: usize,
    pub a_upgrade: usize,
    pub a_sell: usize,
    pub n_actions: usize,
    /// Daten mit Support-Türmen (Village/Farm): 5 Buff-Kanäle, Sperr-Skalare immer, Einkommens-Skalar
    /// (docs/design-towers-step2.md §4). Ohne sie bleibt das Layout wie bisher.
    pub support: bool,
    pub n_channels: usize,
    pub n_scalars: usize,
    pub obs_size: usize,
}

/// Aktion im Raster, bevor sie auf Weltkoordinaten bzw. Turm-IDs abgebildet wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellAction {
    Place { kind: u8, cell: usize },
    Upgrade { cell: usize, path: u8 },
    Sell { cell: usize },
}

impl Layout {
    pub fn new(data: &GameV1, preview_rounds: usize, tower_bans: bool) -> Self {
        let g = &data.map.grid;
        let cells = g.w * g.h;
        let towers = data.towers.len();
        let paths = data
            .towers
            .iter()
            .map(|t| t.upgrade_costs.iter().rposition(|c| !c.is_empty()).map_or(0, |p| p + 1))
            .max()
            .unwrap_or(0);
        let max_tier =
            data.towers.iter().flat_map(|t| t.upgrade_costs.iter().map(Vec::len)).max().unwrap_or(0);
        let a_place = 1;
        let a_upgrade = a_place + towers * cells;
        let a_sell = a_upgrade + paths * cells;
        let support = has_support_towers(data);
        // Pfad, baubar, Turmtyp one-hot, Tier je Pfad; mit Support-Türmen dazu die Buff-Kanäle
        let n_channels = 2 + towers + paths + if support { BUFF_CHANNELS } else { 0 };
        let preview = if preview_rounds > 0 { RoundPreview::len(preview_rounds) } else { 0 };
        let n_scalars = 4 + preview + if tower_bans || support { towers } else { 0 } + support as usize;
        Layout {
            grid_w: g.w,
            grid_h: g.h,
            cells,
            towers,
            paths,
            max_tier: max_tier.max(1),
            x0: g.x0,
            y0: g.y0,
            cell: g.cell,
            a_place,
            a_upgrade,
            a_sell,
            n_actions: a_sell + cells,
            support,
            n_channels,
            n_scalars,
            obs_size: n_channels * cells + n_scalars,
        }
    }

    /// Aktionsindex -> Rasteraktion (`None` für "Runde starten" und ungültige Indizes).
    pub fn decode(&self, a: usize) -> Option<CellAction> {
        if a == 0 || a >= self.n_actions {
            return None;
        }
        Some(if a < self.a_upgrade {
            let i = a - self.a_place;
            CellAction::Place { kind: (i / self.cells) as u8, cell: i % self.cells }
        } else if a < self.a_sell {
            let i = a - self.a_upgrade;
            CellAction::Upgrade { cell: i % self.cells, path: (i / self.cells) as u8 }
        } else {
            CellAction::Sell { cell: a - self.a_sell }
        })
    }

    /// Mittelpunkt einer Zelle in Weltkoordinaten.
    pub fn center(&self, c: usize) -> (f64, f64) {
        let (cx, cy) = ((c % self.grid_w) as f64, (c / self.grid_w) as f64);
        (self.x0 + (cx + 0.5) * self.cell, self.y0 + (cy + 0.5) * self.cell)
    }

    /// Zelle unter einer Weltposition.
    pub fn cell_of(&self, x: f64, y: f64) -> Option<usize> {
        let (cx, cy) = (((x - self.x0) / self.cell).floor(), ((y - self.y0) / self.cell).floor());
        (cx >= 0.0 && cy >= 0.0 && cx < self.grid_w as f64 && cy < self.grid_h as f64)
            .then(|| cy as usize * self.grid_w + cx as usize)
    }
}

/// Buff-Kanäle je Zelle: Reichweite, Tempo, Camo, MIB, Rabatt.
pub const BUFF_CHANNELS: usize = 5;

/// Ob die Daten Support-Türme haben (Auren, Rabatt oder Einkommen an irgendeiner Variante).
pub fn has_support_towers(data: &GameV1) -> bool {
    data.towers
        .iter()
        .flat_map(|t| &t.variants)
        .any(|v| !v.supports.is_empty() || !v.discount.is_empty() || v.income.is_some())
}

/// Auren und Platzier-Rabatt je Zelle (am Zellmittelpunkt), gecacht bis sich die Auren ändern
/// (`Game::buff_epoch`). Für die Place-Maske und die Buff-Kanäle.
#[derive(Clone, Debug, Default)]
struct Raster {
    epoch: u64,
    aura: Vec<Buffs>,
    discount: Vec<f64>,
}

/// Cache hinter `&self` (Maske und Observation). Mutex statt RefCell, weil die VecEnv die Envs
/// parallel über `&Env` maskiert; je Env gibt es nie Konkurrenz um die Sperre.
#[derive(Debug, Default)]
struct RasterCache(Mutex<Raster>);

impl Clone for RasterCache {
    fn clone(&self) -> Self {
        RasterCache(Mutex::new(self.0.lock().unwrap().clone()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepInfo {
    pub reward: f64,
    pub terminated: bool,
    /// Eine Runde wurde gespielt (Aktion 0 oder Bau-Limit erreicht).
    pub round_played: Option<RoundReport>,
    /// Bauaktion war ungültig (sollte durch die Maske nie passieren).
    pub invalid: bool,
}

#[derive(Clone, Debug)]
pub struct Env {
    pub game: Game,
    pub layout: Layout,
    pub cfg: EnvConfig,
    pub builds_left: u32,
    spec: EnvSpec,
    preview: Option<RoundPreview>,
    /// Karten-Bebaubarkeit je (Turm, Zelle), einmal vorberechnet (Polygon-Tests sind teuer).
    map_ok: Arc<Vec<bool>>,
    /// In dieser Episode gesperrte Turmtypen (`EnvConfig::max_banned_towers`). Sperren verbieten nur
    /// das Bauen; Replays speichern sie nicht, geladene Türme gesperrter Typen bleiben nutzbar.
    banned: Vec<bool>,
    rng: u64,
    /// Startzustände (alle Envs teilen sich die Einstellungen), Herkunft der laufenden Episode,
    /// noch nicht abgeholte Ergebnisse und Zustände zu Rundenbeginn (Quelle A).
    starts: Option<Arc<StartConfig>>,
    start: EpisodeStart,
    outcomes: Vec<StartOutcome>,
    snapshots: VecDeque<StartState>,
    /// Ein zufällig gewählter Rundenbeginn der laufenden normalen Episode (Reservoir-Stichprobe).
    candidate: Option<StartState>,
    /// Starts aus Zuständen, die nicht geklappt haben (normale Episode statt dessen).
    fallbacks: u64,
    recorder: Option<Recorder>,
    last_replay: Option<Replay>,
    raster: RasterCache,
}

impl Env {
    pub fn new(spec: EnvSpec, cfg: EnvConfig) -> Result<Self, DataError> {
        if cfg.max_builds_per_round == 0 {
            return Err(DataError::Invalid("max_builds_per_round muss > 0 sein".into()));
        }
        let game = Game::new(Arc::clone(&spec.data))?;
        if cfg.record && cfg.randomness {
            // Replays speichern den Seed nicht und würden beim Nachspielen abweichen
            return Err(DataError::Invalid(
                "record und randomness zusammen werden noch nicht unterstützt".into(),
            ));
        }
        let free = (0..spec.data.towers.len()).filter(|&k| !game.is_locked(k as u8)).count();
        if cfg.max_banned_towers >= free.max(1) {
            return Err(DataError::Invalid(
                "max_banned_towers muss kleiner als die Zahl der nicht gesperrten Türme sein".into(),
            ));
        }
        let layout = Layout::new(&spec.data, cfg.preview_rounds, cfg.max_banned_towers > 0);
        let preview = (cfg.preview_rounds > 0).then(|| RoundPreview::new(&spec.data, cfg.preview_rounds));
        let recorder = cfg.record.then(|| Recorder::new(&spec, &cfg));
        let map_ok = (0..layout.towers * layout.cells)
            .map(|i| {
                let (x, y) = layout.center(i % layout.cells);
                game.map_allows((i / layout.cells) as u8, x, y)
            })
            .collect();
        Ok(Env {
            map_ok: Arc::new(map_ok),
            banned: vec![false; layout.towers],
            rng: splitmix64(cfg.seed),
            starts: None,
            start: EpisodeStart::default(),
            outcomes: Vec::new(),
            snapshots: VecDeque::new(),
            candidate: None,
            fallbacks: 0,
            builds_left: cfg.max_builds_per_round,
            game,
            layout,
            cfg,
            spec,
            preview,
            recorder,
            last_replay: None,
            raster: RasterCache::default(),
        })
    }

    /// Das Raster für den aktuellen Spielstand (neu gerechnet, wenn sich die Auren geändert haben).
    fn raster(&self) -> MutexGuard<'_, Raster> {
        let epoch = self.game.buff_epoch();
        let mut r = self.raster.0.lock().unwrap();
        if r.epoch != epoch {
            let l = &self.layout;
            r.epoch = epoch;
            r.aura = self.game.auras_at((0..l.cells).map(|c| l.center(c)));
            r.discount = (0..l.cells)
                .map(|c| l.center(c))
                .map(|(x, y)| self.game.discount_at(x, y, 0, None))
                .collect();
        }
        r
    }

    /// Turmtyp in dieser Episode nicht baubar (zufällig gesperrt oder in den Regeln gesperrt)?
    fn unavailable(&self, kind: usize) -> bool {
        self.banned.get(kind) == Some(&true) || self.game.is_locked(kind as u8)
    }

    pub fn spec(&self) -> &EnvSpec {
        &self.spec
    }

    /// Neuer Seed für die Sperren (VecEnv: je Env ein eigener).
    pub fn reseed(&mut self, seed: u64) {
        self.rng = splitmix64(seed);
    }

    /// Gesperrte Turmtypen der laufenden Episode.
    pub fn banned(&self) -> &[bool] {
        &self.banned
    }

    /// Reset mit festen Sperren statt gewürfelten (Auswertung je Sperre).
    pub fn reset_with_bans(&mut self, obs: &mut [f32], banned: &[bool]) {
        assert_eq!(banned.len(), self.layout.towers, "eine Sperre je Turmtyp");
        self.reset(obs);
        self.banned.copy_from_slice(banned);
        self.observe(obs);
    }

    pub fn reset(&mut self, obs: &mut [f32]) {
        self.reset_inner(true);
        self.observe(obs);
    }

    fn reset_inner(&mut self, draw_start: bool) {
        if let Some(c) = self.candidate.take() {
            if self.snapshots.len() == SNAPSHOTS {
                self.snapshots.pop_front();
            }
            self.snapshots.push_back(c);
        }
        self.draw_bans();
        self.game.reset();
        if self.cfg.randomness {
            let seed = self.next_rand();
            self.game.set_seed(Some(seed));
        }
        self.start = EpisodeStart::default();
        if let Some(sc) = self.starts.clone().filter(|_| draw_start) {
            if self.unit() < sc.p_start {
                self.start_from_config(&sc);
            }
        }
        self.builds_left = self.cfg.max_builds_per_round;
        // Episoden aus einem Startzustand lassen sich nicht ab Runde 1 nachspielen: kein Replay
        self.recorder = (self.cfg.record && self.start.kind == StartKind::Normal)
            .then(|| Recorder::new(&self.spec, &self.cfg));
    }

    /// Neue Episode ab einem festen Startzustand (ohne Störung), z.B. für die Auswertung.
    /// Bei ungültigem Zustand bleibt die Env im normalen Startzustand.
    /// Ohne Replay (es ließe sich nicht ab Runde 1 nachspielen) und ohne Turm-Sperren (die
    /// Auswertung soll nur den Zustand messen, nicht zusätzlich gewürfelte Sperren).
    pub fn reset_from(&mut self, obs: &mut [f32], state: &StartState) -> Result<(), String> {
        self.reset_inner(false);
        self.banned.fill(false);
        let res = state.build(&mut self.game);
        if res.is_err() {
            self.reset_inner(false);
            self.banned.fill(false);
        } else {
            self.start = EpisodeStart { kind: StartKind::Pool, ..EpisodeStart::default() };
            self.recorder = None;
        }
        self.observe(obs);
        res
    }

    /// Vorausschau (docs/design-lookahead.md): auf einer Kopie optional `action` ausführen und danach
    /// bis zu `rounds` Runden ohne weitere Aktionen spielen. Die Env selbst bleibt unverändert; die
    /// Kopie hat denselben Zufallszustand, die Vorhersage passt also zur laufenden Episode.
    pub fn lookahead(&self, action: Option<usize>, rounds: u32) -> Lookahead {
        let mut sim = self.clone();
        sim.recorder = None;
        sim.starts = None; // keine Snapshots/Ergebnisse aus der Kopie
        let mut obs = vec![0.0; self.layout.obs_size];
        let lives_before = sim.game.lives;
        let mut out = Lookahead { invalid: false, ..Lookahead::default() };
        if let Some(a) = action {
            let info = sim.step(a, &mut obs);
            out.invalid = info.invalid;
            if info.round_played.is_some() {
                out.rounds_played += 1;
            }
        }
        while out.rounds_played < rounds && !sim.game.over() {
            sim.step(0, &mut obs);
            out.rounds_played += 1;
        }
        out.lives_lost = lives_before - sim.game.lives;
        out.cash = sim.game.cash.as_f64();
        out.game_over = sim.game.over() && !sim.game.won();
        out
    }

    /// Ein gestörter Zustand als fester Zustand (z.B. für einen Eval-Satz): Störungen wie beim
    /// Training, aber mit eigenem Seed; ein verkaufter Turm ist danach schon weg. Ändert die Env nicht.
    pub fn perturbed_state(&self, base: &StartState, seed: u64) -> Result<StartState, String> {
        let mut rng = splitmix64(seed);
        let mut g = self.game.clone();
        for _ in 0..8 {
            let p = start::perturb(base, &g, self.layout.cell, &mut RngDice(&mut rng));
            if p.state.build(&mut g).is_ok() {
                p.finish(&mut g);
                return Ok(StartState::from_game(&g));
            }
        }
        Err("keine gültige Störung gefunden".into())
    }

    /// Einstellungen für Startzustände (VecEnv: für alle Envs gemeinsam).
    pub fn set_starts(&mut self, starts: Option<Arc<StartConfig>>) {
        self.starts = starts;
    }

    /// Herkunft der laufenden Episode.
    pub fn start_kind(&self) -> StartKind {
        self.start.kind
    }

    /// Abgeschlossene 3-Runden-Prüfungen seit dem letzten Aufruf.
    pub fn take_outcomes(&mut self) -> Vec<StartOutcome> {
        std::mem::take(&mut self.outcomes)
    }

    /// Zustände zu Rundenbeginn aus normalen Episoden seit dem letzten Aufruf: je Episode (beim
    /// nächsten Reset abgelegt, auch bei vorzeitigem Reset) ein zufällig gewählter Rundenbeginn,
    /// höchstens die letzten `SNAPSHOTS`.
    pub fn take_snapshots(&mut self) -> Vec<StartState> {
        self.snapshots.drain(..).collect()
    }

    /// Starts aus Zuständen, die seit dem letzten Aufruf nicht geklappt haben.
    pub fn take_fallbacks(&mut self) -> u64 {
        std::mem::take(&mut self.fallbacks)
    }

    fn start_from_config(&mut self, sc: &StartConfig) {
        if self.unit() < sc.p_random {
            if self.random_build(sc) {
                self.start.kind = StartKind::Random;
                self.start.lives_perturbed = self.perturb_lives();
            } else {
                self.game.reset();
                self.fallbacks += 1;
            }
            return;
        }
        let Some(base) = sc.sample(&mut RngDice(&mut self.rng)) else {
            self.fallbacks += 1;
            return;
        };
        // gestörter Zustand; ist er ungültig (z.B. Turm auf den Track verschoben), neu würfeln
        for _ in 0..8 {
            let p = start::perturb(&sc.pool[base], &self.game, self.layout.cell, &mut RngDice(&mut self.rng));
            if p.state.build(&mut self.game).is_ok() {
                p.finish(&mut self.game);
                self.start = EpisodeStart {
                    kind: StartKind::Pool,
                    base: Some(sc.id(base)),
                    lives_perturbed: p.lives_perturbed,
                    rounds: 0,
                    checked: false,
                };
                return;
            }
        }
        self.game.reset(); // nichts Gültiges gefunden: normaler Start
        self.fallbacks += 1;
    }

    /// Zufälliger Build (Quelle C): Runde gleichverteilt über 1..=Endrunde-5, Budget aus der
    /// Budgetkurve ± Streuung, zufällig aufgeteilt in Cash und Türme (bebaubare Plätze,
    /// erlaubte Upgrades, keine gesperrten Typen).
    fn random_build(&mut self, sc: &StartConfig) -> bool {
        let rules = &self.game.data().rules;
        let (first, last) = (rules.start_round.saturating_sub(1), rules.end_round.saturating_sub(5));
        if sc.budget.is_empty() || last <= first {
            return false;
        }
        let round = first + RngDice(&mut self.rng).below((last - first) as usize) as u32;
        let base = sc.budget[(round as usize).min(sc.budget.len() - 1)];
        let budget = (base * (1.0 + sc.budget_spread * (2.0 * self.unit() - 1.0))).max(0.0);
        let share = 0.3 + 0.6 * self.unit();
        let lives = self.game.lives;
        self.game.cash = Money::from_f64(budget * share);
        let l = self.layout;
        for _ in 0..60 {
            let towers = self.game.towers.len();
            if towers == 0 || self.unit() < 0.4 {
                let kind = RngDice(&mut self.rng).below(l.towers);
                let c = RngDice(&mut self.rng).below(l.cells);
                let (x, y) = l.center(c);
                if !self.unavailable(kind) && self.map_ok[kind * l.cells + c] {
                    let _ = self.game.apply(Action::Place { kind: kind as u8, x, y });
                }
            } else {
                let id = self.game.towers[RngDice(&mut self.rng).below(towers)].id;
                let path = RngDice(&mut self.rng).below(l.paths) as u8;
                let _ = self.game.apply(Action::Upgrade { tower: id, path });
            }
        }
        self.game.cash += Money::from_f64(budget * (1.0 - share));
        self.game.lives = lives;
        self.game.round = round;
        true
    }

    /// In einem Drittel der Fälle Leben auf 50–100 %.
    fn perturb_lives(&mut self) -> bool {
        if RngDice(&mut self.rng).below(3) != 0 {
            return false;
        }
        let f = 0.5 + 0.5 * self.unit();
        self.game.lives = ((self.game.lives as f64 * f).ceil() as i32).max(1);
        true
    }

    fn unit(&mut self) -> f64 {
        RngDice(&mut self.rng).unit()
    }

    /// Nach jeder Runde: 3-Runden-Prüfung für Starts aus Zuständen, Snapshot für Quelle A.
    fn after_round(&mut self) {
        let s = &mut self.start;
        if s.kind != StartKind::Normal && !s.checked {
            s.rounds += 1;
            if s.rounds >= CHECK_ROUNDS || self.game.over() {
                s.checked = true;
                if self.outcomes.len() >= MAX_OUTCOMES {
                    self.outcomes.remove(0); // niemand holt ab: Älteste verwerfen
                }
                self.outcomes.push(StartOutcome {
                    kind: s.kind,
                    base: s.base,
                    survived: self.game.lives > 0,
                    lives_perturbed: s.lives_perturbed,
                });
            }
        }
        if s.kind == StartKind::Normal && self.starts.is_some() && !self.game.over() {
            // Reservoir-Stichprobe: jeder Rundenbeginn der Episode mit gleicher Wahrscheinlichkeit
            s.rounds += 1;
            let k = s.rounds as usize;
            if RngDice(&mut self.rng).below(k) == 0 {
                self.candidate = Some(StartState::from_game(&self.game));
            }
        }
    }

    pub fn step(&mut self, action: usize, obs: &mut [f32]) -> StepInfo {
        let mut info = StepInfo::default();
        if self.game.over() {
            // nach Spielende passiert nichts mehr (Python würde hier abstürzen)
            info.terminated = true;
            self.observe(obs);
            return info;
        }
        if action == 0 || self.builds_left == 0 {
            let round = self.game.round + 1;
            let report = self.game.play_round();
            info.reward += self.cfg.reward_life * report.leaked as f64;
            if self.game.lives > 0 {
                info.reward += self.cfg.reward_round;
            }
            self.builds_left = self.cfg.max_builds_per_round;
            info.round_played = Some(report);
            self.after_round();
            if let Some(r) = &mut self.recorder {
                r.round_played(round, &self.game, &report);
            }
        } else {
            let banned = matches!(self.layout.decode(action),
                Some(CellAction::Place { kind, .. }) if self.unavailable(kind as usize));
            match self.layout.decode(action).filter(|_| !banned).and_then(|ca| self.to_action(ca)) {
                Some(a) => {
                    let described = replay::describe(&self.game, a);
                    match self.game.apply(a) {
                        Ok(placed) => {
                            if placed.is_some() {
                                info.reward += self.cfg.reward_tower;
                            }
                            if let Some(r) = &mut self.recorder {
                                r.action(described);
                            }
                        }
                        Err(_) => info.invalid = true,
                    }
                }
                None => info.invalid = true,
            }
            self.builds_left -= 1;
        }
        info.terminated = self.game.over();
        if info.terminated {
            info.reward += if self.game.won() { self.cfg.reward_win } else { self.cfg.reward_loss };
            if self.cfg.record {
                // Episoden aus Startzuständen haben kein Replay: nicht das der vorigen stehen lassen
                self.last_replay = self.recorder.as_ref().map(|r| r.replay().clone());
            }
        }
        self.observe(obs);
        info
    }

    /// Würfelt 0..=max_banned_towers verschiedene gesperrte Turmtypen (ohne Sperren: nichts).
    fn draw_bans(&mut self) {
        let max = self.cfg.max_banned_towers;
        if max == 0 {
            return;
        }
        // nur unter den nicht in den Regeln gesperrten Türmen (feste Sperren zählen nicht mit)
        let mut order: Vec<usize> =
            (0..self.banned.len()).filter(|&t| !self.game.is_locked(t as u8)).collect();
        let n = order.len();
        let k = (self.next_rand() % (max as u64 + 1)) as usize;
        for i in 0..k {
            let j = i + (self.next_rand() % (n - i) as u64) as usize; // Fisher-Yates, nur k Schritte
            order.swap(i, j);
        }
        self.banned.fill(false);
        for &t in &order[..k] {
            self.banned[t] = true;
        }
    }

    fn next_rand(&mut self) -> u64 {
        RngDice(&mut self.rng).next_u64()
    }

    /// Der Turm, der für eine Zelle steht: der erste (in Bau-Reihenfolge), dessen Mittelpunkt in
    /// der Zelle liegt. Maske, Observation und Aktionen nutzen alle diese eine Zuordnung, damit
    /// eine erlaubte Aktion auch ausführbar ist (auch für Türme abseits der Zellmitte aus Replays).
    pub fn tower_in_cell(&self, c: usize) -> Option<&btd_core::Tower> {
        let l = &self.layout;
        self.game.towers.iter().find(|t| l.cell_of(t.pos().0, t.pos().1) == Some(c))
    }

    /// Dieselbe Zuordnung für alle Zellen auf einmal: (Zelle, Turm), je Zelle höchstens einer.
    fn cell_towers(&self) -> Vec<(usize, &btd_core::Tower)> {
        let l = &self.layout;
        let mut seen = vec![false; l.cells];
        let mut out = Vec::with_capacity(self.game.towers.len());
        for t in &self.game.towers {
            if let Some(c) = l.cell_of(t.pos().0, t.pos().1) {
                if !std::mem::replace(&mut seen[c], true) {
                    out.push((c, t));
                }
            }
        }
        out
    }

    /// Rasteraktion -> Spielaktion (Weltposition bzw. Turm in der Zelle).
    pub fn to_action(&self, ca: CellAction) -> Option<Action> {
        let l = &self.layout;
        let tower_in = |c: usize| self.tower_in_cell(c).map(|t| t.id);
        Some(match ca {
            CellAction::Place { kind, cell } => {
                let (x, y) = l.center(cell);
                Action::Place { kind, x, y }
            }
            CellAction::Upgrade { cell, path } => Action::Upgrade { tower: tower_in(cell)?, path },
            CellAction::Sell { cell } => Action::Sell { tower: tower_in(cell)? },
        })
    }

    /// Maske wie `BTDEnv.action_masks` in Python: `true` = erlaubt.
    pub fn action_mask(&self, mask: &mut [bool]) {
        let l = &self.layout;
        mask.fill(false);
        mask[0] = true;
        if self.builds_left == 0 || self.game.over() {
            return;
        }
        let g = &self.game;
        let data = g.data();
        // mit Rabatt-Zonen hängt der Preis von der Zelle ab (Raster), sonst nur vom Turmtyp
        let raster = l.support.then(|| self.raster());
        for (k, t) in data.towers.iter().enumerate() {
            if self.unavailable(k) {
                continue;
            }
            let list = g.price(t.cost);
            let base = l.a_place + k * l.cells;
            for c in 0..l.cells {
                let price = match &raster {
                    Some(r) if r.discount[c] > 0.0 => {
                        Money::from_dollars(data.cost_discounted(t.cost, r.discount[c]))
                    }
                    _ => list,
                };
                if g.cash >= price && self.map_ok[k * l.cells + c] {
                    let (x, y) = l.center(c);
                    mask[base + c] = !g.occupied(k as u8, x, y);
                }
            }
        }
        for (c, t) in self.cell_towers() {
            for p in 0..l.paths {
                mask[l.a_upgrade + p * l.cells + c] =
                    g.can_apply(&Action::Upgrade { tower: t.id, path: p as u8 });
            }
            mask[l.a_sell + c] = data.rules.selling;
        }
    }

    /// Observation wie `BTDEnv._obs` in Python (flach, f32).
    pub fn observe(&self, obs: &mut [f32]) {
        let l = &self.layout;
        let g = &self.game;
        let data = g.data();
        let grid = &data.map.grid;
        let max_tier = l.max_tier as f32;
        obs.fill(0.0);
        for c in 0..l.cells {
            obs[c] = grid.path[c] as u8 as f32;
            obs[l.cells + c] = grid.buildable[c] as u8 as f32;
        }
        for (c, t) in self.cell_towers() {
            obs[(2 + t.kind as usize) * l.cells + c] = 1.0;
            for p in 0..l.paths {
                obs[(2 + l.towers + p) * l.cells + c] = t.tiers[p] as f32 / max_tier;
            }
        }
        if l.support {
            // Buff-Kanäle: was ein Turm in der Zelle bekäme (Auren ohne Turmgruppen-Filter), normiert
            // auf die Größenordnung im Spiel (docs/design-towers-step2.md §4)
            let r = self.raster();
            let b0 = (2 + l.towers + l.paths) * l.cells;
            for c in 0..l.cells {
                let a = r.aura[c];
                obs[b0 + c] = (a.range_frac * 5.0).min(1.0) as f32;
                obs[b0 + l.cells + c] = ((1.0 - a.rate) * 4.0).clamp(0.0, 1.0) as f32;
                obs[b0 + 2 * l.cells + c] = a.camo as u8 as f32;
                obs[b0 + 3 * l.cells + c] = a.damage_all as u8 as f32;
                obs[b0 + 4 * l.cells + c] = (r.discount[c] * 4.0).min(1.0) as f32;
            }
        }
        let s = l.n_channels * l.cells;
        obs[s] = (g.cash.dollars() as f64 / 5000.0).min(1.0) as f32;
        obs[s + 1] = (g.lives as f64 / data.rules.start_lives as f64) as f32;
        obs[s + 2] = (g.round as f64 / g.max_round() as f64) as f32;
        obs[s + 3] = (self.builds_left as f64 / self.cfg.max_builds_per_round as f64) as f32;
        let mut o = s + 4;
        if let Some(p) = &self.preview {
            p.write(g.round as usize, &mut obs[o..]);
            o += RoundPreview::len(self.cfg.preview_rounds);
        }
        if self.cfg.max_banned_towers > 0 || l.support {
            for k in 0..l.towers {
                obs[o + k] = if self.unavailable(k) { 0.0 } else { 1.0 };
            }
            o += l.towers;
        }
        if l.support {
            // Einkommen der nächsten Runde (Farm), log-normiert (wird ab 4xx schnell groß)
            let income: f64 = g
                .towers
                .iter()
                .filter_map(|t| data.towers[t.kind as usize].variant(t.tiers)?.income.map(|i| i.per_round))
                .sum();
            obs[o] = (libm::log(1.0 + income) / libm::log(1.0 + INCOME_NORM)).min(1.0) as f32;
        }
    }

    /// Protokoll der laufenden Episode (nur mit `EnvConfig::record`).
    pub fn replay(&self) -> Option<&Replay> {
        self.recorder.as_ref().map(|r| r.replay())
    }

    /// Protokoll der zuletzt beendeten Episode (überlebt das Auto-Reset der Vektor-Env).
    pub fn last_replay(&self) -> Option<&Replay> {
        self.last_replay.as_ref()
    }

    /// Setzt die Env auf einen Spielstand: Replay bis einschließlich Runde `until_round` abspielen.
    /// Das Protokoll läuft danach weiter, sodass ab dort neu aufgezeichnet wird.
    ///
    /// `strict = false` erlaubt Protokolle, deren Ergebnisse von der Sim abweichen (z.B. ein von
    /// Hand aufgeschriebenes echtes Spiel); unmögliche Aktionen werden dann übersprungen.
    /// Gibt die nachgespielten Runden zurück. Endet das Protokoll mit Spielende, ist die Env
    /// danach `terminated` und `last_replay` gesetzt.
    pub fn load_replay(
        &mut self,
        replay: &Replay,
        until_round: Option<u32>,
        strict: bool,
    ) -> Result<Vec<replay::RoundCheck>, ReplayError> {
        let (game, checks) = replay.replay_to(&self.spec, until_round, strict)?;
        self.game = game;
        if self.cfg.randomness {
            // das nachgespielte Spiel läuft ab hier mit eigenem Zufall weiter
            let seed = self.next_rand();
            self.game.set_seed(Some(seed));
        }
        self.builds_left = self.cfg.max_builds_per_round;
        self.last_replay = None;
        // geladenes Spiel zählt als normale Episode (kein 3-Runden-Ergebnis, kein alter Snapshot)
        self.start = EpisodeStart::default();
        self.candidate = None;
        if self.cfg.record {
            let r = Recorder::resume(&self.spec, &self.cfg, &checks);
            if self.game.over() {
                self.last_replay = Some(r.replay().clone());
            }
            self.recorder = Some(r);
        }
        Ok(checks)
    }
}

/// N Envs mit Auto-Reset (sb3-Semantik), Schritte parallel über rayon.
pub struct VecEnv {
    pub envs: Vec<Env>,
    pub layout: Layout,
}

/// Ergebnis eines Vektor-Schritts; Buffer werden vom Aufrufer bereitgestellt.
pub struct VecStepBuffers<'a> {
    pub obs: &'a mut [f32],
    pub rewards: &'a mut [f64],
    pub dones: &'a mut [bool],
    /// Observation vor dem Auto-Reset (nur gültig, wo `dones[i]`).
    pub terminal_obs: &'a mut [f32],
    pub rounds: &'a mut [u32],
    /// Herkunft der Episode, zu der der Schritt gehörte (`StartKind` als Zahl, vor dem Auto-Reset).
    pub start_kinds: &'a mut [u8],
}

impl VecEnv {
    pub fn new(spec: EnvSpec, cfg: EnvConfig, n: usize) -> Result<Self, DataError> {
        // frische Envs sind identisch: einmal anlegen (inkl. Vorberechnung), dann klonen
        let env = Env::new(spec.clone(), cfg.clone())?;
        let mut envs = vec![env; n];
        for (i, e) in envs.iter_mut().enumerate() {
            e.reseed(cfg.seed ^ splitmix64(i as u64 + 1)); // eigene Sperren je Env
        }
        let layout = Layout::new(&spec.data, cfg.preview_rounds, cfg.max_banned_towers > 0);
        Ok(VecEnv { envs, layout })
    }

    pub fn len(&self) -> usize {
        self.envs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.envs.is_empty()
    }

    pub fn reset(&mut self, obs: &mut [f32]) {
        let n = self.layout.obs_size;
        assert_eq!(obs.len(), self.envs.len() * n, "Observation-Buffer: n_envs * obs_size");
        self.envs.par_iter_mut().zip(obs.par_chunks_mut(n)).for_each(|(e, o)| e.reset(o));
    }

    pub fn step(&mut self, actions: &[usize], b: VecStepBuffers) {
        let (k, n) = (self.envs.len(), self.layout.obs_size);
        assert_eq!(actions.len(), k, "eine Aktion pro Env");
        assert!(
            b.obs.len() == k * n && b.terminal_obs.len() == k * n,
            "Observation-Buffer: n_envs * obs_size"
        );
        assert!(
            b.rewards.len() == k && b.dones.len() == k && b.rounds.len() == k && b.start_kinds.len() == k,
            "Buffer: n_envs"
        );
        self.envs
            .par_iter_mut()
            .zip(actions.par_iter())
            .zip(b.obs.par_chunks_mut(n))
            .zip(b.terminal_obs.par_chunks_mut(n))
            .zip(b.rewards.par_iter_mut())
            .zip(b.dones.par_iter_mut())
            .zip(b.rounds.par_iter_mut())
            .zip(b.start_kinds.par_iter_mut())
            .for_each(|(((((((e, &a), o), to), r), d), rd), sk)| {
                *sk = e.start_kind() as u8;
                let info = e.step(a, o);
                *r = info.reward;
                *d = info.terminated;
                *rd = e.game.round;
                if info.terminated {
                    to.copy_from_slice(o);
                    e.reset(o);
                }
            });
    }

    /// Startzustände für alle Envs (gilt ab dem nächsten Reset; `None` = aus). Ungültige
    /// Einstellungen (z.B. ein Zustand, der sich nicht aufbauen lässt) werden abgelehnt.
    pub fn set_starts(&mut self, starts: Option<StartConfig>) -> Result<(), String> {
        if let (Some(sc), Some(e)) = (&starts, self.envs.first()) {
            sc.validate(&e.game)?;
        }
        let starts = starts.map(Arc::new);
        for e in &mut self.envs {
            e.set_starts(starts.clone());
        }
        Ok(())
    }

    /// Fehlgeschlagene Starts aus Zuständen aller Envs seit dem letzten Aufruf.
    pub fn take_fallbacks(&mut self) -> u64 {
        self.envs.iter_mut().map(|e| e.take_fallbacks()).sum()
    }

    /// 3-Runden-Ergebnisse aller Envs seit dem letzten Aufruf.
    pub fn take_outcomes(&mut self) -> Vec<StartOutcome> {
        self.envs.iter_mut().flat_map(|e| e.take_outcomes()).collect()
    }

    /// Zustände zu Rundenbeginn aus normalen Episoden aller Envs (Quelle A).
    pub fn take_snapshots(&mut self) -> Vec<StartState> {
        self.envs.iter_mut().flat_map(|e| e.take_snapshots()).collect()
    }

    pub fn action_masks(&self, masks: &mut [bool]) {
        let n = self.layout.n_actions;
        assert_eq!(masks.len(), self.envs.len() * n, "Masken-Buffer: n_envs * n_actions");
        self.envs.par_iter().zip(masks.par_chunks_mut(n)).for_each(|(e, m)| e.action_mask(m));
    }
}

/// SplitMix64: kleiner, schneller Zufallsgenerator, reicht für die Sperren.
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Höchstzahl gesammelter Snapshots je Env und unabgeholter Ergebnisse.
const SNAPSHOTS: usize = 16;
const MAX_OUTCOMES: usize = 10_000;

/// Ergebnis von `Env::lookahead`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lookahead {
    pub lives_lost: i32,
    pub rounds_played: u32,
    /// Cash danach (Dollar).
    pub cash: f64,
    /// Spiel in der Vorausschau verloren.
    pub game_over: bool,
    /// Die Aktion war ungültig (nichts ausgeführt).
    pub invalid: bool,
}

/// Herkunft und 3-Runden-Prüfung der laufenden Episode (`rounds`: gespielte Runden seit dem Start).
#[derive(Clone, Copy, Debug, Default)]
struct EpisodeStart {
    kind: StartKind,
    base: Option<u64>,
    lives_perturbed: bool,
    rounds: u32,
    checked: bool,
}

/// Der Env-Zufall als `Dice` (SplitMix64-Kette, wie bei den Sperren).
struct RngDice<'a>(&'a mut u64);

impl Dice for RngDice<'_> {
    fn next_u64(&mut self) -> u64 {
        *self.0 = splitmix64(*self.0);
        *self.0
    }
}
