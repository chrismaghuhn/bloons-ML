//! Parität zur Python-Referenz-Sim (`btd_sim`), bit-genau im f64-Build.
//!
//! Spielt den Golden-Trace aus `tests/data/parity_trace.json` nach (erzeugt mit
//! `tools/gen_parity_trace.py`): gleiche Aktionen, dann nach jeder Runde Vergleich von
//! akzeptierten Aktionen, Cash, Leben, Runde, Leaks, Pops, Rundendauer und allen Turmwerten.
//!
//!     cargo test -p btd-core --features f64 --test parity
#![cfg(feature = "f64")]

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, Game, Money};
use btd_data::{DamageType, GameData};
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

fn parse_action(v: &serde_json::Value) -> Action {
    let a = v.as_array().unwrap();
    let n = |i: usize| a[i].as_u64().unwrap() as u16;
    match a[0].as_str().unwrap() {
        "place" => Action::Place { kind: n(1) as u8, x: n(2), y: n(3) },
        "upgrade" => Action::Upgrade { x: n(1), y: n(2), path: n(3) as u8 },
        "sell" => Action::Sell { x: n(1), y: n(2) },
        other => panic!("unbekannte Aktion {other}"),
    }
}

fn dmg_name(t: DamageType) -> &'static str {
    match t {
        DamageType::Sharp => "sharp",
        DamageType::Explosive => "explosive",
        DamageType::Normal => "normal",
    }
}

fn state(g: &Game, r: &btd_core::RoundReport) -> State {
    assert_eq!(g.cash.0 % 1000, 0, "M1 kennt nur ganze Dollar");
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
                (
                    t.kind,
                    t.x,
                    t.y,
                    t.tiers[0],
                    t.tiers[1],
                    t.spent.dollars(),
                    t.range.to_bits(),
                    t.cooldown.to_bits(),
                    t.damage,
                    t.pierce,
                    dmg_name(t.dmg_type).to_string(),
                    t.blast.to_bits(),
                )
            })
            .collect(),
    }
}

#[test]
fn matches_python_reference_bit_for_bit() {
    let data = Arc::new(GameData::load(root().join("../../../data/sim_v0.json")).unwrap());
    let text = std::fs::read_to_string(root().join("tests/data/parity_trace.json")).unwrap();
    let trace: Trace = serde_json::from_str(&text).unwrap();
    assert_eq!(trace.format, "parity_trace/1");

    let mut rounds = 0;
    for (e, ep) in trace.episodes.iter().enumerate() {
        let mut g = Game::new(Arc::clone(&data)).unwrap();
        g.cash = Money::from_dollars(ep.start_cash);
        for (r, round) in ep.rounds.iter().enumerate() {
            for (i, a) in round.actions.iter().enumerate() {
                let action = parse_action(&a.action);
                let ok = g.apply(action).is_ok();
                assert_eq!(ok, a.ok, "Episode {e}, Runde {}, Aktion {i} {action:?}", r + 1);
            }
            let report = g.play_round();
            assert_eq!(state(&g, &report), round.after, "Episode {e}, Runde {}", r + 1);
            rounds += 1;
        }
        assert!(g.over(), "Episode {e}: Spiel sollte vorbei sein wie in Python");
    }
    assert!(rounds > 100, "Trace zu kurz ({rounds} Runden)");
}
