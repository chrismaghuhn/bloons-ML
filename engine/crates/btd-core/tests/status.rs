//! Erweiterter Schaden über Zeit (Immunität, nur bei Schaden, Tag-Ausschluss), Eigenschaften entfernen und
//! Verlangsamen der Kinder beim Platzen (docs/design-towers-step3.md, Teil 2b) auf der Mini-Fixture.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, Game, Money, RoundReport};
use btd_data::v1::GameV1;
use serde_json::{json, Value};

const T1: u8 = 9;
const T2: u8 = 10;

/// Mini-Fixture, erste Runde = `round`, dazu die Türme `t1`/`t2` mit je einem Hitscan-Angriff (Pierce 1,
/// Reichweite 30, Cooldown 0,1): Schaden `damage` und Extras `extras`.
fn data(round: Value, towers: &[(f64, bool, Value)]) -> GameV1 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["rounds"][0] = round;
    // Camo-Red als eigener Typ wie in den Spiel-Daten, mit Verweis auf den Typ ohne Camo
    v["bloons"].as_array_mut().unwrap().push(json!({
        "name": "redcamo", "hp": 1.0, "speed": 25.0, "leak": 1, "camo": true, "radius": 4.0,
        "tags": ["Red", "Camo"], "decamo": "red"
    }));
    for (k, (damage, camo, extras)) in towers.iter().enumerate() {
        v["towers"].as_array_mut().unwrap().push(json!({
            "name": format!("t{}", k + 1), "cost": 100, "upgrade_costs": [[], [], []],
            "variants": [{"tiers": [0, 0, 0], "range": 30.0, "footprint": 5.0, "attacks": [{
                "tag": "a", "cooldown": 0.1, "damage": damage, "pierce": 1, "camo": camo,
                "delivery": {"kind": "hitscan"}, "extras": extras}]}]
        }));
    }
    GameV1::from_json(&v.to_string()).unwrap()
}

fn play(d: GameV1, kinds: &[u8]) -> RoundReport {
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(10_000);
    for (k, &kind) in kinds.iter().enumerate() {
        g.apply(Action::Place { kind, x: -20.0 + 40.0 * k as f64, y: 15.0 }).unwrap();
    }
    g.play_round()
}

fn one(bloon: &str) -> Value {
    json!([{"bloon": bloon, "count": 1, "start": 0.0, "end": 0.0}])
}

/// Brennen ohne direkten Schaden: 1 Schaden je 0,1 s für 10 s.
fn burn(f: impl FnOnce(&mut Value)) -> Value {
    let mut d = json!({"damage": 1.0, "interval": 0.1, "seconds": 10.0, "layers": 99});
    f(&mut d);
    json!({"dot": d})
}

#[test]
fn dot_immunity_tag_exclusion_and_only_if_damaged() {
    let pops = |round: Value, extras: Value| play(data(round, &[(0.0, false, extras)]), &[T1]).pops;
    // ohne Einschränkung brennt der Red ab
    assert!(pops(one("red"), burn(|_| {})) > 0);
    // nur bei Schaden: der Angriff macht keinen, also kein Brennen
    assert_eq!(pops(one("red"), burn(|d| d["only_if_damaged"] = json!(true))), 0);
    // Tag ausgeschlossen
    assert_eq!(pops(one("red"), burn(|d| d["exclude_tags"] = json!(["Red"]))), 0);
    // Immunität: das Brennen trifft Lead nicht (der Angriff selbst macht keinen Schaden)
    assert!(pops(one("lead"), burn(|_| {})) > 0);
    assert_eq!(pops(one("lead"), burn(|d| d["cannot_damage"] = json!(["lead"]))), 0);
}

#[test]
fn cleanse_removes_camo_so_others_can_target() {
    let camo_red = one("redcamo");
    // t1 ohne Camo-Erkennung platzt den Camo-Red nicht
    let pin = (1.0, false, json!({}));
    assert_eq!(play(data(camo_red.clone(), std::slice::from_ref(&pin)), &[T1]).pops, 0);
    // t2 (Camo, kein Schaden) entfernt Camo; danach kann t1 ihn platzen (t2 steht vor t1 an der Strecke)
    let flare = (0.0, true, json!({"cleanse": {"camo": true}}));
    let r = play(data(camo_red.clone(), &[flare.clone(), pin.clone()]), &[T1, T2]);
    assert_eq!(r.pops, 1);
    // nur bei Schaden: ohne Schaden bleibt Camo
    let only = (0.0, true, json!({"cleanse": {"camo": true, "only_if_damaged": true}}));
    assert_eq!(play(data(camo_red.clone(), &[only, pin.clone()]), &[T1, T2]).pops, 0);
    // ausgeschlossener Tag
    let not_red = (0.0, true, json!({"cleanse": {"camo": true, "exclude_tags": ["Red"]}}));
    assert_eq!(play(data(camo_red.clone(), &[not_red, pin.clone()]), &[T1, T2]).pops, 0);
    // auch ein Camo-Flag der Instanz (Rundendaten) wird entfernt
    let flagged = json!([{"bloon": "red", "flags": ["camo"], "count": 1, "start": 0.0, "end": 0.0}]);
    assert_eq!(play(data(flagged, &[flare, pin]), &[T1, T2]).pops, 1);
}

#[test]
fn mib_lifts_the_dot_tag_exclusion() {
    // Brennen nicht auf Reds; eine Village mit MIB in Reichweite hebt das auf
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["rounds"][0] = one("red");
    let burn = burn(|d| d["exclude_tags"] = json!(["Red"]));
    v["towers"].as_array_mut().unwrap().push(json!({
        "name": "t1", "cost": 100, "upgrade_costs": [[], [], []],
        "variants": [{"tiers": [0, 0, 0], "range": 30.0, "footprint": 5.0, "attacks": [{
            "tag": "a", "cooldown": 0.1, "damage": 0.0, "pierce": 1, "delivery": {"kind": "hitscan"}, "extras": burn}]}]
    }));
    v["towers"].as_array_mut().unwrap().push(json!({
        "name": "mib", "set": "support", "cost": 100, "upgrade_costs": [[], [], []],
        "variants": [{"tiers": [0, 0, 0], "range": 50.0, "footprint": 5.0, "attacks": [],
                      "supports": [{"kind": "damage_all", "group": "Village:DamageType"}]}]
    }));
    let d = GameV1::from_json(&v.to_string()).unwrap();
    let run = |with_mib: bool| {
        let mut g = Game::new(Arc::new(d.clone())).unwrap();
        g.cash = Money::from_dollars(10_000);
        g.apply(Action::Place { kind: T1, x: 0.0, y: 15.0 }).unwrap();
        if with_mib {
            g.apply(Action::Place { kind: T2, x: 0.0, y: 30.0 }).unwrap();
        }
        g.play_round().pops
    };
    assert_eq!((run(false), run(true) > 0), (0, true));
}

#[test]
fn pop_slow_stops_the_children() {
    // t1 platzt nur Blues (Reds ausgeschlossen); mit Pin steht der Red-Kind 1 s, die Runde dauert länger
    let popper = |extras: Value| (1.0, false, extras);
    let filter = json!({"exclude_tags": ["Red"]});
    let plain = play(data(one("blue"), &[popper(json!({"filter": filter.clone()}))]), &[T1]);
    let pin = json!({"filter": filter, "pop_slow": {"id": "Stun:Weak", "multiplier": 0.0, "seconds": 1.0,
                                                    "level": 0, "layers": 1}});
    let pinned = play(data(one("blue"), &[popper(pin)]), &[T1]);
    assert_eq!((plain.pops, pinned.pops, plain.leaked, pinned.leaked), (1, 1, 1, 1));
    let extra = pinned.seconds - plain.seconds;
    assert!((0.9..1.1).contains(&extra), "der Red stand etwa 1 s: {extra}");
}

#[test]
fn invalid_status_data_is_refused() {
    let load = |extras: Value| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v["towers"][0]["variants"][0]["attacks"][0]["extras"] = extras;
        GameV1::from_json(&v.to_string())
    };
    assert!(load(burn(|d| d["cannot_damage"] = json!(["purple"]))).is_ok());
    assert!(load(burn(|d| d["cannot_damage"] = json!(["plaid"]))).is_err());
    assert!(load(burn(|d| d["exclude_tags"] = json!(["Nope"]))).is_err());
    assert!(load(json!({"cleanse": {"camo": true, "exclude_tags": ["Nope"]}})).is_err());
    assert!(load(json!({"cleanse": {"lead": true}})).is_err(), "Lead entfernen gibt es nicht");
    assert!(load(json!({"cleanse": {"regrow": true}})).is_err(), "Regrow entfernen gibt es (noch) nicht");
    // decamo nur von einem Camo- auf einen Nicht-Camo-Typ
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["bloons"][1]["decamo"] = json!("red");
    assert!(GameV1::from_json(&v.to_string()).is_err(), "Blue ist kein Camo-Typ");
    let glue_pin = json!({"pop_slow": {"multiplier": 0.0, "seconds": 1.0, "level": 1, "layers": 1}});
    assert!(load(glue_pin).is_err(), "Pin ohne Glue-Stufe");
}

/// Regrow und Camo entfernen: Ein Kind eines Camo-Regrow-Bloons, dem die Camo genommen wird, wächst zum
/// Vorfahren ohne Camo nach (Spiel: RedRegrow wächst zu BlueRegrow), nicht zurück zum Camo-Typ.
#[test]
fn cleansed_regrow_bloons_grow_back_without_camo() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["rounds"][0] = one("bluerc");
    let bloon = |name: &str, children: Value, camo: bool, decamo: Option<&str>| {
        let mut b = json!({"name": name, "hp": 1.0, "speed": 25.0, "leak": 1, "children": children, "camo": camo,
                           "regrow": true, "grow_rate": 1.0, "radius": 4.0, "tags": ["Regrow"]});
        if let Some(d) = decamo {
            b["decamo"] = json!(d);
        }
        b
    };
    for b in [
        bloon("redr", json!([]), false, None),
        bloon("bluer", json!([["redr", 1]]), false, None),
        bloon("redrc", json!([]), true, Some("redr")),
        bloon("bluerc", json!([["redrc", 1]]), true, Some("bluer")),
    ] {
        v["bloons"].as_array_mut().unwrap().push(b);
    }
    let tower = |name: &str, damage: f64, camo: bool, cooldown: f64, extras: Value| {
        json!({"name": name, "cost": 100, "upgrade_costs": [[], [], []],
               "variants": [{"tiers": [0, 0, 0], "range": 30.0, "footprint": 5.0, "attacks": [{
                   "tag": "a", "cooldown": cooldown, "damage": damage, "pierce": 1, "camo": camo,
                   "delivery": {"kind": "hitscan"}, "extras": extras}]}]})
    };
    // a: platzt den Camo-Blue einmal (Kind: Camo-Red mit Vorfahre Camo-Blue); b: entfernt einmal die Camo;
    // c (ohne Camo-Erkennung, weiter hinten): sieht den nachgewachsenen Bloon nur, wenn er keine Camo hat
    let towers = v["towers"].as_array_mut().unwrap();
    towers.push(tower("a", 1.0, true, 100.0, json!({})));
    towers.push(tower("b", 0.0, true, 100.0, json!({"cleanse": {"camo": true}}))); // nur einmal
    towers.push(tower("c", 1.0, false, 0.1, json!({})));
    let d = GameV1::from_json(&v.to_string()).unwrap();
    let (a, b, c) = (T1, T2, T2 + 1);
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(10_000);
    g.apply(Action::Place { kind: a, x: -45.0, y: 15.0 }).unwrap();
    g.apply(Action::Place { kind: b, x: -15.0, y: 15.0 }).unwrap();
    g.apply(Action::Place { kind: c, x: 40.0, y: 15.0 }).unwrap();
    let r = g.play_round();
    assert_eq!(r.leaked, 0, "c platzt den nachgewachsenen Blue ohne Camo und sein Kind");
    assert!(r.pops >= 3, "{}", r.pops);
}
