//! Auren und Rabatt-Zonen (Village, docs/design-towers-step2.md §2.1/§2.2) auf der Mini-Fixture mit
//! einem zusätzlichen Support-Turm. Läuft in der CI ohne Spiel-Dump.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::tower::{AttackKind, ContactRt};
use btd_core::{Action, Buffs, Game, Money, Real};
use btd_data::v1::GameV1;
use serde_json::json;

const PIN: u8 = 0;
const BOMB: u8 = 1;
const DART: u8 = 2;
const BOMBP: u8 = 4;
const ICE: u8 = 6;
const VILLAGE: u8 = 9;

/// Mini-Fixture plus `village` (Pfade: oben Tempo/Camo/MIB, mitte stärkere Reichweite + Primary-
/// Training, unten Rabatt). Die Bomb (hitscan_splash) zählt als `military`, der Rest als `primary`.
fn data() -> GameV1 {
    data_with(|_| {})
}

/// Wie `data`, mit Änderungen am JSON (Turm-Index wie in den Konstanten).
fn data_with(f: impl FnOnce(&mut serde_json::Value)) -> GameV1 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for t in v["towers"].as_array_mut().unwrap() {
        t["set"] = json!(if t["name"] == "bomb" { "military" } else { "primary" });
    }
    let range = json!({"kind": "range", "group": "Range:Support", "add_frac": 0.1});
    let rate = json!({"kind": "rate", "group": "Village:Rate", "factor": 0.5});
    let camo = json!({"kind": "camo", "group": "Village:Visibility"});
    let mib = json!({"kind": "damage_all", "group": "Village:DamageType"});
    let major = json!({"fraction": 0.1, "tier_min": 0, "tier_cap": 3, "stack": "major", "stack_limit": 1});
    let minor = json!({"fraction": 0.05, "tier_min": 0, "tier_cap": 3, "stack": "minor", "stack_limit": 3});
    let variant = |tiers: [u8; 3], supports: serde_json::Value, discount: serde_json::Value| {
        json!({"tiers": tiers, "range": 30.0, "footprint": 5.0, "attacks": [], "supports": supports,
               "discount": discount})
    };
    v["towers"].as_array_mut().unwrap().push(json!({
        "name": "village", "set": "support", "cost": 500, "upgrade_costs": [[100, 100], [100], [100, 100]],
        "variants": [
            variant([0, 0, 0], json!([range]), json!([])),
            variant([1, 0, 0], json!([range, rate]), json!([])),
            variant([2, 0, 0], json!([range, rate, camo, mib]), json!([])),
            variant([0, 1, 0], json!([
                {"kind": "range", "group": "Range:Support", "add_frac": 0.2},
                {"kind": "range", "group": "Village:PrimaryTraining:Range", "sets": ["primary"], "add": 5.0},
                {"kind": "pierce", "group": "Village:PrimaryTraining:Pierce", "sets": ["primary"], "add": 1},
                {"kind": "projectile_speed", "group": "Village:PrimaryTraining:Speed", "sets": ["primary"],
                 "add_frac": 0.5}]), json!([])),
            variant([0, 0, 1], json!([range]), json!([major])),
            variant([0, 0, 2], json!([range]), json!([major, minor])),
        ]
    }));
    f(&mut v);
    GameV1::from_json(&v.to_string()).unwrap()
}

fn game() -> Game {
    game_of(data())
}

fn game_of(d: GameV1) -> Game {
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(100_000);
    g
}

fn place(g: &mut Game, kind: u8, x: f64, y: f64) -> u32 {
    g.apply(Action::Place { kind, x, y }).unwrap().unwrap()
}

fn upgrade(g: &mut Game, id: u32, path: u8, times: usize) {
    for _ in 0..times {
        g.apply(Action::Upgrade { tower: id, path }).unwrap();
    }
}

fn buffs(g: &Game, id: u32) -> Buffs {
    g.tower(id).unwrap().buffs
}

#[test]
fn aura_reaches_towers_in_range_and_disappears_when_sold() {
    let mut g = game();
    let pin = place(&mut g, PIN, 0.0, 15.0);
    let before = g.tower(pin).unwrap().clone();
    // außer Reichweite (30): keine Wirkung
    let far = place(&mut g, VILLAGE, -40.0, 14.0);
    assert!(!g.tower(far).unwrap().reaches(0.0, 15.0, 30.0));
    assert_eq!(buffs(&g, pin), Buffs::NONE);
    assert_eq!(g.tower(pin).unwrap().attacks, before.attacks);
    // in Reichweite: Reichweite +10 %
    assert_eq!(buffs(&g, far), Buffs::NONE, "die eigene Aura wirkt nicht auf die Village");
    let near = place(&mut g, VILLAGE, -20.0, 15.0);
    assert_eq!(buffs(&g, pin).range_frac, 0.1);
    let r = 30.0 * 1.1;
    assert_eq!(g.tower(pin).unwrap().attacks[0].range2, (r as Real) * (r as Real));
    assert_eq!(buffs(&g, far).range_frac, 0.1, "Villages buffen sich gegenseitig");
    // verkauft: wieder bitgleich wie ohne Village
    g.apply(Action::Sell { tower: near }).unwrap();
    assert_eq!(buffs(&g, pin), Buffs::NONE);
    assert_eq!(g.tower(pin).unwrap().attacks, before.attacks);
}

#[test]
fn same_group_does_not_stack_across_villages() {
    let mut g = game();
    let pin = place(&mut g, PIN, 0.0, 15.0);
    place(&mut g, VILLAGE, -20.0, 15.0);
    let v2 = place(&mut g, VILLAGE, 20.0, 15.0);
    assert_eq!(buffs(&g, pin).range_frac, 0.1, "zwei Villages, eine Gruppe: einmal +10 %");
    // die stärkere Aura derselben Gruppe gewinnt, andere Gruppen kommen dazu (nur Primary)
    upgrade(&mut g, v2, 1, 1);
    let b = buffs(&g, pin);
    assert_eq!((b.range_frac, b.range_add, b.pierce, b.speed_frac), (0.2, 5.0, 1, 0.5));
}

#[test]
fn primary_only_auras_skip_other_tower_sets() {
    let mut g = game();
    let dart = place(&mut g, DART, 0.0, 15.0);
    let bomb = place(&mut g, BOMB, 0.0, 28.0);
    let v = place(&mut g, VILLAGE, -20.0, 15.0);
    upgrade(&mut g, v, 1, 1);
    let dart_t = g.tower(dart).unwrap();
    assert_eq!(dart_t.attacks[0].pierce, 2 + 1);
    let AttackKind::Projectile(def) = &dart_t.attacks[0].kind else { panic!("Projektil") };
    assert_eq!(
        def.speed,
        (data().towers[DART as usize].variants[0].attacks[0].delivery_speed() * 1.5) as Real
    );
    let b = buffs(&g, bomb);
    assert_eq!((b.range_frac, b.range_add, b.pierce), (0.2, 0.0, 0), "Bomb ist military");
}

trait Speed {
    fn delivery_speed(&self) -> f64;
}

impl Speed for btd_data::v1::AttackV1 {
    fn delivery_speed(&self) -> f64 {
        match self.delivery {
            btd_data::v1::Delivery::Projectile { speed, .. } => speed,
            _ => panic!("kein Projektil"),
        }
    }
}

#[test]
fn rate_camo_and_mib_reach_contacts() {
    let mut g = game();
    let pin = place(&mut g, PIN, 0.0, 15.0);
    let bomb = place(&mut g, BOMBP, 0.0, 28.0);
    let v = place(&mut g, VILLAGE, -20.0, 15.0);
    upgrade(&mut g, v, 0, 2);
    let a = &g.tower(pin).unwrap().attacks[0];
    assert_eq!((a.cooldown, a.camo, a.cannot_damage.0), (0.25, true, 0));
    let AttackKind::Projectile(def) = &g.tower(bomb).unwrap().attacks[0].kind else { panic!("Projektil") };
    assert!(!def.contacts.is_empty());
    for c in &def.contacts {
        let ContactRt::Explosion { camo, cannot_damage, .. } = c else { panic!("Explosion") };
        assert!(*camo && cannot_damage.0 == 0, "Explosion erbt Camo und alle Schadensarten");
    }
}

#[test]
fn area_radius_follows_buffed_range() {
    let mut g = game();
    let ice = place(&mut g, ICE, 0.0, 15.0);
    let AttackKind::Area { radius } = g.tower(ice).unwrap().attacks[0].kind else { panic!("Fläche") };
    assert_eq!(radius, 20.0);
    place(&mut g, VILLAGE, -20.0, 15.0);
    let AttackKind::Area { radius } = g.tower(ice).unwrap().attacks[0].kind else { panic!("Fläche") };
    assert_eq!(radius, (20.0f64 * 1.1) as Real);
}

#[test]
fn camo_aura_lets_the_pin_pop_camo_reds() {
    let leaked = |with_village: bool| {
        let mut g = game();
        place(&mut g, PIN, 0.0, 15.0);
        if with_village {
            let v = place(&mut g, VILLAGE, -20.0, 15.0);
            upgrade(&mut g, v, 0, 2);
        }
        g.play_round();
        g.play_round().leaked
    };
    assert!(leaked(true) < leaked(false), "Runde 2 hat Camo-Reds");
}

#[test]
fn discount_stacks_per_stack_up_to_its_limit() {
    let mut g = game();
    let list = g.place_price(PIN, 0.0, 15.0).unwrap();
    assert_eq!(list, Money::from_dollars(100));
    let v1 = place(&mut g, VILLAGE, -20.0, 15.0);
    upgrade(&mut g, v1, 2, 1);
    assert_eq!(g.place_price(PIN, 0.0, 15.0), Some(Money::from_dollars(90)));
    assert_eq!(g.place_price(PIN, 45.0, -30.0), Some(list), "außer Reichweite");
    let v2 = place(&mut g, VILLAGE, 20.0, 15.0);
    upgrade(&mut g, v2, 2, 1);
    assert_eq!(g.place_price(PIN, 0.0, 15.0), Some(Money::from_dollars(90)), "großer Rabatt nur einmal");
    upgrade(&mut g, v1, 2, 1);
    upgrade(&mut g, v2, 2, 1);
    // 10 % + 2 × 5 %
    assert_eq!(g.place_price(PIN, 0.0, 15.0), Some(Money::from_dollars(80)));
}

#[test]
fn discount_applies_to_tiers_up_to_the_cap_and_into_spent() {
    let mut g = game();
    let v = place(&mut g, VILLAGE, -20.0, 15.0);
    upgrade(&mut g, v, 2, 1);
    let cash = g.cash;
    let pin = place(&mut g, PIN, 0.0, 15.0);
    upgrade(&mut g, pin, 0, 1);
    assert_eq!(g.tower(pin).unwrap().spent, Money::from_dollars(90 + 45));
    assert_eq!(cash - g.cash, Money::from_dollars(90 + 45));
    // die Village rabattiert ihre eigenen Upgrades nicht, eine zweite schon
    assert_eq!(g.upgrade_price(v, 2), Some(Money::from_dollars(100)));
    let v2 = place(&mut g, VILLAGE, -20.0, 30.0);
    assert_eq!(g.upgrade_price(v, 2), Some(Money::from_dollars(100)), "die zweite hat noch keinen Rabatt");
    upgrade(&mut g, v2, 2, 1);
    assert_eq!(g.upgrade_price(v, 2), Some(Money::from_dollars(90)));
    assert_eq!(g.discount_at(0.0, 15.0, 3, None), 0.1);
    assert_eq!(g.discount_at(0.0, 15.0, 4, None), 0.0, "Tier 4 liegt über der Grenze");
    // Verkaufswert aus dem bezahlten Preis
    let sell = g.tower(pin).unwrap().sell_value(0.7);
    assert_eq!(sell, Money::from_dollars(((90 + 45) as f64 * 0.7) as i64));
}

#[test]
fn deterministic_with_villages() {
    let run = || {
        let mut g = game();
        let v = place(&mut g, VILLAGE, -20.0, 15.0);
        upgrade(&mut g, v, 0, 2);
        place(&mut g, PIN, 0.0, 15.0);
        place(&mut g, BOMBP, 0.0, 28.0);
        let reports: Vec<_> = (0..3).map(|_| g.play_round()).collect();
        (reports, g.state_hash())
    };
    assert_eq!(run(), run());
}

#[test]
fn applying_buffs_keeps_runtime_state() {
    let mut g = game();
    let pin = place(&mut g, PIN, 0.0, 15.0);
    let t = g.towers.iter_mut().find(|t| t.id == pin).unwrap();
    let a = &mut t.attacks[0];
    (a.cd_left, a.shots, a.burst_left, a.burst_cd, a.crit_next) = (0.3, 2, 1, 0.05, 7);
    let runtime = |t: &btd_core::Tower| {
        let a = &t.attacks[0];
        (a.cd_left, a.shots, a.burst_left, a.burst_cd, a.crit_next)
    };
    let before = runtime(t);
    let b = Buffs { rate: 0.5, camo: true, range_frac: 0.1, ..Buffs::NONE };
    t.apply_buffs(b);
    assert_eq!(runtime(t), before);
    assert_eq!(t.attacks[0].cooldown, 0.25);
    t.apply_buffs(Buffs::NONE);
    assert_eq!(runtime(t), before);
}

#[test]
fn village_range_buff_widens_another_villages_aura() {
    // Abstand 32: außerhalb der Reichweite 30, aber innerhalb von 30 × 1,1 = 33
    let mut g = game();
    let a = place(&mut g, VILLAGE, -20.0, 15.0);
    let pin = place(&mut g, PIN, 12.0, 15.0);
    assert_eq!(buffs(&g, pin), Buffs::NONE);
    assert_eq!(g.tower(a).unwrap().reach, 30.0);
    // eine zweite Village bufft die erste (+10 %), damit erreicht deren Aura den Pin
    let b = place(&mut g, VILLAGE, -20.0, 30.0);
    assert_eq!(g.tower(a).unwrap().reach, 33.0);
    assert_eq!(buffs(&g, pin).range_frac, 0.1);
    // auch der Rabatt reicht dann weiter
    upgrade(&mut g, a, 2, 1);
    assert_eq!(g.place_price(PIN, 12.0, 15.0), Some(Money::from_dollars(90)));
    g.apply(Action::Sell { tower: b }).unwrap();
    assert_eq!((g.tower(a).unwrap().reach, buffs(&g, pin)), (30.0, Buffs::NONE));
    assert_eq!(g.place_price(PIN, 12.0, 15.0), Some(Money::from_dollars(100)));
}

#[test]
fn result_does_not_depend_on_build_order() {
    let build = |villages_first: bool, swap: bool| {
        let mut g = game();
        let mut pin = None;
        if !villages_first {
            pin = Some(place(&mut g, PIN, 0.0, 15.0));
        }
        let spots = if swap { [(20.0, 15.0), (-20.0, 15.0)] } else { [(-20.0, 15.0), (20.0, 15.0)] };
        for (k, (x, y)) in spots.into_iter().enumerate() {
            let v = place(&mut g, VILLAGE, x, y);
            upgrade(&mut g, v, if (k == 0) != swap { 0 } else { 1 }, if (k == 0) != swap { 2 } else { 1 });
        }
        let pin = pin.unwrap_or_else(|| place(&mut g, PIN, 0.0, 15.0));
        let t = g.tower(pin).unwrap();
        (t.attacks.clone(), t.buffs, t.reach)
    };
    let a = build(true, false);
    assert_ne!(a.1, Buffs::NONE);
    assert_eq!(a, build(true, true));
    assert_eq!(a, build(false, false));
    assert_eq!(a, build(false, true));
}

#[test]
fn upgrading_a_buffed_tower_keeps_its_buffs() {
    let mut buffed = game();
    let v = place(&mut buffed, VILLAGE, -20.0, 15.0);
    upgrade(&mut buffed, v, 0, 2);
    let pin = place(&mut buffed, PIN, 0.0, 15.0);
    upgrade(&mut buffed, pin, 0, 1);
    // gleicher Endzustand, wenn die Village erst nach dem Upgrade kommt
    let mut later = game();
    let pin2 = place(&mut later, PIN, 0.0, 15.0);
    upgrade(&mut later, pin2, 0, 1);
    let unbuffed = later.tower(pin2).unwrap().attacks.clone();
    let v2 = place(&mut later, VILLAGE, -20.0, 15.0);
    upgrade(&mut later, v2, 0, 2);
    assert_eq!(buffed.tower(pin).unwrap().attacks, later.tower(pin2).unwrap().attacks);
    // und ohne Village wieder exakt ungebufft
    buffed.apply(Action::Sell { tower: v }).unwrap();
    assert_eq!(buffed.tower(pin).unwrap().attacks, unbuffed);
}

#[test]
fn hitscan_keeps_its_pierce_and_filters_stay_under_mib() {
    const GLUE: u8 = 7;
    let mut g = game();
    let pin = place(&mut g, PIN, 0.0, 15.0);
    let glue = place(&mut g, GLUE, 0.0, 28.0);
    let extras = g.tower(glue).unwrap().attacks[0].extras.clone();
    let v = place(&mut g, VILLAGE, -20.0, 15.0);
    upgrade(&mut g, v, 1, 1);
    assert_eq!(buffs(&g, pin).pierce, 1);
    assert_eq!(g.tower(pin).unwrap().attacks[0].pierce, 1, "Sofort-Treffer: Pierce-Aura wirkt nicht");
    let v2 = place(&mut g, VILLAGE, 20.0, 28.0);
    upgrade(&mut g, v2, 0, 2);
    assert!(buffs(&g, glue).damage_all);
    assert_eq!(g.tower(glue).unwrap().attacks[0].extras, extras, "MIB lässt Tag-Ausschlüsse stehen");
}

#[test]
fn camo_and_mib_reach_shards() {
    const FRAG: u8 = 5;
    let mut g = game();
    let frag = place(&mut g, FRAG, 0.0, 15.0);
    let v = place(&mut g, VILLAGE, -20.0, 15.0);
    upgrade(&mut g, v, 0, 2);
    let AttackKind::Projectile(def) = &g.tower(frag).unwrap().attacks[0].kind else { panic!("Projektil") };
    let ContactRt::Shards { camo, cannot_damage, .. } = &def.contacts[0] else { panic!("Splitter") };
    assert!(*camo && cannot_damage.0 == 0);
}

#[test]
fn coupled_area_radius_grows_with_range() {
    // Ring of Fire: fester Radius 22 = Reichweite 20 + Versatz 2, wächst mit der Reichweite mit
    let mut g = game_of(data_with(|v| {
        v["towers"][ICE as usize]["variants"][0]["attacks"][0]["delivery"] =
            json!({"kind": "area", "radius": 22.0, "radius_offset": 2.0});
    }));
    let ice = place(&mut g, ICE, 0.0, 15.0);
    place(&mut g, VILLAGE, -20.0, 15.0);
    let AttackKind::Area { radius } = g.tower(ice).unwrap().attacks[0].kind else { panic!("Fläche") };
    assert_eq!(radius, (22.0 + (20.0f64 * 1.1 - 20.0)) as Real);
}

#[test]
fn discount_with_affect_self_and_tier_min() {
    let mut g = game_of(data_with(|v| {
        let d = &mut v["towers"][VILLAGE as usize]["variants"][4]["discount"][0];
        d["affect_self"] = json!(true);
        d["tier_min"] = json!(1);
    }));
    let v = place(&mut g, VILLAGE, -20.0, 15.0);
    upgrade(&mut g, v, 2, 1);
    assert_eq!(g.upgrade_price(v, 2), Some(Money::from_dollars(90)), "rabattiert die eigenen Upgrades");
    assert_eq!(g.place_price(PIN, 0.0, 15.0), Some(Money::from_dollars(100)), "Tier 0 liegt unter tier_min");
    let pin = place(&mut g, PIN, 0.0, 15.0);
    assert_eq!(g.upgrade_price(pin, 0), Some(Money::from_dollars(45)));
}
