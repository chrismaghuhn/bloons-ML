//! RL-Schicht über `btd-core`: Aktionsraum, Masken, Observation, Rewards, Vektor-Env und
//! Spielprotokolle (Replays).
//!
//! Mit dem sim_v0-Datensatz sind Aktionsraum, Observation und Rewards identisch zu
//! `btd_sim/env.py`, damit Python- und Rust-Env austauschbar sind (Paritätstest in
//! `tests/test_rust_env.py`). Die Runden-Vorschau (`EnvConfig::preview_rounds`) ist nur für die
//! Spieldaten gedacht und erweitert die Observation; ohne sie bleibt die Parität bestehen.

pub mod preview;
pub mod replay;

use std::path::Path;
use std::sync::Arc;

use btd_core::{Action, Game, RoundReport};
use btd_data::v1::GameV1;
use btd_data::DataError;
use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_64_with_seed;

pub use preview::RoundPreview;
pub use replay::{Recorder, Replay, ReplayError};

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
    pub fn new(data: &GameV1, preview_rounds: usize) -> Self {
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
        let n_channels = 2 + towers + paths; // Pfad, baubar, Turmtyp one-hot, Tier je Pfad
        let preview = if preview_rounds > 0 { RoundPreview::len(preview_rounds) } else { 0 };
        let n_scalars = 4 + preview;
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
    recorder: Option<Recorder>,
    last_replay: Option<Replay>,
}

impl Env {
    pub fn new(spec: EnvSpec, cfg: EnvConfig) -> Result<Self, DataError> {
        if cfg.max_builds_per_round == 0 {
            return Err(DataError::Invalid("max_builds_per_round muss > 0 sein".into()));
        }
        let game = Game::new(Arc::clone(&spec.data))?;
        let layout = Layout::new(&spec.data, cfg.preview_rounds);
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
            builds_left: cfg.max_builds_per_round,
            game,
            layout,
            cfg,
            spec,
            preview,
            recorder,
            last_replay: None,
        })
    }

    pub fn spec(&self) -> &EnvSpec {
        &self.spec
    }

    pub fn reset(&mut self, obs: &mut [f32]) {
        self.game.reset();
        self.builds_left = self.cfg.max_builds_per_round;
        if self.recorder.is_some() {
            self.recorder = Some(Recorder::new(&self.spec, &self.cfg));
        }
        self.observe(obs);
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
            if let Some(r) = &mut self.recorder {
                r.round_played(round, &self.game, &report);
            }
        } else {
            match self.layout.decode(action).and_then(|ca| self.to_action(ca)) {
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
            if let Some(r) = &self.recorder {
                self.last_replay = Some(r.replay().clone());
            }
        }
        self.observe(obs);
        info
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
        for (k, t) in data.towers.iter().enumerate() {
            if g.cash < g.price(t.cost) {
                continue;
            }
            let base = l.a_place + k * l.cells;
            for c in 0..l.cells {
                if self.map_ok[k * l.cells + c] {
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
        let s = l.n_channels * l.cells;
        obs[s] = (g.cash.dollars() as f64 / 5000.0).min(1.0) as f32;
        obs[s + 1] = (g.lives as f64 / data.rules.start_lives as f64) as f32;
        obs[s + 2] = (g.round as f64 / g.max_round() as f64) as f32;
        obs[s + 3] = (self.builds_left as f64 / self.cfg.max_builds_per_round as f64) as f32;
        if let Some(p) = &self.preview {
            p.write(g.round as usize, &mut obs[s + 4..]);
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
        self.builds_left = self.cfg.max_builds_per_round;
        self.last_replay = None;
        if self.recorder.is_some() {
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
}

impl VecEnv {
    pub fn new(spec: EnvSpec, cfg: EnvConfig, n: usize) -> Result<Self, DataError> {
        // frische Envs sind identisch: einmal anlegen (inkl. Vorberechnung), dann klonen
        let env = Env::new(spec.clone(), cfg.clone())?;
        let envs = vec![env; n];
        let layout = Layout::new(&spec.data, cfg.preview_rounds);
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
        assert!(b.rewards.len() == k && b.dones.len() == k && b.rounds.len() == k, "Buffer: n_envs");
        self.envs
            .par_iter_mut()
            .zip(actions.par_iter())
            .zip(b.obs.par_chunks_mut(n))
            .zip(b.terminal_obs.par_chunks_mut(n))
            .zip(b.rewards.par_iter_mut())
            .zip(b.dones.par_iter_mut())
            .zip(b.rounds.par_iter_mut())
            .for_each(|((((((e, &a), o), to), r), d), rd)| {
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

    pub fn action_masks(&self, masks: &mut [bool]) {
        let n = self.layout.n_actions;
        assert_eq!(masks.len(), self.envs.len() * n, "Masken-Buffer: n_envs * n_actions");
        self.envs.par_iter().zip(masks.par_chunks_mut(n)).for_each(|(e, m)| e.action_mask(m));
    }
}
