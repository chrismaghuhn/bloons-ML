//! Parität zur Python-Referenz-Sim (`btd_sim`), bit-genau im f64-Build.
//!
//! Spielt den Golden-Trace aus `tests/data/parity_trace.json` nach (erzeugt mit
//! `tools/gen_parity_trace.py`), diesmal über das Format `game_v1` (`data/sim_v0.game_v1.json`):
//! gleiche Aktionen, dann nach jeder Runde Vergleich von akzeptierten Aktionen, Cash, Leben,
//! Runde, Leaks, Pops, Rundendauer und allen Turmwerten.
//!
//!     cargo test -p btd-core --features f64 --test parity
#![cfg(feature = "f64")]

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, Game, Money};
use btd_data::v1::{Delivery, GameV1, Props};
use serde::Deserialize;

#[derive(Deserialize)]
struct Trace {
    format: String,
    episodes: Vec<Episode>,
}

#[derive(Deserialize)]
struct Episode {
    start_cash: i64,
    rounds: Vec<Round>,
}

#[derive(Deserialize)]
struct Round {
    actions: Vec<TracedAction>,
    after: State,
}

#[derive(Deserialize)]
struct TracedAction {
    action: serde_json::Value,
    ok: bool,
}

/// (kind, x, y, tier0, tier1, spent, range_bits, cooldown_bits, damage, pierce, dmg_type, blast_bits)
type TowerRow = (u8, u16, u16, u8, u8, i64, u64, u64, i32, u32, String, u64);

#[derive(Deserialize, Debug, PartialEq)]
struct State {
    cash: i64,
    lives: i32,
    round: u32,
    leaked: i32,
    pops: u32,
    seconds: u64,
    towers: Vec<TowerRow>,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Zellmittelpunkt im sim_v0-Raster (1 Zelle = 1 Einheit).
fn center(c: u64) -> f64 {
    c as f64 + 0.5
}

/// Aktion aus dem Trace; Upgrade/Verkauf wählen den Turm über die Zelle. Ohne Turm dort
/// wird eine ungültige Turm-ID genommen, damit die Engine die Aktion ablehnt (wie Python).
fn parse_action(g: &Game, v: &serde_json::Value) -> Action {
    let a = v.as_array().unwrap();
    let n = |i: usize| a[i].as_u64().unwrap();
    let id_at = |x, y| g.tower_at(center(x), center(y)).map_or(u32::MAX, |t| t.id);
    match a[0].as_str().unwrap() {
        "place" => Action::Place { kind: n(1) as u8, x: center(n(2)), y: center(n(3)) },
        "upgrade" => Action::Upgrade { tower: id_at(n(1), n(2)), path: n(3) as u8 },
        "sell" => Action::Sell { tower: id_at(n(1), n(2)) },
        other => panic!("unbekannte Aktion {other}"),
    }
}

fn dmg_name(cannot: Props) -> &'static str {
    match cannot.0 {
        Props::LEAD => "sharp",
        Props::BLACK => "explosive",
        0 => "normal",
        _ => "?",
    }
}

fn state(g: &Game, data: &GameV1, r: &btd_core::RoundReport) -> State {
    assert_eq!(g.cash.0 % 1000, 0, "sim_v0 kennt nur ganze Dollar");
    State {
        cash: g.cash.dollars(),
        lives: g.lives,
        round: g.round,
        leaked: r.leaked,
        pops: r.pops,
        seconds: r.seconds.to_bits(),
        towers: g
            .towers
            .iter()
            .map(|t| {
                let v = data.towers[t.kind as usize].variant(t.tiers).unwrap();
                let a = &t.attacks[0];
                let blast = match v.attacks[0].delivery {
                    Delivery::Hitscan {} => 0.0,
                    Delivery::HitscanSplash { radius } => radius,
                };
                (
                    t.kind,
                    (t.x - 0.5) as u16,
                    (t.y - 0.5) as u16,
                    t.tiers[0],
                    t.tiers[1],
                    t.spent.dollars(),
                    v.range.to_bits(),
                    a.cooldown.to_bits(),
                    a.damage as i32,
                    a.pierce,
                    dmg_name(a.cannot_damage).to_string(),
                    blast.to_bits(),
                )
            })
            .collect(),
    }
}

#[test]
fn matches_python_reference_bit_for_bit() {
    let data = Arc::new(GameV1::load(root().join("../../../data/sim_v0.game_v1.json")).unwrap());
    let text = std::fs::read_to_string(root().join("tests/data/parity_trace.json")).unwrap();
    let trace: Trace = serde_json::from_str(&text).unwrap();
    assert_eq!(trace.format, "parity_trace/1");

    let mut rounds = 0;
    for (e, ep) in trace.episodes.iter().enumerate() {
        let mut g = Game::new(Arc::clone(&data)).unwrap();
        g.cash = Money::from_dollars(ep.start_cash);
        for (r, round) in ep.rounds.iter().enumerate() {
            for (i, a) in round.actions.iter().enumerate() {
                let action = parse_action(&g, &a.action);
                let ok = g.apply(action).is_ok();
                assert_eq!(ok, a.ok, "Episode {e}, Runde {}, Aktion {i} {action:?}", r + 1);
            }
            let report = g.play_round();
            assert_eq!(state(&g, &data, &report), round.after, "Episode {e}, Runde {}", r + 1);
            rounds += 1;
        }
        assert!(g.over(), "Episode {e}: Spiel sollte vorbei sein wie in Python");
    }
    assert!(rounds > 100, "Trace zu kurz ({rounds} Runden)");
}
