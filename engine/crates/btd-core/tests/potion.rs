//! Tränke und Zeit-Effekte (Alchemist, docs/design-towers-step3.md §2.4) auf der Mini-Fixture mit einem
//! zusätzlichen Trank-Turm. Läuft in der CI ohne Spiel-Dump.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, Game, Money};
use btd_data::v1::GameV1;
use serde_json::{json, Value};

const PIN: u8 = 0;
const GLUE: u8 = 7;
const ALCH: u8 = 9;
const ALCH2: u8 = 10;

/// Effekt mit Schaden +1 für 100 Schüsse und 100 s (Gruppe "Brew", Rang 0); `f` ändert ihn.
fn effect(f: impl FnOnce(&mut Value)) -> Value {
    let mut e = json!({"group": "Brew", "seconds": 100.0, "shots": 100, "damage": 1.0});
    f(&mut e);
    e
}

/// Mini-Fixture, erste Runde = ein Ceramic (10 HP); zwei Trank-Türme `alch`/`alch2` mit den Effekten.
fn data(e1: Value, e2: Value) -> GameV1 {
    data_cd(e1, e2, 0.2)
}

/// Wie `data`, mit Cooldown der Tränke.
fn data_cd(e1: Value, e2: Value, cooldown: f64) -> GameV1 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["rounds"][0] = json!([{"bloon": "ceramic", "count": 1, "start": 0.0, "end": 0.0}]);
    for (name, e) in [("alch", e1), ("alch2", e2)] {
        v["towers"].as_array_mut().unwrap().push(json!({
            "name": name, "cost": 100, "upgrade_costs": [[], [], []],
            "variants": [{"tiers": [0, 0, 0], "range": 100.0, "footprint": 5.0, "attacks": [{
                "tag": "potion", "targeting": "friendly", "cooldown": cooldown, "damage": 0.0, "pierce": 1,
                "delivery": {"kind": "potion", "effect": e}}]}]
        }));
    }
    GameV1::from_json(&v.to_string()).unwrap()
}

/// Spielt Runde 1 mit einem Pin bei (0, 15) und den angegebenen Trank-Türmen; gibt die Pops zurück.
fn pops(d: GameV1, alchs: &[(u8, f64, f64)]) -> u32 {
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(10_000);
    g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
    for &(kind, x, y) in alchs {
        g.apply(Action::Place { kind, x, y }).unwrap();
    }
    g.play_round().pops
}

fn base() -> Value {
    effect(|_| {})
}

#[test]
fn brew_damage_lets_the_pin_pop_a_ceramic() {
    // ohne Trank: 1 Schaden je Treffer reicht nicht für 10 HP, mit +1 schon
    assert_eq!(pops(data(base(), base()), &[]), 0);
    assert!(pops(data(base(), base()), &[(ALCH, -30.0, 30.0)]) > 0);
    // nur ein Schuss gebufft (danach gesperrt): zu wenig
    let one = effect(|e| (e["shots"], e["block"]) = (json!(1), json!(100.0)));
    assert_eq!(pops(data(one, base()), &[(ALCH, -30.0, 30.0)]), 0);
}

#[test]
fn same_group_does_not_stack_but_a_higher_rank_replaces() {
    let single = pops(data(base(), base()), &[(ALCH, -30.0, 30.0)]);
    // zwei Alchemisten derselben Gruppe und desselben Rangs: wirkt einmal
    assert_eq!(pops(data(base(), base()), &[(ALCH, -30.0, 30.0), (ALCH2, -10.0, 30.0)]), single);
    // Rang 1 mit +9 ersetzt Rang 0 mit +1: ein Treffer platzt den Ceramic sofort
    let strong = effect(|e| (e["rank"], e["damage"]) = (json!(1), json!(9.0)));
    let strong_alone = pops(data(base(), strong.clone()), &[(ALCH2, -10.0, 30.0)]);
    assert!(strong_alone > single);
    assert_eq!(pops(data(base(), strong), &[(ALCH, -30.0, 30.0), (ALCH2, -10.0, 30.0)]), strong_alone);
    // verschiedene Gruppen addieren sich
    let other = effect(|e| e["group"] = json!("Dip"));
    assert!(pops(data(base(), other), &[(ALCH, -30.0, 30.0), (ALCH2, -10.0, 30.0)]) > single);
}

#[test]
fn bonus_damage_only_for_hits_with_damage() {
    // Dip-artiger Bonus (+10 gegen Ceramic) auf einem Glue Gunner (Schaden 0): kein Schaden, keine Pops
    let dip =
        effect(|e| (e["damage"], e["damage_vs"]) = (json!(0.0), json!([{"tag": "Ceramic", "add": 10.0}])));
    let run = |kind: u8| {
        let mut g = Game::new(Arc::new(data(dip.clone(), base()))).unwrap();
        g.cash = Money::from_dollars(10_000);
        g.apply(Action::Place { kind, x: 0.0, y: 15.0 }).unwrap();
        g.apply(Action::Place { kind: ALCH, x: -30.0, y: 30.0 }).unwrap();
        g.play_round().pops
    };
    assert_eq!(run(GLUE), 0);
    assert!(run(PIN) > 0, "der Pin (Schaden 1) bekommt den Bonus");
}

#[test]
fn excluded_towers_and_blocks() {
    let no_pin = effect(|e| e["exclude"] = json!(["pin"]));
    assert_eq!(pops(data(no_pin, base()), &[(ALCH, -30.0, 30.0)]), 0);
    // kurzer Effekt (0,05 s) mit langer Sperre: abgelaufen, bevor der Ceramic in Reichweite ist; ohne Sperre
    // wird er laufend neu vergeben
    // (+9 Schaden: ein gebuffter Treffer platzt den Ceramic)
    let short = |block: f64| {
        effect(|e| (e["seconds"], e["block"], e["damage"]) = (json!(0.05), json!(block), json!(9.0)))
    };
    assert_eq!(pops(data_cd(short(100.0), base(), 0.01), &[(ALCH, -30.0, 30.0)]), 0);
    assert!(pops(data_cd(short(0.0), base(), 0.01), &[(ALCH, -30.0, 30.0)]) > 0);
}

#[test]
fn potion_goes_to_the_tower_with_the_highest_spent() {
    // zweiter Pin weit weg von der Strecke; bekommt er den (einzigen) Trank, bleibt der erste ohne
    let d = data_cd(base(), base(), 100.0);
    let run = |far_spent: i64| {
        let mut g = Game::new(Arc::new(d.clone())).unwrap();
        g.cash = Money::from_dollars(10_000);
        g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
        let far = g.apply(Action::Place { kind: PIN, x: 0.0, y: -35.0 }).unwrap().unwrap();
        g.towers.iter_mut().find(|t| t.id == far).unwrap().spent = Money::from_dollars(far_spent);
        g.apply(Action::Place { kind: ALCH, x: -30.0, y: 30.0 }).unwrap();
        g.play_round().pops
    };
    assert_eq!(run(1000), 0, "der teurere Pin (abseits) bekommt den Trank");
    assert!(run(1) > 0, "der teurere Pin an der Strecke bekommt ihn");
}

#[test]
fn effects_end_with_the_round_and_change_nothing_between_rounds() {
    let d = data(
        effect(|e| (e["rate_frac"], e["range_frac"], e["pierce"]) = (json!(1.0), json!(0.5), json!(2))),
        base(),
    );
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(10_000);
    g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
    g.apply(Action::Place { kind: ALCH, x: -30.0, y: 30.0 }).unwrap();
    let before = g.towers[0].attacks.clone();
    let hash = g.state_hash();
    // der Trank wirkt in der Runde (mehr Pops als ohne) …
    assert!(g.clone().play_round().pops > pops(data(base(), base()), &[]));
    // … und ist danach weg
    g.play_round();
    assert_eq!(g.towers[0].attacks[0].cooldown, before[0].cooldown);
    assert_eq!(g.towers[0].attacks[0].range2, before[0].range2);
    assert_eq!(g.towers[0].attacks[0].damage, before[0].damage);
    assert!(g.towers.iter().all(|t| t.timed.is_empty() && t.blocked.is_empty()));
    assert_ne!(g.state_hash(), hash, "Runde gespielt (Cash/Runde ändern sich)");
}

#[test]
fn invalid_potions_are_refused() {
    let load = |delivery: Value, targeting: &str| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let a = &mut v["towers"][0]["variants"][0]["attacks"][0];
        (a["delivery"], a["targeting"], a["damage"]) = (delivery, json!(targeting), json!(0.0));
        GameV1::from_json(&v.to_string())
    };
    let potion = |e: Value| json!({"kind": "potion", "effect": e});
    assert!(load(potion(base()), "friendly").is_ok());
    assert!(load(potion(base()), "first").is_err(), "Trank nur mit friendly");
    assert!(load(json!({"kind": "hitscan"}), "friendly").is_err(), "friendly nur mit Trank");
    assert!(load(potion(effect(|e| e["shots"] = json!(0))), "friendly").is_err());
    assert!(load(potion(effect(|e| e["group"] = json!(""))), "friendly").is_err());
    assert!(load(potion(effect(|e| e["exclude"] = json!(["castle"]))), "friendly").is_err());
    assert!(load(
        potion(effect(|e| e["damage_vs"] = json!([{"tag": "Ceramic", "add": 1.0, "all": ["Nope"]}]))),
        "friendly"
    )
    .is_err());
    // Bonus mit "alle Tags": Lead + Fortified
    assert!(load(
        potion(effect(|e| e["damage_vs"] = json!([{"tag": "Ceramic", "add": 1.0, "all": ["Lead"]}]))),
        "friendly"
    )
    .is_ok());
}
