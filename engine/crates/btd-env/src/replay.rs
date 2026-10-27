//! Spielprotokolle (`.jsonl`): aufzeichnen, speichern, nachspielen, ab einer Runde weiterspielen.
//!
//! Format `btd-replay/1`, eine JSON-Zeile pro Eintrag:
//!
//! ```text
//! {"type":"header","format":"btd-replay/1","data":"sim_v0.json","data_hash":"…","float":"f32","max_builds_per_round":6}
//! {"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7,"y":4}}],"result":{"lives":150,"cash":671,"round":1,"leaked":0,"pops":20,"hash":"…"}}
//! ```
//!
//! `result` ist optional bzw. teilweise ausfüllbar: Ein von Hand aufgeschriebenes echtes Spiel
//! enthält z.B. nur `lives` und `cash`. Beim Nachspielen werden nur vorhandene Felder verglichen.
//! `hash` ist nur zwischen gleichen Builds vergleichbar (f32 vs. f64), deshalb steht `float` im Header.

use std::path::Path;

use btd_core::{Action, Game, RoundReport};
use btd_data::GameData;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EnvConfig, EnvSpec};

pub const FORMAT: &str = "btd-replay/1";

#[cfg(feature = "f64")]
pub const FLOAT: &str = "f64";
#[cfg(not(feature = "f64"))]
pub const FLOAT: &str = "f32";

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("Zeile {line}: {msg}")]
    Parse { line: usize, msg: String },
    #[error("Datei {0}: {1}")]
    Io(String, std::io::Error),
    #[error("Replay passt nicht zu den Daten: {0}")]
    DataMismatch(String),
    #[error("Runde {round}: Aktion {action:?} nicht möglich ({err:?})")]
    Action { round: u32, action: ReplayAction, err: btd_core::ActionError },
    #[error("Runde {round}: Ergebnis weicht ab, erwartet {expected:?}, bekommen {got:?}")]
    Mismatch { round: u32, expected: Box<RoundResult>, got: Box<RoundResult> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub format: String,
    pub data: String,
    /// xxh3 der Datendatei (hex). Fehlt er, wird nicht geprüft (z.B. von Hand geschrieben).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub float: Option<String>,
    #[serde(default = "default_builds")]
    pub max_builds_per_round: u32,
}

fn default_builds() -> u32 {
    EnvConfig::default().max_builds_per_round
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayAction {
    Place { tower: String, x: u16, y: u16 },
    Upgrade { x: u16, y: u16, path: u8 },
    Sell { x: u16, y: u16 },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoundResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lives: Option<i32>,
    /// Ganze Dollar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash: Option<i64>,
    /// Anzahl geschaffter Runden danach.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaked: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pops: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

impl RoundResult {
    fn from_game(g: &Game, r: &RoundReport) -> Self {
        RoundResult {
            lives: Some(g.lives),
            cash: Some(g.cash.dollars()),
            round: Some(g.round),
            leaked: Some(r.leaked),
            pops: Some(r.pops),
            hash: Some(format!("{:016x}", g.state_hash())),
        }
    }

    /// Stimmen alle Felder überein, die in `self` gesetzt sind?
    fn matches(&self, got: &RoundResult, compare_hash: bool) -> bool {
        fn eq<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
            a.is_none() || a == b
        }
        eq(&self.lives, &got.lives)
            && eq(&self.cash, &got.cash)
            && eq(&self.round, &got.round)
            && eq(&self.leaked, &got.leaked)
            && eq(&self.pops, &got.pops)
            && (!compare_hash || eq(&self.hash, &got.hash))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoundEntry {
    /// Nummer der gespielten Runde (1-basiert).
    pub round: u32,
    #[serde(default)]
    pub actions: Vec<ReplayAction>,
    #[serde(default)]
    pub result: RoundResult,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Line {
    Header(Header),
    Round(RoundEntry),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub header: Header,
    pub rounds: Vec<RoundEntry>,
}

/// Ergebnis einer nachgespielten Runde.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundCheck {
    pub round: u32,
    pub expected: RoundResult,
    pub got: RoundResult,
    pub matches: bool,
}

impl Replay {
    pub fn to_jsonl(&self) -> String {
        let mut out = serde_json::to_string(&Line::Header(self.header.clone())).unwrap();
        out.push('\n');
        for r in &self.rounds {
            out.push_str(&serde_json::to_string(&Line::Round(r.clone())).unwrap());
            out.push('\n');
        }
        out
    }

    pub fn from_jsonl(text: &str) -> Result<Self, ReplayError> {
        let mut header = None;
        let mut rounds = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let parsed: Line = serde_json::from_str(line)
                .map_err(|e| ReplayError::Parse { line: i + 1, msg: e.to_string() })?;
            match parsed {
                Line::Header(h) if header.is_none() && rounds.is_empty() => header = Some(h),
                Line::Header(_) => {
                    return Err(ReplayError::Parse {
                        line: i + 1,
                        msg: "Header muss die erste Zeile sein".into(),
                    })
                }
                Line::Round(r) if header.is_some() => rounds.push(r),
                Line::Round(_) => return Err(ReplayError::Parse { line: i + 1, msg: "Header fehlt".into() }),
            }
        }
        let header = header.ok_or(ReplayError::Parse { line: 1, msg: "leere Datei".into() })?;
        if header.format != FORMAT {
            return Err(ReplayError::Parse {
                line: 1,
                msg: format!("Format {:?} nicht unterstützt", header.format),
            });
        }
        if rounds.windows(2).any(|w| w[1].round != w[0].round + 1) {
            return Err(ReplayError::Parse { line: 0, msg: "Rundennummern nicht fortlaufend".into() });
        }
        if rounds.first().is_some_and(|r| r.round != 1) {
            return Err(ReplayError::Parse { line: 2, msg: "erste Runde muss 1 sein".into() });
        }
        Ok(Replay { header, rounds })
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ReplayError> {
        let path = path.as_ref();
        std::fs::write(path, self.to_jsonl()).map_err(|e| ReplayError::Io(path.display().to_string(), e))
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ReplayError> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).map_err(|e| ReplayError::Io(path.display().to_string(), e))?;
        Self::from_jsonl(&text)
    }

    /// Spielt das Protokoll nach (strikt: Abbruch bei der ersten Abweichung) und liefert den
    /// Spielstand nach Runde `until_round` (bzw. nach der letzten Runde).
    pub fn play(
        &self,
        spec: &EnvSpec,
        until_round: Option<u32>,
        mut on_round: impl FnMut(&RoundCheck),
    ) -> Result<Game, ReplayError> {
        let (game, checks) = self.run(spec, until_round, true)?;
        checks.iter().for_each(&mut on_round);
        Ok(game)
    }

    /// Spielt alles nach und sammelt die Abweichungen, statt abzubrechen (Kalibrierung gegen
    /// das echte Spiel: dort weichen die Werte erwartungsgemäß ab).
    pub fn compare(&self, spec: &EnvSpec) -> Result<Vec<RoundCheck>, ReplayError> {
        self.run(spec, None, false).map(|(_, checks)| checks)
    }

    fn run(
        &self,
        spec: &EnvSpec,
        until: Option<u32>,
        strict: bool,
    ) -> Result<(Game, Vec<RoundCheck>), ReplayError> {
        if let Some(h) = &self.header.data_hash {
            let ours = format!("{:016x}", spec.data_hash);
            if *h != ours {
                return Err(ReplayError::DataMismatch(format!(
                    "data_hash {h} != {ours} ({})",
                    spec.data_name
                )));
            }
        }
        let compare_hash = self.header.float.as_deref() == Some(FLOAT);
        let mut game = Game::new(spec.data.clone()).map_err(|e| ReplayError::DataMismatch(e.to_string()))?;
        let mut checks = Vec::new();
        for entry in &self.rounds {
            if until.is_some_and(|u| entry.round > u) {
                break;
            }
            for a in &entry.actions {
                let action = to_action(&spec.data, a).ok_or_else(|| {
                    ReplayError::DataMismatch(format!("Runde {}: unbekannter Turm in {a:?}", entry.round))
                })?;
                game.apply(action).map_err(|err| ReplayError::Action {
                    round: entry.round,
                    action: a.clone(),
                    err,
                })?;
            }
            let report = game.play_round();
            let got = RoundResult::from_game(&game, &report);
            let matches = entry.result.matches(&got, compare_hash);
            if strict && !matches {
                return Err(ReplayError::Mismatch {
                    round: entry.round,
                    expected: Box::new(entry.result.clone()),
                    got: Box::new(got),
                });
            }
            checks.push(RoundCheck { round: entry.round, expected: entry.result.clone(), got, matches });
        }
        Ok((game, checks))
    }
}

fn to_action(data: &GameData, a: &ReplayAction) -> Option<Action> {
    Some(match a {
        ReplayAction::Place { tower, x, y } => {
            let kind = data.towers.iter().position(|t| &t.name == tower)? as u8;
            Action::Place { kind, x: *x, y: *y }
        }
        ReplayAction::Upgrade { x, y, path } => Action::Upgrade { x: *x, y: *y, path: *path },
        ReplayAction::Sell { x, y } => Action::Sell { x: *x, y: *y },
    })
}

fn from_action(data: &GameData, a: Action) -> ReplayAction {
    match a {
        Action::Place { kind, x, y } => {
            ReplayAction::Place { tower: data.towers[kind as usize].name.clone(), x, y }
        }
        Action::Upgrade { x, y, path } => ReplayAction::Upgrade { x, y, path },
        Action::Sell { x, y } => ReplayAction::Sell { x, y },
    }
}

/// Schreibt während des Spiels mit: erfolgreiche Aktionen und das Ergebnis jeder Runde.
#[derive(Clone, Debug)]
pub struct Recorder {
    replay: Replay,
    pending: Vec<ReplayAction>,
}

impl Recorder {
    pub fn new(spec: &EnvSpec, cfg: &EnvConfig) -> Self {
        Recorder {
            replay: Replay {
                header: Header {
                    format: FORMAT.into(),
                    data: spec.data_name.clone(),
                    data_hash: Some(format!("{:016x}", spec.data_hash)),
                    float: Some(FLOAT.into()),
                    max_builds_per_round: cfg.max_builds_per_round,
                },
                rounds: Vec::new(),
            },
            pending: Vec::new(),
        }
    }

    /// Weiter aufzeichnen ab einem geladenen Spielstand (Runden bis `upto` übernehmen).
    pub fn resume(spec: &EnvSpec, cfg: &EnvConfig, from: &Replay, upto: u32) -> Self {
        let mut r = Self::new(spec, cfg);
        r.replay.rounds = from.rounds.iter().filter(|e| e.round <= upto).cloned().collect();
        r
    }

    pub fn action(&mut self, data: &GameData, a: Action) {
        self.pending.push(from_action(data, a));
    }

    pub fn round_played(&mut self, g: &Game, report: &RoundReport) {
        let round = self.replay.rounds.last().map_or(1, |r| r.round + 1);
        self.replay.rounds.push(RoundEntry {
            round,
            actions: std::mem::take(&mut self.pending),
            result: RoundResult::from_game(g, report),
        });
    }

    pub fn replay(&self) -> &Replay {
        &self.replay
    }
}
