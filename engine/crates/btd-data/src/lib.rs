//! Datenformate und Laden/Validieren. Keine Spiellogik.
//!
//! Das Format ist `game_v1` (Modul [`v1`], docs/m2-plan.md). Namen werden beim Laden in Indizes
//! aufgelöst, sodass der Core nur mit IDs arbeitet.

pub mod v1;

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

pub(crate) fn invalid(msg: impl Into<String>) -> DataError {
    DataError::Invalid(msg.into())
}

/// Obergrenze für Geldbeträge gegen Überläufe bei von Hand gebauten Daten
/// (Money rechnet in 1/1000 $ als i64).
pub(crate) const MAX_DOLLARS: i64 = 1_000_000_000_000;

pub(crate) fn check_unique<'a>(names: impl Iterator<Item = &'a str>, what: &str) -> Result<(), DataError> {
    let mut seen = std::collections::HashSet::new();
    for n in names {
        if !seen.insert(n) {
            return Err(invalid(format!("{what} {n:?} ist doppelt")));
        }
    }
    Ok(())
}
