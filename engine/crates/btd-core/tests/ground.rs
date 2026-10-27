//! Bodenobjekte (Spike Factory, docs/design-towers-step3.md §2.3), Strecken-Ziel `track_spread` und Effekte
//! zu Rundenbeginn auf der Mini-Fixture. Läuft in der CI ohne Spiel-Dump.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, Game, Money};
use btd_data::v1::GameV1;
use serde_json::{json, Value};

const PIN: u8 = 0;
const SPIKE: u8 = 9;

fn mini() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Liegender Spike-Stapel: Pierce 3, Radius 6, Schaden 1, 10 s; `f` ändert ihn.
fn ground(f: impl FnOnce(&mut Value)) -> Value {
    let mut g = json!({"lifespan": 10.0, "radius": 6.0, "pierce": 3, "damage": 1.0, "camo": true});
    f(&mut g);
    g
}

/// Mini-Fixture mit `spike` (Wurf in 0,5 s auf die Strecke in Reichweite 30, Cooldown `cooldown`); erste
/// Runde = `count` Bloons `bloon` über `secs` Sekunden.
fn data(g: Value, cooldown: f64, bloon: &str, count: u32, secs: f64) -> GameV1 {
    GameV1::from_json(&data_json(g, cooldown, bloon, count, secs)).unwrap()
}

fn data_json(g: Value, cooldown: f64, bloon: &str, count: u32, secs: f64) -> String {
    let mut v = mini();
    v["rounds"][0] = json!([{"bloon": bloon, "count": count, "start": 0.0, "end": secs}]);
    v["towers"].as_array_mut().unwrap().push(json!({
        "name": "spike", "cost": 100, "upgrade_costs": [[], [], []],
        "variants": [{"tiers": [0, 0, 0], "range": 30.0, "footprint": 5.0, "attacks": [{
            "tag": "spike", "targeting": "track_spread", "cooldown": cooldown, "damage": 0.0, "pierce": 1,
            "delivery": {"kind": "lob", "time": 0.5, "ground": g}}]}]
    }));
    v.to_string()
}

fn play(d: GameV1, kind: u8, x: f64, y: f64) -> btd_core::RoundReport {
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(10_000);
    g.apply(Action::Place { kind, x, y }).unwrap();
    g.play_round()
}

#[test]
fn a_spike_pile_pops_until_its_pierce_is_used() {
    // ein Stapel (Cooldown 100), landet vor den Reds (Strecke ab x = -60, Tempo 25) am Anfang der Strecke in
    // Reichweite, platzt 3 von 10
    assert_eq!(play(data(ground(|_| {}), 100.0, "red", 10, 5.0), SPIKE, 0.0, 20.0).pops, 3);
    // abgelaufen, bevor die Reds ankommen
    let short = ground(|g| g["lifespan"] = json!(0.5));
    assert_eq!(play(data(short, 100.0, "red", 10, 5.0), SPIKE, 0.0, 20.0).pops, 0);
    // keine Strecke in Reichweite: kein Wurf
    assert_eq!(play(data(ground(|_| {}), 100.0, "red", 10, 5.0), SPIKE, 0.0, -35.0).pops, 0);
}

#[test]
fn piles_do_not_keep_the_round_open() {
    let long = ground(|g| (g["lifespan"], g["pierce"]) = (json!(100.0), json!(1000)));
    let r = play(data(long, 0.5, "red", 10, 5.0), SPIKE, 0.0, 20.0);
    assert_eq!((r.pops, r.leaked), (10, 0));
    assert!(r.seconds < 10.0, "Runde endet mit dem letzten Bloon, nicht mit den Spikes: {}", r.seconds);
}

#[test]
fn collision_interval_lets_fast_bloons_through() {
    // Blues (Tempo 35) brauchen ~0,6 s durch einen Stapel; mit Prüfung nur alle 2 s rutschen welche durch
    let pile = |every: f64| ground(|g| (g["pierce"], g["collide_every"]) = (json!(1000), json!(every)));
    let every_tick = play(data(pile(0.0), 100.0, "blue", 10, 5.0), SPIKE, 0.0, 20.0).pops;
    let rarely = play(data(pile(2.0), 100.0, "blue", 10, 5.0), SPIKE, 0.0, 20.0).pops;
    assert!(rarely < every_tick, "{rarely} < {every_tick}");
    // Treffer-Reset: derselbe Bloon wird erneut getroffen (Blue = 2 Schichten, ein Stapel mit Schaden 1)
    let reset = ground(|g| (g["pierce"], g["clear_hits"]) = (json!(1000), json!(0.1)));
    let once = play(data(pile(0.0), 100.0, "blue", 1, 0.0), SPIKE, 0.0, 20.0).pops;
    let again = play(data(reset, 100.0, "blue", 1, 0.0), SPIKE, 0.0, 20.0).pops;
    assert_eq!((once, again), (1, 2));
}

#[test]
fn track_spread_spreads_and_is_deterministic() {
    // viele Würfe mit Pierce 1: verteilt über die Strecke in Reichweite platzen sie viele Reds
    let pile = ground(|g| g["pierce"] = json!(1));
    let d = data(pile, 0.2, "red", 20, 10.0);
    let a = play(d.clone(), SPIKE, 0.0, 20.0);
    let b = play(d.clone(), SPIKE, 0.0, 20.0);
    assert_eq!(a, b);
    assert!(a.pops >= 10, "{}", a.pops);
    // mit Zufall: deterministisch je Seed
    let seeded = |seed| {
        let mut g = Game::new(Arc::new(d.clone())).unwrap();
        g.set_seed(Some(seed));
        g.cash = Money::from_dollars(10_000);
        g.apply(Action::Place { kind: SPIKE, x: 0.0, y: 20.0 }).unwrap();
        g.play_round()
    };
    assert_eq!(seeded(3), seeded(3));
    assert!((0..8).any(|s| seeded(s) != seeded(3)), "andere Seeds, andere Punkte");
}

#[test]
fn buffs_and_exhaust_effects_reach_the_pile() {
    // Pierce-Zuschlag (hier als Effekt zu Rundenbeginn am Spike-Turm selbst) gilt für den liegenden Stapel
    let mut v: Value = serde_json::from_str(&data_json(ground(|_| {}), 100.0, "red", 10, 5.0)).unwrap();
    let base = play(GameV1::from_json(&v.to_string()).unwrap(), SPIKE, 0.0, 20.0).pops;
    let n = v["towers"].as_array().unwrap().len() - 1;
    v["towers"][n]["variants"][0]["round_start"] = json!({"group": "Start", "pierce": 4});
    let buffed = play(GameV1::from_json(&v.to_string()).unwrap(), SPIKE, 0.0, 20.0).pops;
    assert_eq!((base, buffed), (3, 7));
    // Explosion, sobald der Pierce verbraucht ist (Spiked Mines): platzt den Rest der Gruppe
    let mine = ground(|g| {
        g["on_exhaust"] =
            json!([{"kind": "explosion", "radius": 100.0, "pierce": 100, "damage": 1.0, "camo": true}])
    });
    assert_eq!(play(data(mine, 100.0, "red", 10, 0.5), SPIKE, 0.0, 20.0).pops, 10);
}

#[test]
fn round_start_effect_speeds_up_the_first_seconds() {
    // Pin mit 4-fachem Tempo für die ganze Runde: platzt einen Ceramic, der ihm sonst entkommt
    let with = |start: Option<Value>| {
        let mut v = mini();
        v["rounds"][0] = json!([{"bloon": "ceramic", "count": 1, "start": 0.0, "end": 0.0}]);
        if let Some(e) = start {
            v["towers"][0]["variants"][0]["round_start"] = e;
        }
        let mut g = Game::new(Arc::new(GameV1::from_json(&v.to_string()).unwrap())).unwrap();
        g.cash = Money::from_dollars(10_000);
        g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
        let r = g.play_round();
        assert!(g.towers[0].timed.is_empty(), "nach der Runde gelöscht");
        r.pops
    };
    assert_eq!(with(None), 0);
    assert!(with(Some(json!({"group": "Start", "seconds": 100.0, "rate_frac": 3.0}))) > 0);
    // nur 0,1 s zu Beginn (bevor der Ceramic in Reichweite ist): ohne Wirkung
    assert_eq!(with(Some(json!({"group": "Start", "seconds": 0.1, "rate_frac": 3.0}))), 0);
}

#[test]
fn invalid_ground_data_is_refused() {
    let load = |delivery: Value, targeting: &str| {
        let mut v = mini();
        let a = &mut v["towers"][0]["variants"][0]["attacks"][0];
        (a["delivery"], a["targeting"], a["damage"]) = (delivery, json!(targeting), json!(0.0));
        GameV1::from_json(&v.to_string())
    };
    let lob = |g: Value| json!({"kind": "lob", "time": 0.5, "ground": g});
    assert!(load(lob(ground(|_| {})), "track_spread").is_ok());
    assert!(load(json!({"kind": "hitscan"}), "track_spread").is_err(), "track_spread nur mit Wurf");
    assert!(load(lob(ground(|g| g["pierce"] = json!(0))), "track_spread").is_err());
    assert!(load(lob(ground(|g| g["lifespan"] = json!(0.0))), "track_spread").is_err());
    assert!(load(json!({"kind": "lob", "time": 0.5}), "track_spread").is_err(), "weder Explosion noch Boden");
}
