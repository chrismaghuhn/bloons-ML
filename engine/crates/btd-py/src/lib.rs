//! Python-Bindings (`import btd_rs`) für `btd-env`.
//!
//! Gebaut mit maturin: `maturin develop --release -m engine/crates/btd-py/Cargo.toml`
//! (mit `--features f64` für den bit-genauen Paritätsmodus). Die gymnasium-/sb3-Wrapper
//! liegen in Python unter `btd_sim/rust_env.py`.

use btd_env::replay::{RoundCheck, FLOAT, FORMAT};
use btd_env::{Env, EnvConfig, EnvSpec, Replay, VecEnv, VecStepBuffers};
use numpy::{PyArray1, PyArray2, PyArrayMethods, PyReadonlyArray1};
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn value_err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn load_spec(path: &str) -> PyResult<EnvSpec> {
    EnvSpec::load(path).map_err(|e| PyIOError::new_err(e.to_string()))
}

fn config(
    max_builds_per_round: u32,
    reward_round: f64,
    reward_life: f64,
    reward_win: f64,
    reward_loss: f64,
    record: bool,
) -> PyResult<EnvConfig> {
    if max_builds_per_round == 0 {
        return Err(value_err("max_builds_per_round muss > 0 sein"));
    }
    Ok(EnvConfig { max_builds_per_round, reward_round, reward_life, reward_win, reward_loss, record })
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
                        reward_win=10.0, reward_loss=-5.0, record=false))]
    fn new(
        data_path: &str,
        max_builds_per_round: u32,
        reward_round: f64,
        reward_life: f64,
        reward_win: f64,
        reward_loss: f64,
        record: bool,
    ) -> PyResult<Self> {
        let cfg = config(max_builds_per_round, reward_round, reward_life, reward_win, reward_loss, record)?;
        let env = Env::new(load_spec(data_path)?, cfg).map_err(value_err)?;
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

    fn reset<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
        self.env.reset(&mut self.obs);
        PyArray1::from_slice(py, &self.obs)
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
        r.save(path).map_err(|e| PyIOError::new_err(e.to_string()))
    }

    /// Spielstand aus einem Protokoll laden (bis einschließlich `until_round`), danach weiterspielen.
    #[pyo3(signature = (path, until_round=None))]
    fn load_replay<'py>(
        &mut self,
        py: Python<'py>,
        path: &str,
        until_round: Option<u32>,
    ) -> PyResult<Bound<'py, PyArray1<f32>>> {
        let replay = Replay::load(path).map_err(value_err)?;
        self.env.load_replay(&replay, until_round).map_err(value_err)?;
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
    masks: Vec<bool>,
    actions: Vec<usize>,
}

#[pymethods]
impl PyVecEnv {
    #[new]
    #[pyo3(signature = (data_path, n_envs, max_builds_per_round=6, reward_round=1.0, reward_life=-0.02,
                        reward_win=10.0, reward_loss=-5.0))]
    fn new(
        data_path: &str,
        n_envs: usize,
        max_builds_per_round: u32,
        reward_round: f64,
        reward_life: f64,
        reward_win: f64,
        reward_loss: f64,
    ) -> PyResult<Self> {
        if n_envs == 0 {
            return Err(value_err("n_envs muss > 0 sein"));
        }
        let cfg = config(max_builds_per_round, reward_round, reward_life, reward_win, reward_loss, false)?;
        let v = VecEnv::new(load_spec(data_path)?, cfg, n_envs).map_err(value_err)?;
        let (no, na) = (v.layout.obs_size, v.layout.n_actions);
        Ok(PyVecEnv {
            obs: vec![0.0; n_envs * no],
            tobs: vec![0.0; n_envs * no],
            rew: vec![0.0; n_envs],
            done: vec![false; n_envs],
            rounds: vec![0; n_envs],
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
        let Self { v, obs, tobs, rew, done, rounds, actions, .. } = self;
        py.detach(|| {
            v.step(actions, VecStepBuffers { obs, rewards: rew, dones: done, terminal_obs: tobs, rounds })
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
    let replay = Replay::load(replay_path).map_err(value_err)?;
    let checks = replay.compare(&spec).map_err(value_err)?;
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
