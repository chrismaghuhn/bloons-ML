use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, ActionError, Game, Money};
use btd_data::v1::{Flags, GameV1, SpawnGroup};

fn data() -> GameV1 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.game_v1.json");
    GameV1::load(path).expect("data/sim_v0.game_v1.json laden (tools/export_sim_data.py)")
}

fn game() -> Game {
    Game::new(Arc::new(data())).unwrap()
}

const DART: u8 = 0;
const BOMB: u8 = 3;

fn place(g: &mut Game, kind: u8, cx: u16, cy: u16) -> Result<u32, ActionError> {
    g.apply(Action::Place { kind, x: cx as f64 + 0.5, y: cy as f64 + 0.5 }).map(|id| id.unwrap())
}

#[test]
fn loads_reference_data() {
    let d = data();
    assert_eq!(d.rounds.len(), 40);
    assert_eq!(d.towers.len(), 4);
    assert_eq!(d.map.grid.w * d.map.grid.h, d.map.grid.buildable.len());
}

#[test]
fn place_rules() {
    let mut g = game();
    assert_eq!(place(&mut g, DART, 0, 5), Err(ActionError::NotPlaceable), "Pfad");
    assert_eq!(place(&mut g, DART, 99, 0), Err(ActionError::NotPlaceable), "außerhalb");
    assert_eq!(place(&mut g, 9, 7, 4), Err(ActionError::UnknownTower));
    assert!(place(&mut g, DART, 7, 4).is_ok());
    assert_eq!(g.cash, Money::from_dollars(450));
    assert_eq!(place(&mut g, DART, 7, 4), Err(ActionError::Occupied));
    // Footprints berühren sich in der Nachbarzelle nur, das ist erlaubt
    assert!(g.can_place_at(DART, 8.5, 4.5));
    assert!(!g.can_place_at(DART, 7.9, 4.5), "überlappt");
    assert_eq!(place(&mut g, BOMB, 8, 4), Err(ActionError::NotEnoughCash));
}

#[test]
fn crosspath_rule_comes_from_variants() {
    let mut g = game();
    g.cash = Money::from_dollars(100_000);
    let id = place(&mut g, DART, 7, 4).unwrap();
    for path in [1, 1, 0, 0, 0] {
        g.apply(Action::Upgrade { tower: id, path }).unwrap();
    }
    for path in [0, 1, 2] {
        assert_eq!(g.apply(Action::Upgrade { tower: id, path }), Err(ActionError::UpgradeUnavailable));
    }
    assert_eq!(g.tower(id).unwrap().tiers, [3, 2, 0]);
}

#[test]
fn sell_refunds_and_keeps_build_order() {
    let mut g = game();
    g.cash = Money::from_dollars(10_000);
    let ids: Vec<u32> = [7, 8, 9].iter().map(|&x| place(&mut g, DART, x, 4).unwrap()).collect();
    g.apply(Action::Upgrade { tower: ids[1], path: 0 }).unwrap(); // +90
    let before = g.cash;
    g.apply(Action::Sell { tower: ids[1] }).unwrap();
    assert_eq!(g.cash - before, Money::from_dollars(203)); // int(290 * 0.7)
    assert_eq!(g.towers.iter().map(|t| t.id).collect::<Vec<_>>(), vec![ids[0], ids[2]]);
    assert!(g.can_place_at(DART, 8.5, 4.5));
    assert_eq!(g.apply(Action::Sell { tower: ids[1] }), Err(ActionError::NoTower));
}

#[test]
fn selling_can_be_disabled() {
    let mut d = data();
    d.rules.selling = false;
    let mut g = Game::new(Arc::new(d)).unwrap();
    let id = place(&mut g, DART, 7, 4).unwrap();
    assert_eq!(g.apply(Action::Sell { tower: id }), Err(ActionError::SellingDisabled));
}

fn scripted(g: &mut Game) -> Vec<u64> {
    let mut hashes = vec![];
    place(g, DART, 7, 4).unwrap();
    place(g, DART, 5, 6).unwrap();
    while !g.over() {
        g.play_round();
        for (x, y) in [(4, 6), (7, 6), (11, 6), (13, 5), (8, 8)] {
            for k in [BOMB, DART] {
                if place(g, k, x, y).is_ok() {
                    break;
                }
            }
        }
        let ids: Vec<_> = g.towers.iter().map(|t| t.id).collect();
        for id in ids {
            for path in 0..2 {
                let _ = g.apply(Action::Upgrade { tower: id, path });
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
        d.rounds[0] = vec![SpawnGroup { kind: 0, flags: Flags::default(), count: 100, start: 0.0, end: 9.9 }];
        let mut g = Game::new(Arc::new(d)).unwrap();
        g.cash = Money::from_dollars(10_000);
        place(&mut g, DART, 7, 4).unwrap();
        let a = &mut g.towers[0].attacks[0];
        a.range2 = 99.0 * 99.0;
        a.pierce = 1;
        a.cooldown = cooldown as btd_core::Real;
        let r = g.play_round();
        assert_eq!((r.pops, r.leaked), (100, 0));
        assert!((r.seconds - 99.0 * cooldown).abs() <= 0.1 + 1e-6, "{cooldown}: {r:?}");
    }
}

#[test]
fn camo_bloons_need_camo_detection() {
    let mut d = data();
    d.rounds[0] = vec![SpawnGroup { kind: 0, flags: Flags(Flags::CAMO), count: 5, start: 0.0, end: 1.0 }];
    for (camo, expected_pops) in [(false, 0), (true, 5)] {
        let mut g = Game::new(Arc::new(d.clone())).unwrap();
        place(&mut g, DART, 7, 4).unwrap();
        g.towers[0].attacks[0].camo = camo;
        let r = g.play_round();
        assert_eq!(r.pops, expected_pops, "camo={camo}");
    }
}

#[test]
fn start_round_and_income_come_from_data() {
    let mut d = data();
    d.rules.start_round = 6;
    d.income[5].cash_per_pop = 0.5;
    let mut g = Game::new(Arc::new(d)).unwrap();
    assert_eq!(g.round, 5);
    let before = g.cash;
    let r = g.play_round();
    assert_eq!(g.round, 6);
    let bonus = Money::from_dollars(106);
    assert_eq!(g.cash - before, Money::from_f64(r.pops as f64 * 0.5) + bonus);
}

#[test]
fn rejects_invalid_hand_built_data() {
    let mut d = data();
    d.bloons[0].children = vec![(0, 1)]; // red -> red: Zyklus
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
    assert_eq!(place(&mut g, DART, 7, 4), Err(ActionError::GameOver));
    assert_eq!(g.play_round().ticks, 0);
}
