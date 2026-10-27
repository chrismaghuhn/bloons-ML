//! Python-Bindings (`import btd_rs`) für `btd-env`.
//!
//! Gebaut mit maturin: `maturin develop --release -m engine/crates/btd-py/Cargo.toml`
//! (mit `--features f64` für den bit-genauen Paritätsmodus). Die gymnasium-/sb3-Wrapper
//! liegen in Python unter `btd_sim/rust_env.py`.

use btd_env::replay::{ReplayError, RoundCheck, FLOAT, FORMAT};
use btd_env::{
    Env, EnvConfig, EnvSpec, Replay, StartConfig, StartKind, StartOutcome, StartState, VecEnv, VecStepBuffers,
};
use numpy::{PyArray1, PyArray2, PyArrayMethods, PyReadonlyArray1};
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn value_err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn data_err(e: btd_data::DataError) -> PyErr {
    match e {
        btd_data::DataError::Io { .. } => PyIOError::new_err(e.to_string()),
        _ => value_err(e),
    }
}

fn replay_err(e: ReplayError) -> PyErr {
    match e {
        ReplayError::Io(..) => PyIOError::new_err(e.to_string()),
        _ => value_err(e),
    }
}

fn load_spec(path: &str) -> PyResult<EnvSpec> {
    EnvSpec::load(path).map_err(data_err)
}

#[allow(clippy::too_many_arguments)]
fn config(
    max_builds_per_round: u32,
    reward_round: f64,
    reward_life: f64,
    reward_win: f64,
    reward_loss: f64,
    reward_tower: f64,
    record: bool,
    preview_rounds: usize,
) -> PyResult<EnvConfig> {
    if max_builds_per_round == 0 {
        return Err(value_err("max_builds_per_round muss > 0 sein"));
    }
    Ok(EnvConfig {
        max_builds_per_round,
        reward_round,
        reward_life,
        reward_win,
        reward_loss,
        reward_tower,
        record,
        preview_rounds,
        ..EnvConfig::default()
    })
}

fn parse_state(s: &str) -> PyResult<StartState> {
    serde_json::from_str(s).map_err(|e| value_err(format!("Startzustand: {e}")))
}

fn outcome_tuple(o: StartOutcome) -> (u8, i64, bool, bool) {
    let kind = match o.kind {
        StartKind::Normal => 0,
        StartKind::Pool => 1,
        StartKind::Random => 2,
    };
    (kind, o.base.map_or(-1, |b| b as i64), o.survived, o.lives_perturbed)
}

fn array2<'py, T: numpy::Element + Copy>(
    py: Python<'py>,
    data: &[T],
    rows: usize,
    cols: usize,
) -> PyResult<Bound<'py, PyArray2<T>>> {
    PyArray1::from_slice(py, data).reshape([rows, cols])
}

/// (obs, reward, terminated, info) wie gymnasium ohne `truncated`.
type StepResult<'py> = (Bound<'py, PyArray1<f32>>, f64, bool, Bound<'py, PyDict>);

/// Eine Env. API wie `btd_sim.env.BTDEnv` (ohne gymnasium-Basisklasse).
#[pyclass(name = "Env", module = "btd_rs")]
struct PyEnv {
    env: Env,
    obs: Vec<f32>,
    mask: Vec<bool>,
}

#[pymethods]
impl PyEnv {
    #[new]
    #[pyo3(signature = (data_path, max_builds_per_round=6, reward_round=1.0, reward_life=-0.02,
                        reward_win=10.0, reward_loss=-5.0, record=false, preview_rounds=0, reward_tower=0.0,
                        max_banned_towers=0, seed=0, randomness=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        data_path: &str,
        max_builds_per_round: u32,
        reward_round: f64,
        reward_life: f64,
        reward_win: f64,
        reward_loss: f64,
        record: bool,
        preview_rounds: usize,
        reward_tower: f64,
        max_banned_towers: usize,
        seed: u64,
        randomness: bool,
    ) -> PyResult<Self> {
        let mut cfg = config(
            max_builds_per_round,
            reward_round,
            reward_life,
            reward_win,
            reward_loss,
            reward_tower,
            record,
            preview_rounds,
        )?;
        (cfg.max_banned_towers, cfg.seed, cfg.randomness) = (max_banned_towers, seed, randomness);
        let env = Env::new(load_spec(data_path)?, cfg).map_err(data_err)?;
        let (obs, mask) = (vec![0.0; env.layout.obs_size], vec![false; env.layout.n_actions]);
        Ok(PyEnv { env, obs, mask })
    }

    #[getter]
    fn n_actions(&self) -> usize {
        self.env.layout.n_actions
    }

    #[getter]
    fn obs_size(&self) -> usize {
        self.env.layout.obs_size
    }

    #[getter]
    fn round(&self) -> u32 {
        self.env.game.round
    }

    #[getter]
    fn lives(&self) -> i32 {
        self.env.game.lives
    }

    #[getter]
    fn cash(&self) -> i64 {
        self.env.game.cash.dollars()
    }

    #[getter]
    fn builds_left(&self) -> u32 {
        self.env.builds_left
    }

    #[getter]
    fn towers(&self) -> usize {
        self.env.game.towers.len()
    }

    /// Neue Episode ab einem Startzustand (JSON wie in `VecEnv.set_starts`), ohne Störung.
    fn reset_from<'py>(&mut self, py: Python<'py>, state: &str) -> PyResult<Bound<'py, PyArray1<f32>>> {
        let state = parse_state(state)?;
        self.env.reset_from(&mut self.obs, &state).map_err(value_err)?;
        Ok(PyArray1::from_slice(py, &self.obs))
    }

    /// Gestörter Zustand (Störungen wie im Training, eigener Seed) als JSON, z.B. für Eval-Sätze.
    fn perturb_state(&self, state: &str, seed: u64) -> PyResult<String> {
        let s = self.env.perturbed_state(&parse_state(state)?, seed).map_err(value_err)?;
        Ok(serde_json::to_string(&s).expect("serialisierbar"))
    }

    /// Aktueller Spielstand als Startzustand (JSON).
    fn state(&self) -> String {
        serde_json::to_string(&StartState::from_game(&self.env.game)).expect("serialisierbar")
    }

    /// Gesperrte Turmtypen der laufenden Episode (Reihenfolge wie in den Daten).
    #[getter]
    fn banned(&self) -> Vec<bool> {
        self.env.banned().to_vec()
    }

    /// Neue Episode; `banned` (je Turmtyp) setzt die Sperren fest statt sie zu würfeln.
    #[pyo3(signature = (banned=None))]
    fn reset<'py>(
        &mut self,
        py: Python<'py>,
        banned: Option<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyArray1<f32>>> {
        match banned {
            Some(b) => {
                if b.len() != self.env.layout.towers {
                    return Err(value_err(format!("banned braucht {} Einträge", self.env.layout.towers)));
                }
                self.env.reset_with_bans(&mut self.obs, &b);
            }
            None => self.env.reset(&mut self.obs),
        }
        Ok(PyArray1::from_slice(py, &self.obs))
    }

    /// -> (obs, reward, terminated, info)
    fn step<'py>(&mut self, py: Python<'py>, action: usize) -> PyResult<StepResult<'py>> {
        if action >= self.env.layout.n_actions {
            return Err(value_err(format!("Aktion {action} außerhalb von 0..{}", self.env.layout.n_actions)));
        }
        let info = self.env.step(action, &mut self.obs);
        let d = PyDict::new(py);
        d.set_item("round", self.env.game.round)?;
        d.set_item("lives", self.env.game.lives)?;
        d.set_item("cash", self.env.game.cash.dollars())?;
        d.set_item("towers", self.env.game.towers.len())?;
        if let Some(r) = info.round_played {
            d.set_item("leaked", r.leaked)?;
            d.set_item("pops", r.pops)?;
            d.set_item("seconds", r.seconds)?;
        } else {
            d.set_item("invalid", info.invalid)?;
        }
        Ok((PyArray1::from_slice(py, &self.obs), info.reward, info.terminated, d))
    }

    fn action_masks<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyArray1<bool>> {
        self.env.action_mask(&mut self.mask);
        PyArray1::from_slice(py, &self.mask)
    }

    fn state_hash(&self) -> u64 {
        self.env.game.state_hash()
    }

    /// Protokoll als JSONL-Text: laufende Episode oder (`last=True`) die zuletzt beendete.
    #[pyo3(signature = (last=false))]
    fn replay_jsonl(&self, last: bool) -> Option<String> {
        let r = if last { self.env.last_replay() } else { self.env.replay() };
        r.map(Replay::to_jsonl)
    }

    #[pyo3(signature = (path, last=false))]
    fn save_replay(&self, path: &str, last: bool) -> PyResult<()> {
        let r = if last { self.env.last_replay() } else { self.env.replay() };
        let r = r.ok_or_else(|| value_err("kein Protokoll vorhanden (record=True? Episode beendet?)"))?;
        r.save(path).map_err(replay_err)
    }

    /// Spielstand aus einem Protokoll laden (bis einschließlich `until_round`), danach weiterspielen.
    /// `strict=False` für Protokolle, die von der Sim abweichen dürfen (echte Spiele).
    #[pyo3(signature = (path, until_round=None, strict=true))]
    fn load_replay<'py>(
        &mut self,
        py: Python<'py>,
        path: &str,
        until_round: Option<u32>,
        strict: bool,
    ) -> PyResult<Bound<'py, PyArray1<f32>>> {
        let replay = Replay::load(path).map_err(replay_err)?;
        self.env.load_replay(&replay, until_round, strict).map_err(replay_err)?;
        self.env.observe(&mut self.obs);
        Ok(PyArray1::from_slice(py, &self.obs))
    }
}

/// N Envs mit Auto-Reset, Schritte parallel (GIL wird währenddessen freigegeben).
#[pyclass(name = "VecEnv", module = "btd_rs")]
struct PyVecEnv {
    v: VecEnv,
    obs: Vec<f32>,
    tobs: Vec<f32>,
    rew: Vec<f64>,
    done: Vec<bool>,
    rounds: Vec<u32>,
    kinds: Vec<u8>,
    masks: Vec<bool>,
    actions: Vec<usize>,
}

#[pymethods]
impl PyVecEnv {
    #[new]
    #[pyo3(signature = (data_path, n_envs, max_builds_per_round=6, reward_round=1.0, reward_life=-0.02,
                        reward_win=10.0, reward_loss=-5.0, preview_rounds=0, reward_tower=0.0,
                        max_banned_towers=0, seed=0, randomness=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        data_path: &str,
        n_envs: usize,
        max_builds_per_round: u32,
        reward_round: f64,
        reward_life: f64,
        reward_win: f64,
        reward_loss: f64,
        preview_rounds: usize,
        reward_tower: f64,
        max_banned_towers: usize,
        seed: u64,
        randomness: bool,
    ) -> PyResult<Self> {
        if n_envs == 0 {
            return Err(value_err("n_envs muss > 0 sein"));
        }
        let mut cfg = config(
            max_builds_per_round,
            reward_round,
            reward_life,
            reward_win,
            reward_loss,
            reward_tower,
            false,
            preview_rounds,
        )?;
        (cfg.max_banned_towers, cfg.seed, cfg.randomness) = (max_banned_towers, seed, randomness);
        let v = VecEnv::new(load_spec(data_path)?, cfg, n_envs).map_err(data_err)?;
        let (no, na) = (v.layout.obs_size, v.layout.n_actions);
        Ok(PyVecEnv {
            obs: vec![0.0; n_envs * no],
            tobs: vec![0.0; n_envs * no],
            rew: vec![0.0; n_envs],
            done: vec![false; n_envs],
            rounds: vec![0; n_envs],
            kinds: vec![0; n_envs],
            masks: vec![false; n_envs * na],
            actions: vec![0; n_envs],
            v,
        })
    }

    #[getter]
    fn n_envs(&self) -> usize {
        self.v.len()
    }

    #[getter]
    fn n_actions(&self) -> usize {
        self.v.layout.n_actions
    }

    #[getter]
    fn obs_size(&self) -> usize {
        self.v.layout.obs_size
    }

    fn reset<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let Self { v, obs, .. } = self;
        py.detach(|| v.reset(obs));
        array2(py, &self.obs, self.v.len(), self.v.layout.obs_size)
    }

    /// -> (obs, rewards, dones, terminal_obs, rounds); terminal_obs ist nur dort gültig, wo dones.
    #[allow(clippy::type_complexity)]
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        actions: PyReadonlyArray1<'py, i64>,
    ) -> PyResult<(
        Bound<'py, PyArray2<f32>>,
        Bound<'py, PyArray1<f64>>,
        Bound<'py, PyArray1<bool>>,
        Bound<'py, PyArray2<f32>>,
        Bound<'py, PyArray1<u32>>,
    )> {
        let a = actions.as_slice()?;
        let (n, na) = (self.v.len(), self.v.layout.n_actions);
        if a.len() != n {
            return Err(value_err(format!("{} Aktionen für {n} Envs", a.len())));
        }
        for (dst, &src) in self.actions.iter_mut().zip(a) {
            *dst = usize::try_from(src)
                .ok()
                .filter(|&x| x < na)
                .ok_or_else(|| value_err(format!("Aktion {src}")))?;
        }
        let Self { v, obs, tobs, rew, done, rounds, kinds, actions, .. } = self;
        py.detach(|| {
            let b = VecStepBuffers {
                obs,
                rewards: rew,
                dones: done,
                terminal_obs: tobs,
                rounds,
                start_kinds: kinds,
            };
            v.step(actions, b)
        });
        let no = self.v.layout.obs_size;
        Ok((
            array2(py, &self.obs, n, no)?,
            PyArray1::from_slice(py, &self.rew),
            PyArray1::from_slice(py, &self.done),
            array2(py, &self.tobs, n, no)?,
            PyArray1::from_slice(py, &self.rounds),
        ))
    }

    /// Herkunft der Episoden im letzten Schritt (0 normal, 1 Pool, 2 zufälliger Build), vor dem Auto-Reset.
    #[getter]
    fn last_start_kinds<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<u8>> {
        PyArray1::from_slice(py, &self.kinds)
    }

    /// Startzustände für alle Envs setzen (gilt ab dem nächsten Reset). `pool`: Zustände als JSON
    /// ({round, cash, lives, towers: [{tower, x, y, tiers}]}), `weights` je Zustand, `budget` je
    /// Anzahl geschaffter Runden (Cash + Turmwert) für zufällige Builds.
    #[pyo3(signature = (pool, weights, p_start, p_random, budget, budget_spread=0.3, ids=None))]
    #[allow(clippy::too_many_arguments)]
    fn set_starts(
        &mut self,
        pool: Vec<String>,
        weights: Vec<f64>,
        p_start: f64,
        p_random: f64,
        budget: Vec<f64>,
        budget_spread: f64,
        ids: Option<Vec<u64>>,
    ) -> PyResult<()> {
        let ids = ids.unwrap_or_default();
        let pool = pool.iter().map(|s| parse_state(s)).collect::<PyResult<Vec<_>>>()?;
        let p = |x: f64| x.clamp(0.0, 1.0);
        self.v
            .set_starts(Some(StartConfig {
                pool,
                weights,
                ids,
                p_start: p(p_start),
                p_random: p(p_random),
                budget,
                budget_spread: budget_spread.max(0.0),
            }))
            .map_err(value_err)
    }

    /// Fehlgeschlagene Starts aus Zuständen seit dem letzten Aufruf (normale Episode statt dessen).
    fn take_fallbacks(&mut self) -> u64 {
        self.v.take_fallbacks()
    }

    fn clear_starts(&mut self) {
        self.v.set_starts(None).expect("ohne Einstellungen nichts zu prüfen");
    }

    /// 3-Runden-Ergebnisse seit dem letzten Aufruf: [(Herkunft, Pool-ID oder -1, überlebt, Leben gestört)].
    fn take_outcomes(&mut self) -> Vec<(u8, i64, bool, bool)> {
        self.v.take_outcomes().into_iter().map(outcome_tuple).collect()
    }

    /// Zustände zu Rundenbeginn aus normalen Episoden (Quelle A) als JSON.
    fn take_snapshots(&mut self) -> Vec<String> {
        self.v.take_snapshots().iter().map(|s| serde_json::to_string(s).expect("serialisierbar")).collect()
    }

    fn action_masks<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<bool>>> {
        let Self { v, masks, .. } = self;
        py.detach(|| v.action_masks(masks));
        array2(py, &self.masks, self.v.len(), self.v.layout.n_actions)
    }
}

/// Protokoll gegen die Sim vergleichen (Abweichungen sammeln statt abbrechen).
/// -> Liste von Dicts {round, matches, expected, got}
#[pyfunction]
fn replay_compare<'py>(
    py: Python<'py>,
    data_path: &str,
    replay_path: &str,
) -> PyResult<Vec<Bound<'py, PyDict>>> {
    let spec = load_spec(data_path)?;
    let replay = Replay::load(replay_path).map_err(replay_err)?;
    let checks = replay.compare(&spec).map_err(replay_err)?;
    checks.iter().map(|c| check_dict(py, c)).collect()
}

fn check_dict<'py>(py: Python<'py>, c: &RoundCheck) -> PyResult<Bound<'py, PyDict>> {
    let result = |r: &btd_env::replay::RoundResult| -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        d.set_item("lives", r.lives)?;
        d.set_item("cash", r.cash)?;
        d.set_item("round", r.round)?;
        d.set_item("leaked", r.leaked)?;
        d.set_item("pops", r.pops)?;
        d.set_item("hash", r.hash.clone())?;
        Ok(d)
    };
    let d = PyDict::new(py);
    d.set_item("round", c.round)?;
    d.set_item("matches", c.matches)?;
    d.set_item("action_errors", c.action_errors.clone())?;
    d.set_item("expected", result(&c.expected)?)?;
    d.set_item("got", result(&c.got)?)?;
    Ok(d)
}

#[pymodule]
fn btd_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEnv>()?;
    m.add_class::<PyVecEnv>()?;
    m.add_function(wrap_pyfunction!(replay_compare, m)?)?;
    m.add("FLOAT", FLOAT)?;
    m.add("REPLAY_FORMAT", FORMAT)?;
    Ok(())
}
