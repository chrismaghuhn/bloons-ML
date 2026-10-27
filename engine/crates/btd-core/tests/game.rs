use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, ActionError, Game, Money};
use btd_data::{GameData, Spawn};

fn data() -> GameData {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.json");
    GameData::load(path).expect("data/sim_v0.json laden (tools/export_sim_data.py)")
}

fn game() -> Game {
    Game::new(Arc::new(data())).unwrap()
}

const DART: u8 = 0;
const BOMB: u8 = 3;

#[test]
fn loads_reference_data() {
    let d = data();
    assert_eq!(d.rounds.len(), 40);
    assert_eq!(d.towers.len(), 4);
    assert_eq!(d.map.grid_w * d.map.grid_h, d.map.buildable.len());
}

#[test]
fn place_rules() {
    let mut g = game();
    assert_eq!(g.apply(Action::Place { kind: DART, x: 0, y: 5 }), Err(ActionError::NotBuildable));
    assert_eq!(g.apply(Action::Place { kind: DART, x: 99, y: 0 }), Err(ActionError::OutOfBounds));
    assert_eq!(g.apply(Action::Place { kind: 9, x: 7, y: 4 }), Err(ActionError::UnknownTower));
    assert!(g.apply(Action::Place { kind: DART, x: 7, y: 4 }).is_ok());
    assert_eq!(g.cash, Money::from_dollars(450));
    assert_eq!(g.apply(Action::Place { kind: DART, x: 7, y: 4 }), Err(ActionError::Occupied));
    assert_eq!(g.apply(Action::Place { kind: BOMB, x: 8, y: 4 }), Err(ActionError::NotEnoughCash));
}

#[test]
fn crosspath_rule() {
    let mut g = game();
    g.cash = Money::from_dollars(100_000);
    g.apply(Action::Place { kind: DART, x: 7, y: 4 }).unwrap();
    for _ in 0..2 {
        g.apply(Action::Upgrade { x: 7, y: 4, path: 1 }).unwrap();
    }
    g.apply(Action::Upgrade { x: 7, y: 4, path: 0 }).unwrap();
    g.apply(Action::Upgrade { x: 7, y: 4, path: 0 }).unwrap();
    // 2/2 ist erlaubt, 3/2 auch, aber dann ist der andere Pfad bei 2 gedeckelt
    g.apply(Action::Upgrade { x: 7, y: 4, path: 0 }).unwrap();
    assert_eq!(g.apply(Action::Upgrade { x: 7, y: 4, path: 1 }), Err(ActionError::UpgradeUnavailable));
    assert_eq!(g.apply(Action::Upgrade { x: 7, y: 4, path: 0 }), Err(ActionError::UpgradeUnavailable));
    assert_eq!(g.tower_at(7, 4).unwrap().tiers, [3, 2]);
}

#[test]
fn sell_refunds_and_keeps_build_order() {
    let mut g = game();
    g.cash = Money::from_dollars(10_000);
    for x in [7, 8, 9] {
        g.apply(Action::Place { kind: DART, x, y: 4 }).unwrap();
    }
    g.apply(Action::Upgrade { x: 8, y: 4, path: 0 }).unwrap(); // +90
    let before = g.cash;
    g.apply(Action::Sell { x: 8, y: 4 }).unwrap();
    assert_eq!(g.cash - before, Money::from_dollars(203)); // int(290 * 0.7)
    assert_eq!(g.towers.iter().map(|t| t.x).collect::<Vec<_>>(), vec![7, 9]);
    assert!(!g.is_occupied(8, 4));
}

fn scripted(g: &mut Game) -> Vec<u64> {
    let mut hashes = vec![];
    g.apply(Action::Place { kind: DART, x: 7, y: 4 }).unwrap();
    g.apply(Action::Place { kind: DART, x: 5, y: 6 }).unwrap();
    while !g.over() {
        g.play_round();
        for (x, y) in [(4, 6), (7, 6), (11, 6), (13, 5), (8, 8)] {
            for k in [BOMB, DART] {
                if g.apply(Action::Place { kind: k, x, y }).is_ok() {
                    break;
                }
            }
        }
        let cells: Vec<_> = g.towers.iter().map(|t| (t.x, t.y)).collect();
        for (x, y) in cells {
            for path in 0..2 {
                let _ = g.apply(Action::Upgrade { x, y, path });
            }
        }
        hashes.push(g.state_hash());
    }
    hashes
}

#[test]
fn deterministic_replay() {
    let (mut a, mut b) = (game(), game());
    let ha = scripted(&mut a);
    assert!(ha.len() > 5, "Spiel sollte einige Runden dauern");
    assert_eq!(ha, scripted(&mut b));
    a.reset();
    assert_eq!(ha, scripted(&mut a), "reset() muss vollständig zurücksetzen");
}

#[test]
fn cooldown_is_not_snapped_to_tick() {
    for cooldown in [0.12, 0.2, 0.23] {
        let mut d = data();
        d.rounds[0] = (0..100).map(|i| Spawn { time: i as f64 * 0.1, kind: 0 }).collect();
        let mut g = Game::new(Arc::new(d)).unwrap();
        g.cash = Money::from_dollars(10_000);
        g.apply(Action::Place { kind: DART, x: 7, y: 4 }).unwrap();
        let tw = &mut g.towers[0];
        tw.range = 99.0;
        tw.pierce = 1;
        tw.cooldown = cooldown as btd_core::Real;
        let r = g.play_round();
        assert_eq!((r.pops, r.leaked), (100, 0));
        assert!((r.seconds - 99.0 * cooldown).abs() <= 0.1 + 1e-6, "{cooldown}: {r:?}");
    }
}

#[test]
fn rejects_invalid_hand_built_data() {
    let mut d = data();
    d.bloons[0].children = vec![0]; // red -> red: Zyklus
    assert!(Game::new(Arc::new(d)).is_err());
}

#[test]
fn losing_ends_the_game() {
    let mut g = game();
    while !g.over() {
        g.play_round();
    }
    assert_eq!(g.lives, 0);
    assert!(!g.won());
    assert_eq!(g.apply(Action::Place { kind: DART, x: 7, y: 4 }), Err(ActionError::GameOver));
    assert_eq!(g.play_round().ticks, 0);
}
