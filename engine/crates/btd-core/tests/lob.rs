//! Wurf und fester Zielpunkt (Mortar, docs/design-towers-step3.md §2.1/§3) auf der Mini-Fixture mit
//! einem zusätzlichen Wurf-Turm. Läuft in der CI ohne Spiel-Dump.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, Game, Money};
use btd_data::v1::GameV1;
use serde_json::json;

const LOB: u8 = 9;

/// Mini-Fixture plus `lob` (Wurf mit Explosion) mit den Angaben `delivery` und `targeting`; die erste
/// Runde enthält `reds` Reds.
fn data(delivery: serde_json::Value, targeting: &str, reds: u32, cooldown: f64) -> GameV1 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["rounds"][0] = json!([{"bloon": "red", "count": reds, "start": 0.0, "end": 0.5 * reds as f64}]);
    v["towers"].as_array_mut().unwrap().push(json!({
        "name": "lob", "cost": 100, "upgrade_costs": [[], [], []],
        "variants": [{"tiers": [0, 0, 0], "range": 1000.0, "footprint": 5.0, "attacks": [{
            "tag": "lob", "targeting": targeting, "cooldown": cooldown, "damage": 0.0, "pierce": 1,
            "delivery": delivery}]}]
    }));
    GameV1::from_json(&v.to_string()).unwrap()
}

fn explosion(radius: f64) -> serde_json::Value {
    json!([{"kind": "explosion", "radius": radius, "pierce": 100, "damage": 1.0}])
}

fn pops(d: GameV1, x: f64, y: f64, seed: Option<u64>) -> u32 {
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.set_seed(seed);
    g.cash = Money::from_dollars(10_000);
    g.apply(Action::Place { kind: LOB, x, y }).unwrap();
    g.play_round().pops
}

#[test]
fn lob_explodes_where_the_target_was_after_the_flight_time() {
    // ein Red (Tempo 25), ein Wurf: trifft nach einem Tick, nach 1 s ist der Red 25 Einheiten weiter
    let lob = |time: f64| json!({"kind": "lob", "time": time, "on_arrive": explosion(2.0)});
    assert_eq!(pops(data(lob(1.0 / 60.0), "first", 1, 100.0), 0.0, 20.0, None), 1);
    assert_eq!(pops(data(lob(1.0), "first", 1, 100.0), 0.0, 20.0, None), 0);
    // Tempo statt Zeit: Flugzeit = Abstand / Tempo (20 Einheiten bei Tempo 10 = 2 s)
    let fast = json!({"kind": "lob", "speed": 1e6, "on_arrive": explosion(2.0)});
    let slow = json!({"kind": "lob", "speed": 10.0, "on_arrive": explosion(2.0)});
    assert_eq!(pops(data(fast, "first", 1, 100.0), 0.0, 20.0, None), 1);
    assert_eq!(pops(data(slow, "first", 1, 100.0), 0.0, 20.0, None), 0);
}

#[test]
fn track_best_aims_at_the_fixed_point_from_anywhere() {
    // gerade Strecke über die ganze Karte: überall gleich dicht, der späteste Punkt in den bounds gewinnt
    let d = data(json!({"kind": "lob", "time": 0.5, "on_arrive": explosion(10.0)}), "track_best", 10, 0.1);
    let g = Game::new(Arc::new(d.clone())).unwrap();
    let points = g.track_best_points();
    assert_eq!(points.len(), 1);
    assert_eq!((points[0].0, points[0].1), (10.0, (50.0, 0.0)));
    // egal wo der Turm steht: alle Reds laufen durch den Punkt und platzen dort
    assert_eq!(pops(d.clone(), -40.0, 30.0, None), 10);
    assert_eq!(pops(d, 40.0, -30.0, None), 10);
}

#[test]
fn spread_only_with_randomness() {
    let lob =
        |spread: f64| json!({"kind": "lob", "time": 0.5, "spread": spread, "on_arrive": explosion(10.0)});
    let base = pops(data(lob(0.0), "track_best", 10, 0.1), -40.0, 30.0, None);
    // ohne Zufall: genau der Punkt, auch mit Streuung
    assert_eq!(pops(data(lob(500.0), "track_best", 10, 0.1), -40.0, 30.0, None), base);
    // mit Zufall: große Streuung verfehlt fast immer, deterministisch je Seed
    let a = pops(data(lob(500.0), "track_best", 10, 0.1), -40.0, 30.0, Some(7));
    let b = pops(data(lob(500.0), "track_best", 10, 0.1), -40.0, 30.0, Some(7));
    assert!(a < base, "{a} < {base}");
    assert_eq!(a, b);
}

#[test]
fn invalid_lobs_are_refused() {
    let load = |delivery: serde_json::Value, targeting: &str, damage: f64| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
        let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let a = &mut v["towers"][0]["variants"][0]["attacks"][0];
        (a["delivery"], a["targeting"], a["damage"]) = (delivery, json!(targeting), json!(damage));
        GameV1::from_json(&v.to_string())
    };
    let ok = || json!({"kind": "lob", "time": 0.5, "on_arrive": explosion(10.0)});
    assert!(load(ok(), "track_best", 0.0).is_ok());
    assert!(load(ok(), "first", 1.0).is_err(), "Schaden steckt in den Explosionen, nicht im Angriff");
    let both = json!({"kind": "lob", "time": 0.5, "speed": 10.0, "on_arrive": explosion(10.0)});
    assert!(load(both, "first", 0.0).is_err(), "Zeit und Tempo zugleich");
    let none = json!({"kind": "lob", "on_arrive": explosion(10.0)});
    assert!(load(none, "first", 0.0).is_err(), "weder Zeit noch Tempo");
    let empty = json!({"kind": "lob", "time": 0.5, "on_arrive": []});
    assert!(load(empty, "first", 0.0).is_err(), "ohne Wirkung bei Ankunft");
    let shards = json!({"kind": "lob", "time": 0.5, "on_arrive": [{"kind": "shards", "emission": {"kind": "single"},
        "speed": 100.0, "lifespan": 0.5, "radius": 2.0, "pierce": 1, "damage": 1.0}]});
    assert!(load(shards, "first", 0.0).is_err(), "nur Explosionen bei Ankunft");
    assert!(load(json!({"kind": "hitscan"}), "track_best", 1.0).is_err(), "track_best nur mit Wurf");
}
