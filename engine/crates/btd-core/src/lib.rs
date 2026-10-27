//! Headless BTD6-Simulation. Kein IO, kein Python.
//!
//! M1: Funktionsumfang der Python-Referenz-Sim (`btd_sim`), Hitscan-Angriffe, Grid-Platzierung.
//! Mit dem Feature `f64` rechnet der Sim-Zustand in f64 und ist dann bit-identisch zu numpy,
//! was die Paritätstests nutzen. Standard ist f32 wie in Unity.

pub mod bloon;
pub mod game;
pub mod map;
pub mod money;
pub mod tower;

pub use game::{Action, ActionError, Game, RoundReport};
pub use money::Money;

#[cfg(feature = "f64")]
pub type Real = f64;
#[cfg(not(feature = "f64"))]
pub type Real = f32;

/// Konvertierung aus den f64-Daten in den Sim-Typ.
#[inline]
pub(crate) fn real(x: f64) -> Real {
    x as Real
}
