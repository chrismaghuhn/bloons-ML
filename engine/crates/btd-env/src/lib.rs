//! RL-Schicht über `btd-core`: Aktionsraum, Masken, Observation, Rewards, Vektor-Env und
//! Spielprotokolle (Replays).
//!
//! M1: Aktionsraum, Observation und Rewards sind identisch zu `btd_sim/env.py`, damit die
//! Python- und die Rust-Env austauschbar sind (Paritätstest in `tests/test_rust_env.py`).

pub mod replay;

use std::path::Path;
use std::sync::Arc;

use btd_core::{Action, Game, RoundReport};
use btd_data::{DataError, GameData};
use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_64_with_seed;

pub use replay::{Recorder, Replay, ReplayError};

/// Seed für den Hash der Datendatei im Replay-Header.
const DATA_HASH_SEED: u64 = 0xDA7A;

/// Geladene Spieldaten plus Hash der Quelldatei (damit Replays nicht stillschweigend mit
/// anderen Daten abgespielt werden).
#[derive(Clone, Debug)]
pub struct EnvSpec {
    pub data: Arc<GameData>,
    pub data_hash: u64,
    pub data_name: String,
}

impl EnvSpec {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DataError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|source| DataError::Io { path: path.display().to_string(), source })?;
        Ok(EnvSpec {
            data: Arc::new(GameData::from_json(&text)?),
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
    /// Spielprotokoll mitschreiben (`Env::replay`, `Env::last_replay`).
    pub record: bool,
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
            record: false,
        }
    }
}

/// Aktionsraum (Discrete), N = Anzahl Zellen, T = Anzahl Turmtypen:
/// `0` Runde starten, `1 + k*N + c` Turm k auf Zelle c bauen,
/// `1 + T*N + p*N + c` Upgrade-Pfad p auf Zelle c, `1 + T*N + 2*N + c` Verkaufen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub grid_w: usize,
    pub grid_h: usize,
    pub cells: usize,
    pub towers: usize,
    pub a_place: usize,
    pub a_upgrade: usize,
    pub a_sell: usize,
    pub n_actions: usize,
    pub n_channels: usize,
    pub n_scalars: usize,
    pub obs_size: usize,
}

impl Layout {
    pub fn new(data: &GameData) -> Self {
        let (grid_w, grid_h) = (data.map.grid_w, data.map.grid_h);
        let cells = grid_w * grid_h;
        let towers = data.towers.len();
        let a_place = 1;
        let a_upgrade = a_place + towers * cells;
        let a_sell = a_upgrade + 2 * cells;
        let n_channels = 2 + towers + 2; // Pfad, baubar, Turmtyp one-hot, Tier Pfad 1/2
        let n_scalars = 4;
        Layout {
            grid_w,
            grid_h,
            cells,
            towers,
            a_place,
            a_upgrade,
            a_sell,
            n_actions: a_sell + cells,
            n_channels,
            n_scalars,
            obs_size: n_channels * cells + n_scalars,
        }
    }

    /// Aktionsindex -> Spielaktion (`None` für "Runde starten").
    pub fn decode(&self, a: usize) -> Option<Action> {
        let xy = |c: usize| ((c % self.grid_w) as u16, (c / self.grid_w) as u16);
        if a == 0 || a >= self.n_actions {
            return None;
        }
        Some(if a < self.a_upgrade {
            let (k, c) = ((a - self.a_place) / self.cells, (a - self.a_place) % self.cells);
            let (x, y) = xy(c);
            Action::Place { kind: k as u8, x, y }
        } else if a < self.a_sell {
            let (p, c) = ((a - self.a_upgrade) / self.cells, (a - self.a_upgrade) % self.cells);
            let (x, y) = xy(c);
            Action::Upgrade { x, y, path: p as u8 }
        } else {
            let (x, y) = xy(a - self.a_sell);
            Action::Sell { x, y }
        })
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
    recorder: Option<Recorder>,
    last_replay: Option<Replay>,
}

impl Env {
    pub fn new(spec: EnvSpec, cfg: EnvConfig) -> Result<Self, DataError> {
        let game = Game::new(Arc::clone(&spec.data))?;
        let layout = Layout::new(&spec.data);
        let recorder = cfg.record.then(|| Recorder::new(&spec, &cfg));
        Ok(Env {
            builds_left: cfg.max_builds_per_round,
            game,
            layout,
            cfg,
            spec,
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
            let report = self.game.play_round();
            info.reward += self.cfg.reward_life * report.leaked as f64;
            if self.game.lives > 0 {
                info.reward += self.cfg.reward_round;
            }
            self.builds_left = self.cfg.max_builds_per_round;
            info.round_played = Some(report);
            if let Some(r) = &mut self.recorder {
                r.round_played(&self.game, &report);
            }
        } else {
            match self.layout.decode(action).map(|a| (a, self.game.apply(a))) {
                Some((a, Ok(()))) => {
                    if let Some(r) = &mut self.recorder {
                        r.action(&self.spec.data, a);
                    }
                }
                _ => info.invalid = true,
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
            if g.cash < btd_core::Money::from_dollars(t.cost) {
                continue;
            }
            let base = l.a_place + k * l.cells;
            for c in 0..l.cells {
                mask[base + c] =
                    data.map.buildable[c] && !g.is_occupied((c % l.grid_w) as u16, (c / l.grid_w) as u16);
            }
        }
        for t in &g.towers {
            let c = t.y as usize * l.grid_w + t.x as usize;
            for p in 0..2 {
                mask[l.a_upgrade + p * l.cells + c] =
                    g.can_apply(&Action::Upgrade { x: t.x, y: t.y, path: p as u8 });
            }
            mask[l.a_sell + c] = true;
        }
    }

    /// Observation wie `BTDEnv._obs` in Python (flach, f32).
    pub fn observe(&self, obs: &mut [f32]) {
        let l = &self.layout;
        let g = &self.game;
        let data = g.data();
        let max_tier = data.rules.max_tier as f32;
        obs.fill(0.0);
        for c in 0..l.cells {
            obs[c] = data.map.path_mask[c] as u8 as f32;
            obs[l.cells + c] = data.map.buildable[c] as u8 as f32;
        }
        for t in &g.towers {
            let c = t.y as usize * l.grid_w + t.x as usize;
            obs[(2 + t.kind as usize) * l.cells + c] = 1.0;
            obs[(2 + l.towers) * l.cells + c] = t.tiers[0] as f32 / max_tier;
            obs[(3 + l.towers) * l.cells + c] = t.tiers[1] as f32 / max_tier;
        }
        let s = l.n_channels * l.cells;
        obs[s] = (g.cash.dollars() as f64 / 5000.0).min(1.0) as f32;
        obs[s + 1] = (g.lives as f64 / data.rules.start_lives as f64) as f32;
        obs[s + 2] = (g.round as f64 / g.max_round() as f64) as f32;
        obs[s + 3] = (self.builds_left as f64 / self.cfg.max_builds_per_round as f64) as f32;
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
    pub fn load_replay(&mut self, replay: &Replay, until_round: Option<u32>) -> Result<(), ReplayError> {
        let game = replay.play(&self.spec, until_round, |_| {})?;
        self.game = game;
        self.builds_left = self.cfg.max_builds_per_round;
        if self.recorder.is_some() {
            self.recorder = Some(Recorder::resume(&self.spec, &self.cfg, replay, self.game.round));
        }
        Ok(())
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
        let envs = (0..n).map(|_| Env::new(spec.clone(), cfg.clone())).collect::<Result<Vec<_>, _>>()?;
        let layout = Layout::new(&spec.data);
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
        self.envs.par_iter_mut().zip(obs.par_chunks_mut(n)).for_each(|(e, o)| e.reset(o));
    }

    pub fn step(&mut self, actions: &[usize], b: VecStepBuffers) {
        let n = self.layout.obs_size;
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
        self.envs.par_iter().zip(masks.par_chunks_mut(n)).for_each(|(e, m)| e.action_mask(m));
    }
}
