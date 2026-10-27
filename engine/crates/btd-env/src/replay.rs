//! Spielprotokolle (`.jsonl`): aufzeichnen, speichern, nachspielen, ab einer Runde weiterspielen.
//!
//! Format `btd-replay/2`, eine JSON-Zeile pro Eintrag (Rundennummern sind die echten Spielrunden,
//! ein CHIMPS-Protokoll beginnt also bei Runde 6):
//!
//! ```text
//! {"type":"header","format":"btd-replay/2","data":"sim_v0.game_v1.json","data_hash":"…","float":"f32","max_builds_per_round":6}
//! {"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7.5,"y":4.5}}],"result":{"lives":150,"cash":671,"round":1,"leaked":0,"pops":20,"hash":"…"}}
//! ```
//!
//! `result` ist optional bzw. teilweise ausfüllbar: Ein von Hand aufgeschriebenes echtes Spiel
//! enthält z.B. nur `lives` und `cash`. Beim Nachspielen werden nur vorhandene Felder verglichen.
//! `hash` ist nur zwischen gleichen Builds vergleichbar (f32 vs. f64), deshalb steht `float` im Header.

use std::path::Path;

use btd_core::{Action, Game, RoundReport};
use btd_data::v1::GameV1;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EnvConfig, EnvSpec};

pub const FORMAT: &str = "btd-replay/2";

/// Float-Build der Engine ("f32"/"f64"), kommt aus btd-core (dessen Feature entscheidet).
pub use btd_core::FLOAT;

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
    #[error(
        "Replay wurde mit dem {replay}-Build aufgezeichnet, diese Engine ist {ours}: Ergebnisse sind \
         nur im gleichen Build reproduzierbar (nicht-strikt vergleichen oder passend bauen)"
    )]
    FloatMismatch { replay: String, ours: &'static str },
    #[error("Spiel endet nach Runde {ended}, das Protokoll hat aber {total} Runden")]
    TrailingRounds { ended: u32, total: u32 },
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayAction {
    /// Turm nach Name an eine Weltposition setzen.
    Place {
        tower: String,
        x: f64,
        y: f64,
    },
    /// Turm, dessen Footprint die Position enthält.
    Upgrade {
        x: f64,
        y: f64,
        path: u8,
    },
    Sell {
        x: f64,
        y: f64,
    },
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
    /// Aktionen, die die Sim nicht ausführen konnte (nur im nicht-strikten Modus; dort werden
    /// sie übersprungen, z.B. weil im echten Spiel mehr Cash da war).
    pub action_errors: Vec<String>,
    /// Aktionen, die tatsächlich ausgeführt wurden (ohne die übersprungenen).
    pub applied: Vec<ReplayAction>,
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
        if rounds.first().is_some_and(|r| r.round == 0) {
            return Err(ReplayError::Parse { line: 2, msg: "Runden zählen ab 1".into() });
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
        let (game, checks) = self.replay_to(spec, until_round, true)?;
        checks.iter().for_each(&mut on_round);
        Ok(game)
    }

    /// Spielt alles nach und sammelt die Abweichungen, statt abzubrechen (Kalibrierung gegen
    /// das echte Spiel: dort weichen die Werte erwartungsgemäß ab). Aktionen, die die Sim nicht
    /// ausführen kann, werden übersprungen und in `RoundCheck::action_errors` vermerkt.
    pub fn compare(&self, spec: &EnvSpec) -> Result<Vec<RoundCheck>, ReplayError> {
        self.replay_to(spec, None, false).map(|(_, checks)| checks)
    }

    /// Nachspielen bis einschließlich Runde `until`. `strict`: Abbruch bei abweichendem
    /// Ergebnis, unmöglicher Aktion oder anderem Float-Build; sonst wird alles gesammelt.
    pub fn replay_to(
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
        if let Some(f) = self.header.float.as_deref().filter(|&f| f != FLOAT) {
            if strict {
                return Err(ReplayError::FloatMismatch { replay: f.to_string(), ours: FLOAT });
            }
        }
        let compare_hash = self.header.float.as_deref() == Some(FLOAT);
        let mut game = Game::new(spec.data.clone()).map_err(|e| ReplayError::DataMismatch(e.to_string()))?;
        let first = spec.data.rules.start_round;
        if let Some(e) = self.rounds.first().filter(|e| e.round != first) {
            return Err(ReplayError::DataMismatch(format!(
                "Protokoll beginnt bei Runde {}, die Daten bei Runde {first}",
                e.round
            )));
        }
        let mut checks = Vec::new();
        for entry in &self.rounds {
            if until.is_some_and(|u| entry.round > u) {
                break;
            }
            let mut action_errors = Vec::new();
            let mut applied = Vec::new();
            for a in &entry.actions {
                let Some(action) = to_action(&game, &spec.data, a) else {
                    let msg = format!("Runde {}: unbekannter Turm in {a:?}", entry.round);
                    if strict {
                        return Err(ReplayError::DataMismatch(msg));
                    }
                    action_errors.push(msg);
                    continue;
                };
                match game.apply(action) {
                    Ok(_) => applied.push(a.clone()),
                    Err(err) if strict => {
                        return Err(ReplayError::Action { round: entry.round, action: a.clone(), err })
                    }
                    Err(err) => action_errors.push(format!("{a:?}: {err:?}")),
                }
            }
            let report = game.play_round();
            let got = RoundResult::from_game(&game, &report);
            let matches = action_errors.is_empty() && entry.result.matches(&got, compare_hash);
            if strict && !matches {
                return Err(ReplayError::Mismatch {
                    round: entry.round,
                    expected: Box::new(entry.result.clone()),
                    got: Box::new(got),
                });
            }
            checks.push(RoundCheck {
                round: entry.round,
                expected: entry.result.clone(),
                got,
                matches,
                action_errors,
                applied,
            });
            if game.over() {
                let total = self.rounds.last().map_or(0, |r| r.round);
                if strict && until.map_or(true, |u| u > entry.round) && total > entry.round {
                    return Err(ReplayError::TrailingRounds { ended: entry.round, total });
                }
                break;
            }
        }
        Ok((game, checks))
    }
}

/// Protokollaktion -> Spielaktion. `None` nur bei unbekanntem Turmnamen; steht an der Position
/// kein Turm, entsteht eine Aktion mit ungültiger ID, die die Engine mit `NoTower` ablehnt.
fn to_action(game: &Game, data: &GameV1, a: &ReplayAction) -> Option<Action> {
    let id_at = |x: f64, y: f64| game.tower_at(x, y).map_or(u32::MAX, |t| t.id);
    Some(match a {
        ReplayAction::Place { tower, x, y } => {
            let kind = data.towers.iter().position(|t| &t.name == tower)? as u8;
            Action::Place { kind, x: *x, y: *y }
        }
        ReplayAction::Upgrade { x, y, path } => Action::Upgrade { tower: id_at(*x, *y), path: *path },
        ReplayAction::Sell { x, y } => Action::Sell { tower: id_at(*x, *y) },
    })
}

/// Spielaktion -> Protokollaktion; vor dem Ausführen aufrufen (beim Verkaufen ist der Turm
/// danach weg). Türme werden über ihre Position beschrieben, nicht über die interne ID.
pub fn describe(game: &Game, a: Action) -> ReplayAction {
    let pos = |id| game.tower(id).map_or((f64::NAN, f64::NAN), |t| t.pos());
    match a {
        Action::Place { kind, x, y } => {
            ReplayAction::Place { tower: game.data().towers[kind as usize].name.clone(), x, y }
        }
        Action::Upgrade { tower, path } => {
            let (x, y) = pos(tower);
            ReplayAction::Upgrade { x, y, path }
        }
        Action::Sell { tower } => {
            let (x, y) = pos(tower);
            ReplayAction::Sell { x, y }
        }
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

    /// Weiter aufzeichnen ab einem nachgespielten Spielstand. Übernommen wird, was die Sim
    /// tatsächlich gemacht hat (ausgeführte Aktionen, eigene Ergebnisse), nicht der Inhalt der
    /// Quelldatei: nach einem nicht-strikten Laden (echtes Spiel) bleibt das Protokoll so in
    /// sich stimmig und strikt nachspielbar.
    pub fn resume(spec: &EnvSpec, cfg: &EnvConfig, checks: &[RoundCheck]) -> Self {
        let mut r = Self::new(spec, cfg);
        r.replay.rounds = checks
            .iter()
            .map(|c| RoundEntry { round: c.round, actions: c.applied.clone(), result: c.got.clone() })
            .collect();
        r
    }

    pub fn action(&mut self, a: ReplayAction) {
        self.pending.push(a);
    }

    /// `round` = Nummer der gerade gespielten Runde (vor dem Spielen `game.round + 1`).
    pub fn round_played(&mut self, round: u32, g: &Game, report: &RoundReport) {
        self.replay.rounds.push(RoundEntry {
            round,
            actions: std::mem::take(&mut self.pending),
            result: RoundResult::from_game(g, report),
        });
    }

    /// Bisheriges Protokoll. Bauaktionen seit der letzten Runde fehlen noch: sie werden erst
    /// mit dem Ergebnis der nächsten Runde als Eintrag geschrieben.
    pub fn replay(&self) -> &Replay {
        &self.replay
    }
}
