//! Env mit Support-Türmen (docs/design-towers-step2.md §2.2/§2.4/§4): Layout, feste Sperren,
//! Rabatt-Maske, Buff-Kanäle und `spent` in Startzuständen. Mini-Fixture plus Village und Farm.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, ActionError, Money};
use btd_data::v1::GameV1;
use btd_env::{Env, EnvConfig, EnvSpec, StartState, BUFF_CHANNELS};
use serde_json::json;

const PIN: u8 = 0;
const VILLAGE: u8 = 9;
const FARM: u8 = 10;

fn mini() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Mini-Fixture plus Village (oben: Tempo/Camo/MIB, unten: Rabatt) und Farm (gesperrt, wenn `lock`).
fn data(lock: bool) -> GameV1 {
    let mut v = mini();
    for t in v["towers"].as_array_mut().unwrap() {
        t["set"] = json!("primary");
    }
    let range = json!({"kind": "range", "group": "Range:Support", "add_frac": 0.1});
    let rate = json!({"kind": "rate", "group": "Village:Rate", "factor": 0.85});
    let camo = json!({"kind": "camo", "group": "Village:Visibility"});
    let mib = json!({"kind": "damage_all", "group": "Village:DamageType"});
    let major = json!({"fraction": 0.1, "tier_min": 0, "tier_cap": 3, "stack": "major", "stack_limit": 1});
    let variant = |tiers: [u8; 3], supports: serde_json::Value, discount: serde_json::Value| {
        json!({"tiers": tiers, "range": 25.0, "footprint": 5.0, "attacks": [], "supports": supports,
               "discount": discount})
    };
    let towers = v["towers"].as_array_mut().unwrap();
    towers.push(json!({
        "name": "village", "set": "support", "cost": 500, "upgrade_costs": [[100, 100], [], [100]],
        "variants": [
            variant([0, 0, 0], json!([range]), json!([])),
            variant([1, 0, 0], json!([range, rate]), json!([])),
            variant([2, 0, 0], json!([range, rate, camo, mib]), json!([])),
            variant([0, 0, 1], json!([range]), json!([major])),
        ]
    }));
    towers.push(json!({
        "name": "farm", "set": "support", "cost": 400, "upgrade_costs": [[], [], []],
        "variants": [{"tiers": [0, 0, 0], "range": 25.0, "footprint": 7.0, "attacks": [],
                      "income": {"per_round": 80.0}}]
    }));
    if lock {
        v["rules"]["locked_towers"] = json!(["farm"]);
    }
    GameV1::from_json(&v.to_string()).unwrap()
}

fn spec_of(d: GameV1) -> EnvSpec {
    EnvSpec { data: Arc::new(d), data_hash: 0, data_name: "mini+support".into() }
}

fn env_with(lock: bool, cfg: EnvConfig) -> Env {
    Env::new(spec_of(data(lock)), cfg).unwrap()
}

fn env() -> Env {
    env_with(true, EnvConfig::default())
}

fn buffers(e: &Env) -> (Vec<f32>, Vec<bool>) {
    (vec![0.0; e.layout.obs_size], vec![false; e.layout.n_actions])
}

fn place_action(e: &Env, kind: u8, x: f64, y: f64) -> usize {
    let c = e.layout.cell_of(x, y).unwrap();
    e.layout.a_place + kind as usize * e.layout.cells + c
}

#[test]
fn layout_with_support_towers_is_fixed() {
    let towers = 9 + 2;
    let e = env();
    assert!(e.layout.support);
    assert_eq!(e.layout.n_channels, 2 + towers + 3 + BUFF_CHANNELS);
    assert_eq!(e.layout.n_scalars, 4 + towers + 1, "Sperr-Skalare immer, dazu Einkommen");
    // gleiche Größe ohne feste Sperre und mit Zufallssperren
    let free = env_with(false, EnvConfig::default());
    let bans = env_with(true, EnvConfig { max_banned_towers: 2, ..Default::default() });
    assert_eq!((free.layout, bans.layout), (e.layout, e.layout));
    // ohne Support-Türme bleibt das Layout wie bisher
    let plain =
        Env::new(spec_of(GameV1::from_json(&mini().to_string()).unwrap()), EnvConfig::default()).unwrap();
    assert!(!plain.layout.support);
    assert_eq!((plain.layout.n_channels, plain.layout.n_scalars), (2 + 9 + 3, 4));
}

#[test]
fn locked_farm_is_never_placeable_nor_drawn_as_ban() {
    let mut e = env_with(true, EnvConfig { max_banned_towers: 9, seed: 3, ..Default::default() });
    let (mut obs, mut mask) = buffers(&e);
    e.game.cash = Money::from_dollars(100_000);
    for _ in 0..30 {
        e.reset(&mut obs);
        e.game.cash = Money::from_dollars(100_000);
        e.action_mask(&mut mask);
        let farm = e.layout.a_place + FARM as usize * e.layout.cells;
        assert!(!mask[farm..farm + e.layout.cells].iter().any(|&m| m), "Farm nie in der Maske");
        assert!(!e.banned()[FARM as usize], "feste Sperre zählt nicht als Zufallssperre");
        e.observe(&mut obs);
        let bans = e.layout.n_channels * e.layout.cells + 4;
        assert_eq!(obs[bans + FARM as usize], 0.0, "Observation zeigt die Farm als gesperrt");
    }
    assert_eq!(e.game.apply(Action::Place { kind: FARM, x: 25.0, y: 25.0 }), Err(ActionError::TowerLocked));
    // alle nicht gesperrten Türme zufällig sperren wäre zu viel
    assert!(Env::new(spec_of(data(true)), EnvConfig { max_banned_towers: 10, ..Default::default() }).is_err());
}

#[test]
fn place_mask_uses_the_discounted_price_per_cell() {
    let mut e = env();
    let (mut obs, mut mask) = buffers(&e);
    e.reset(&mut obs);
    e.game.cash = Money::from_dollars(10_000);
    let v = e.game.apply(Action::Place { kind: VILLAGE, x: -25.0, y: 15.0 }).unwrap().unwrap();
    e.game.apply(Action::Upgrade { tower: v, path: 2 }).unwrap();
    e.game.cash = Money::from_dollars(90); // reicht nur mit Rabatt für den Pin (100)
    e.action_mask(&mut mask);
    let near = place_action(&e, PIN, -5.0, 15.0); // 20 von der Village
    let far = place_action(&e, PIN, 25.0, 15.0); // 50 von der Village
    assert!(mask[near] && !mask[far]);
    let info = e.step(near, &mut obs);
    assert!(!info.invalid);
    assert_eq!(e.game.cash, Money::ZERO);
}

#[test]
fn buff_channels_follow_villages() {
    let mut e = env();
    let (mut obs, _) = buffers(&e);
    e.reset(&mut obs);
    e.game.cash = Money::from_dollars(10_000);
    let l = e.layout;
    let b0 = (2 + l.towers + l.paths) * l.cells;
    let near = l.cell_of(-5.0, 15.0).unwrap();
    let channel = |obs: &[f32], k: usize| obs[b0 + k * l.cells + near];
    let v = e.game.apply(Action::Place { kind: VILLAGE, x: -25.0, y: 15.0 }).unwrap().unwrap();
    e.game.apply(Action::Upgrade { tower: v, path: 0 }).unwrap();
    e.game.apply(Action::Upgrade { tower: v, path: 0 }).unwrap();
    e.observe(&mut obs);
    let expect = [0.5, ((1.0 - 0.85f64) * 4.0) as f32, 1.0, 1.0, 0.0];
    for (k, x) in expect.into_iter().enumerate() {
        assert!((channel(&obs, k) - x).abs() < 1e-6, "Kanal {k}: {} statt {x}", channel(&obs, k));
    }
    let far = l.cell_of(25.0, 15.0).unwrap();
    assert!((0..BUFF_CHANNELS).all(|k| obs[b0 + k * l.cells + far] == 0.0));
    // verkauft: der Cache rechnet neu
    e.game.apply(Action::Sell { tower: v }).unwrap();
    e.observe(&mut obs);
    assert!((0..BUFF_CHANNELS).all(|k| channel(&obs, k) == 0.0));
}

#[test]
fn spent_survives_the_start_state_roundtrip() {
    // Pin vor der Village gebaut, danach rabattiert upgegradet: der Aufbau (Turm für Turm komplett)
    // käme auf einen anderen Preis, `spent` hält den bezahlten fest
    let mut e = env();
    let (mut obs, _) = buffers(&e);
    e.reset(&mut obs);
    e.game.cash = Money::from_dollars(10_000);
    let pin = e.game.apply(Action::Place { kind: PIN, x: -5.0, y: 15.0 }).unwrap().unwrap();
    let v = e.game.apply(Action::Place { kind: VILLAGE, x: -25.0, y: 15.0 }).unwrap().unwrap();
    e.game.apply(Action::Upgrade { tower: v, path: 2 }).unwrap();
    e.game.apply(Action::Upgrade { tower: pin, path: 0 }).unwrap();
    let paid = e.game.tower(pin).unwrap().spent;
    assert_eq!(paid, Money::from_dollars(100 + 45));
    let state = StartState::from_game(&e.game);
    assert_eq!(state.towers[0].spent, Some(145.0));
    e.reset_from(&mut obs, &state).unwrap();
    assert_eq!(e.game.towers[0].spent, paid);
    // ohne das Feld (ältere Zustände): der beim Aufbau bezahlte Preis
    let mut old = state.clone();
    old.towers.iter_mut().for_each(|t| t.spent = None);
    e.reset_from(&mut obs, &old).unwrap();
    assert_eq!(e.game.towers[0].spent, Money::from_dollars(150));
    // Zustände mit gesperrtem Turm werden abgelehnt
    let mut farm = state.clone();
    farm.towers.push(btd_env::start::TowerState {
        tower: "farm".into(),
        x: 25.0,
        y: 25.0,
        tiers: [0, 0, 0],
        spent: None,
    });
    assert!(e.reset_from(&mut obs, &farm).unwrap_err().contains("TowerLocked"));
}

#[test]
fn random_play_with_villages_stays_valid() {
    let mut e = env_with(true, EnvConfig { max_banned_towers: 2, seed: 7, ..Default::default() });
    let (mut obs, mut mask) = buffers(&e);
    let mut state = 12345u64;
    for _ in 0..20 {
        e.reset(&mut obs);
        e.game.cash = Money::from_dollars(5_000);
        loop {
            e.action_mask(&mut mask);
            let valid: Vec<usize> = (0..mask.len()).filter(|&i| mask[i]).collect();
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let info = e.step(valid[(state % valid.len() as u64) as usize], &mut obs);
            assert!(!info.invalid, "Maske erlaubt nur gültige Aktionen");
            if info.terminated {
                break;
            }
        }
    }
}

/// Zustand: Village mit Rabatt (xx1), daneben ein rabattiert gekaufter Pin (1-0-0).
fn discounted_state(e: &mut Env) -> StartState {
    let (mut obs, _) = buffers(e);
    e.reset(&mut obs);
    e.game.cash = Money::from_dollars(10_000);
    let v = e.game.apply(Action::Place { kind: VILLAGE, x: -25.0, y: 15.0 }).unwrap().unwrap();
    e.game.apply(Action::Upgrade { tower: v, path: 2 }).unwrap();
    let pin = e.game.apply(Action::Place { kind: PIN, x: -5.0, y: 15.0 }).unwrap().unwrap();
    e.game.apply(Action::Upgrade { tower: pin, path: 0 }).unwrap();
    assert_eq!(e.game.tower(pin).unwrap().spent, Money::from_dollars(90 + 45));
    StartState::from_game(&e.game)
}

#[test]
fn downgrade_refunds_the_discounted_price() {
    use btd_env::start::Perturbed;
    let mut e = env();
    let state = discounted_state(&mut e);
    let finish = |downgrades: Vec<(usize, u8)>| {
        let p = Perturbed { state: state.clone(), downgrades, sell: None, lives_perturbed: false };
        let mut g = e.game.clone();
        p.state.build(&mut g).unwrap();
        let cash = g.cash;
        p.finish(&mut g);
        (g.cash - cash, g)
    };
    // Pin-Upgrade zurück: 45 (rabattiert) erstattet, spent 135 -> 90
    let (refund, g) = finish(vec![(1, 0)]);
    assert_eq!(refund, Money::from_dollars(45));
    assert_eq!((g.towers[1].tiers, g.towers[1].spent), ([0, 0, 0], Money::from_dollars(90)));
    // Village-Rabatt zurück: die eigene Zone rabattiert sie nicht (100), danach kostet der Pin wieder voll
    let (refund, g) = finish(vec![(0, 2)]);
    assert_eq!(refund, Money::from_dollars(100));
    assert_eq!(g.upgrade_price(g.towers[1].id, 0), Some(Money::from_dollars(100)));
}

#[test]
fn reset_from_refreshes_the_raster_and_checks_spent() {
    let mut e = env();
    let state = discounted_state(&mut e);
    let (mut obs, _) = buffers(&e);
    e.reset(&mut obs);
    let l = e.layout;
    let discount = (2 + l.towers + l.paths + 4) * l.cells + l.cell_of(-15.0, 25.0).unwrap();
    assert_eq!(obs[discount], 0.0);
    e.reset_from(&mut obs, &state).unwrap();
    assert!((obs[discount] - 0.4).abs() < 1e-6, "Rabatt-Kanal nach dem Laden: {}", obs[discount]);
    let mut bad = state.clone();
    bad.towers[1].spent = Some(f64::NAN);
    assert!(e.reset_from(&mut obs, &bad).unwrap_err().contains("spent"));
}
