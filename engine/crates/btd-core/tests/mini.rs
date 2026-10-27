//! Engine-Verhalten auf der synthetischen Fixture `data/fixtures/game_v1_mini.json`
//! (3 Upgrade-Pfade, Polygon-Platzierung, Camo, mehr-HP-Bloons, 60 Ticks/s). Läuft in der CI,
//! wo der echte Spiel-Dump nicht vorhanden ist.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, ActionError, Game, Money};
use btd_data::v1::GameV1;

fn data() -> GameV1 {
    GameV1::load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json"))
        .unwrap()
}

fn game() -> Game {
    Game::new(Arc::new(data())).unwrap()
}

const PIN: u8 = 0;
const BOMB: u8 = 1;

#[test]
fn polygon_placement() {
    let g = game();
    assert!(g.can_place_at(PIN, 0.0, 20.0), "Land");
    assert!(!g.can_place_at(PIN, 0.0, 2.0), "Track");
    assert!(!g.can_place_at(PIN, 40.0, 30.0), "gesperrte Fläche");
    assert!(!g.can_place_at(PIN, 48.0, -20.0), "Footprint ragt über den Rand");
}

#[test]
fn three_paths_with_crosspath_limit_from_variants() {
    let mut g = game();
    g.cash = Money::from_dollars(10_000);
    let id = g.apply(Action::Place { kind: PIN, x: 0.0, y: 20.0 }).unwrap().unwrap();
    g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
    g.apply(Action::Upgrade { tower: id, path: 2 }).unwrap();
    assert_eq!(
        g.apply(Action::Upgrade { tower: id, path: 1 }),
        Err(ActionError::UpgradeUnavailable),
        "1-1-1"
    );
    g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
    assert_eq!(g.tower(id).unwrap().tiers, [2, 0, 1]);
    assert_eq!(g.tower(id).unwrap().attacks[0].pierce, 6);
    assert_eq!(g.tower(id).unwrap().spent, Money::from_dollars(100 + 50 + 40 + 100));
}

fn play_three_rounds(build: impl Fn(&mut Game)) -> (Game, Vec<btd_core::RoundReport>) {
    let mut g = game();
    build(&mut g);
    let reports = (0..3).map(|_| g.play_round()).collect();
    (g, reports)
}

#[test]
fn camo_needs_detection_and_lead_needs_the_right_damage() {
    // Basis-Pin: kein Camo, kann kein Lead
    let (_, base) = play_three_rounds(|g| {
        g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
    });
    // Pin mit Camo (Pfad 2) plus Bomb gegen Lead
    let (_, strong) = play_three_rounds(|g| {
        g.cash = Money::from_dollars(10_000);
        let id = g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap().unwrap();
        g.apply(Action::Upgrade { tower: id, path: 1 }).unwrap();
        g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
        g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
        g.apply(Action::Place { kind: BOMB, x: -20.0, y: -15.0 }).unwrap();
    });
    assert!(base[1].leaked >= 3, "Camo-Reds kommen am Basis-Pin vorbei: {:?}", base[1]);
    assert_eq!(strong[1].leaked, 0, "{:?}", strong[1]);
    assert!(base[2].leaked > strong[2].leaked, "Lead/Ceramic: {:?} vs {:?}", base[2], strong[2]);
}

#[test]
fn ceramic_takes_several_hits_before_it_pops() {
    // Ceramic (10 HP) ist ~2,4 s in Reichweite, der Pin schießt alle 0,5 s: rund 5 Treffer
    let run = |damage_upgrade: bool| {
        let mut d = data();
        d.rounds[0] = vec![btd_data::v1::SpawnGroup {
            kind: 3,
            flags: Default::default(),
            count: 1,
            start: 0.0,
            end: 0.0,
        }];
        let mut g = Game::new(Arc::new(d)).unwrap();
        g.cash = Money::from_dollars(1_000);
        let id = g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap().unwrap();
        if damage_upgrade {
            g.apply(Action::Upgrade { tower: id, path: 1 }).unwrap(); // 2 Schaden
        }
        g.play_round()
    };
    let weak = run(false);
    assert_eq!((weak.pops, weak.leaked), (0, 15), "5 Treffer à 1 reichen nicht: {weak:?}");
    let strong = run(true);
    assert!(strong.pops >= 1, "5 Treffer à 2 = 10 HP: {strong:?}");
    assert!(strong.leaked < 15, "{strong:?}");
}

#[test]
fn deterministic_and_reset_clean() {
    let run = || {
        play_three_rounds(|g| {
            g.cash = Money::from_dollars(2_000);
            g.apply(Action::Place { kind: PIN, x: -10.0, y: 15.0 }).unwrap();
            g.apply(Action::Place { kind: BOMB, x: 10.0, y: -15.0 }).unwrap();
        })
    };
    let (a, ra) = run();
    let (b, rb) = run();
    assert_eq!(ra, rb);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn splash_without_camo_detection_never_fires_at_pure_camo_group() {
    let mut d = data();
    d.rounds[0] = vec![btd_data::v1::SpawnGroup {
        kind: 0,
        flags: btd_data::v1::Flags(btd_data::v1::Flags::CAMO),
        count: 5,
        start: 0.0,
        end: 1.0,
    }];
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind: BOMB, x: 0.0, y: -15.0 }).unwrap();
    let r = g.play_round();
    assert_eq!((r.pops, r.leaked), (0, 5));
}
