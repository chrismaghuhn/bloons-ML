//! Startzustände: Aufbau, Störungen, zufällige Builds, Pool-Starts und Ergebnisse.

use std::path::PathBuf;

use btd_core::Action;
use btd_env::start::{perturb, Dice};
use btd_env::{Env, EnvConfig, EnvSpec, StartConfig, StartKind, StartState, VecEnv, VecStepBuffers};

fn spec() -> EnvSpec {
    EnvSpec::load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.game_v1.json"))
        .unwrap()
}

fn env() -> Env {
    Env::new(spec(), EnvConfig::default()).unwrap()
}

/// Ein Spiel mit ein paar Türmen und Upgrades nach 3 Runden.
fn played() -> Env {
    let mut e = env();
    let mut obs = vec![0.0; e.layout.obs_size];
    e.reset(&mut obs);
    let l = e.layout;
    let free = |e: &Env, k: u8| (0..l.cells).find(|&c| e.game.can_place_at(k, l.center(c).0, l.center(c).1));
    for round in 0..3 {
        for k in 0..2u8 {
            if let Some(c) = free(&e, k) {
                let (x, y) = l.center(c);
                let _ = e.game.apply(Action::Place { kind: k, x, y });
            }
        }
        if round == 2 {
            let id = e.game.towers[0].id;
            e.game.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
        }
        e.step(0, &mut obs);
    }
    assert!(!e.game.over() && e.game.towers.len() >= 2);
    e
}

struct Seq(u64);
impl Dice for Seq {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 1
    }
}

#[test]
fn state_roundtrip_rebuilds_the_same_game() {
    let src = played();
    let state = StartState::from_game(&src.game);
    let json = serde_json::to_string(&state).unwrap();
    let back: StartState = serde_json::from_str(&json).unwrap();
    assert_eq!(back, state);

    let mut e = env();
    let mut obs = vec![0.0; e.layout.obs_size];
    e.reset_from(&mut obs, &back).unwrap();
    assert_eq!(e.game.state_hash(), src.game.state_hash(), "gleiche Türme, Stufen, Ausgaben, Cash, Runde");
    assert_eq!(e.start_kind(), StartKind::Pool);
    // die Observation kennt die späte Runde
    let mut o2 = vec![0.0; e.layout.obs_size];
    src.observe(&mut o2);
    assert_eq!(obs, o2);
}

#[test]
fn invalid_states_are_rejected() {
    let good = StartState::from_game(&played().game);
    let mut e = env();
    let mut obs = vec![0.0; e.layout.obs_size];
    let bad = |f: &dyn Fn(&mut StartState)| {
        let mut s = good.clone();
        f(&mut s);
        s
    };
    for s in [
        bad(&|s| s.towers[0].tower = "gibtsnicht".into()),
        bad(&|s| s.towers[1].x = s.towers[0].x), // überlappt
        bad(&|s| s.towers[0].tiers = [9, 0, 0]),
        bad(&|s| s.round = 10_000),
        bad(&|s| s.lives = 0),
        bad(&|s| s.cash = f64::NAN),
    ] {
        assert!(e.reset_from(&mut obs, &s).is_err(), "{s:?}");
        assert_eq!((e.game.round, e.game.towers.len()), (0, 0), "nach Fehler normaler Start");
    }
}

#[test]
fn downgrade_refunds_and_lives_stay_in_range() {
    let base = StartState::from_game(&played().game);
    let e = env();
    let data = e.game.data().clone();
    // Wert der Türme zu Spielpreisen (Basis + alle gekauften Upgrades)
    let value = |s: &StartState| -> f64 {
        s.towers
            .iter()
            .map(|t| {
                let d = data.towers.iter().find(|d| d.name == t.tower).unwrap();
                let ups: i64 = (0..t.tiers.len())
                    .map(|p| (0..t.tiers[p] as usize).map(|k| data.cost(d.upgrade_costs[p][k])).sum::<i64>())
                    .sum();
                (data.cost(d.cost) + ups) as f64
            })
            .sum()
    };
    let mut refunds = 0;
    for seed in 0..300 {
        let p = perturb(&base, &e.game, e.layout.cell, &mut Seq(seed));
        assert!(p.state.lives >= 1 && p.state.lives <= base.lives);
        if p.lives_perturbed {
            assert!(p.state.lives * 2 >= base.lives, "höchstens die Hälfte der Leben weg");
        } else {
            assert_eq!(p.state.lives, base.lives);
        }
        assert!(p.state.cash >= 0.0);
        let lost = value(&base) - value(&p.state);
        if lost > 0.0 {
            // Downgrade ohne Cash-Störung: Cash steigt um genau den verlorenen Turmwert
            if (p.state.cash - base.cash - lost).abs() < 1e-9 {
                refunds += 1;
            }
        }
    }
    assert!(refunds > 10, "Downgrades mit exakter Erstattung kommen vor ({refunds})");
}

#[test]
fn pool_starts_outcomes_and_snapshots() {
    let base = StartState::from_game(&played().game);
    let cfg = StartConfig {
        pool: vec![base.clone()],
        weights: vec![1.0],
        ids: vec![42],
        p_start: 1.0,
        p_random: 0.0,
        budget: vec![],
        budget_spread: 0.3,
    };
    let mut v = VecEnv::new(spec(), EnvConfig { seed: 1, ..Default::default() }, 4).unwrap();
    v.set_starts(Some(cfg)).unwrap();
    let n = v.layout.obs_size;
    let mut obs = vec![0.0; 4 * n];
    v.reset(&mut obs);
    for e in &v.envs {
        assert_eq!(e.start_kind(), StartKind::Pool);
        assert_eq!(e.game.round, base.round, "startet in der Runde des Zustands");
    }
    let (mut tobs, mut rew, mut done, mut rounds, mut kinds) =
        (vec![0.0; 4 * n], vec![0.0; 4], vec![false; 4], vec![0; 4], vec![0u8; 4]);
    for _ in 0..3 {
        v.step(
            &[0; 4],
            VecStepBuffers {
                obs: &mut obs,
                rewards: &mut rew,
                dones: &mut done,
                terminal_obs: &mut tobs,
                rounds: &mut rounds,
                start_kinds: &mut kinds,
            },
        );
        assert!(kinds.iter().all(|&k| k == StartKind::Pool as u8));
    }
    let out = v.take_outcomes();
    assert_eq!(out.len(), 4, "je Env eine 3-Runden-Prüfung");
    assert!(out.iter().all(|o| o.base == Some(42) && o.kind == StartKind::Pool), "stabile ID statt Index");
    assert!(v.take_outcomes().is_empty(), "abgeholt");
    assert!(v.take_snapshots().is_empty(), "Pool-Episoden liefern keine Snapshots");

    // normale Episoden liefern je beendeter Episode einen Snapshot (Quelle A)
    v.set_starts(Some(StartConfig { p_start: 0.0, ..StartConfig::default() })).unwrap();
    v.reset(&mut obs);
    let mut finished = 0;
    while finished < 4 {
        v.step(
            &[0; 4],
            VecStepBuffers {
                obs: &mut obs,
                rewards: &mut rew,
                dones: &mut done,
                terminal_obs: &mut tobs,
                rounds: &mut rounds,
                start_kinds: &mut kinds,
            },
        );
        finished += done.iter().filter(|&&d| d).count();
    }
    // der Snapshot einer Episode wird beim nächsten Reset abgelegt
    let snaps = v.take_snapshots();
    assert!(snaps.len() >= 4, "{}", snaps.len());
    assert!(snaps.iter().all(|s| s.round >= 1 && s.lives > 0));
}

#[test]
fn random_builds_respect_budget_and_bans() {
    let mut e = Env::new(spec(), EnvConfig { max_banned_towers: 2, seed: 3, ..Default::default() }).unwrap();
    let budget: Vec<f64> = (0..50).map(|r| 650.0 + 300.0 * r as f64).collect();
    e.set_starts(Some(std::sync::Arc::new(StartConfig {
        p_start: 1.0,
        p_random: 1.0,
        budget: budget.clone(),
        budget_spread: 0.3,
        ..StartConfig::default()
    })));
    let mut obs = vec![0.0; e.layout.obs_size];
    let mut with_towers = 0;
    for _ in 0..50 {
        e.reset(&mut obs);
        assert_eq!(e.start_kind(), StartKind::Random);
        let g = &e.game;
        let end = g.data().rules.end_round;
        assert!(g.round + 5 <= end, "Startrunde mindestens 5 vor dem Ende");
        let spent: f64 = g.towers.iter().map(|t| t.spent.as_f64()).sum();
        let total = g.cash.as_f64() + spent;
        let b = budget[g.round as usize];
        assert!(total <= b * 1.3 + 1.0 && total >= b * 0.7 - 1.0, "Wert {total} passt zum Budget {b}");
        for t in &g.towers {
            assert!(!e.banned()[t.kind as usize], "kein gesperrter Typ");
        }
        with_towers += (!g.towers.is_empty()) as u32;
    }
    assert!(with_towers > 40, "meist mit Türmen");
}

#[test]
fn crosspath_and_sold_towers_roundtrip() {
    let mut src = played();
    let id = src.game.towers[0].id;
    src.game.cash = btd_core::Money::from_dollars(100_000);
    for p in [1u8, 1, 0] {
        src.game.apply(Action::Upgrade { tower: id, path: p }).unwrap();
    }
    let sold = src.game.towers[1].id;
    src.game.apply(Action::Sell { tower: sold }).unwrap();
    let state = StartState::from_game(&src.game);
    assert!(state.towers[0].tiers.iter().filter(|&&t| t > 0).count() == 2, "Crosspath");
    let mut e = env();
    let mut obs = vec![0.0; e.layout.obs_size];
    e.reset_from(&mut obs, &state).unwrap();
    let t =
        |g: &btd_core::Game| g.towers.iter().map(|t| (t.kind, t.tiers, t.pos(), t.spent)).collect::<Vec<_>>();
    assert_eq!(t(&e.game), t(&src.game), "gleiche Türme, Stufen, Plätze und Ausgaben");
    assert_eq!((e.game.cash, e.game.lives, e.game.round), (src.game.cash, src.game.lives, src.game.round));
}

#[test]
fn rounds_at_the_edges_and_invalid_configs() {
    let mut e = env();
    let mut obs = vec![0.0; e.layout.obs_size];
    let end = e.game.data().rules.end_round;
    let s = |round| StartState { round, cash: 100.0, lives: 10, towers: vec![] };
    assert!(e.reset_from(&mut obs, &s(0)).is_ok(), "vor Runde 1");
    assert!(e.reset_from(&mut obs, &s(end - 1)).is_ok(), "vor der letzten Runde");
    assert!(e.reset_from(&mut obs, &s(end)).is_err());
    assert!(e.reset_from(&mut obs, &StartState { cash: 1e12, ..s(1) }).is_err(), "Cash begrenzt");

    let mut v = VecEnv::new(spec(), EnvConfig::default(), 2).unwrap();
    let bad = StartConfig { pool: vec![s(end)], weights: vec![1.0], p_start: 1.0, ..StartConfig::default() };
    assert!(v.set_starts(Some(bad)).unwrap_err().contains("Zustand 0"));
    let no_budget = StartConfig { p_start: 1.0, p_random: 1.0, ..StartConfig::default() };
    assert!(v.set_starts(Some(no_budget)).is_err());
    let inf = StartConfig { budget: vec![f64::INFINITY], ..StartConfig::default() };
    assert!(v.set_starts(Some(inf)).is_err());
}

#[test]
fn failed_starts_are_counted_and_reset_from_ignores_starts() {
    let base = StartState::from_game(&played().game);
    // Gewicht 0: es lässt sich nichts ziehen -> normaler Start, gezählt
    let cfg =
        StartConfig { pool: vec![base.clone()], weights: vec![0.0], p_start: 1.0, ..StartConfig::default() };
    let mut v = VecEnv::new(spec(), EnvConfig::default(), 3).unwrap();
    v.set_starts(Some(cfg)).unwrap();
    let mut obs = vec![0.0; 3 * v.layout.obs_size];
    v.reset(&mut obs);
    assert!(v.envs.iter().all(|e| e.start_kind() == StartKind::Normal && e.game.round == 0));
    assert_eq!(v.take_fallbacks(), 3);

    // reset_from würfelt keinen Start; ein ungültiger Zustand ergibt einen normalen Start
    let mut e = env();
    e.set_starts(Some(std::sync::Arc::new(StartConfig {
        pool: vec![base.clone()],
        weights: vec![1.0],
        p_start: 1.0,
        ..StartConfig::default()
    })));
    let mut o = vec![0.0; e.layout.obs_size];
    let bad = StartState { round: 10_000, ..base };
    assert!(e.reset_from(&mut o, &bad).is_err());
    assert_eq!((e.start_kind(), e.game.round, e.game.towers.len()), (StartKind::Normal, 0, 0));
    assert_eq!(e.take_fallbacks(), 0);
}

#[test]
fn record_and_start_states() {
    let base = StartState::from_game(&played().game);
    let mut e = Env::new(spec(), EnvConfig { record: true, ..Default::default() }).unwrap();
    let mut obs = vec![0.0; e.layout.obs_size];
    // normale Episode bis zum Ende: Replay vorhanden
    e.reset(&mut obs);
    while !e.step(0, &mut obs).terminated {}
    assert!(e.last_replay().is_some());
    // Episode aus einem Zustand: kein Replay, auch nicht das alte
    e.reset_from(&mut obs, &base).unwrap();
    assert!(e.replay().is_none());
    while !e.step(0, &mut obs).terminated {}
    assert!(e.last_replay().is_none(), "kein Replay der vorigen Episode stehen lassen");
    // load_replay nimmt die Aufnahme wieder auf
    let mut f = Env::new(spec(), EnvConfig { record: true, ..Default::default() }).unwrap();
    f.reset(&mut obs);
    for _ in 0..3 {
        f.step(0, &mut obs);
    }
    let rep = f.replay().unwrap().clone();
    e.reset_from(&mut obs, &base).unwrap();
    e.load_replay(&rep, None, true).unwrap();
    assert!(e.replay().is_some(), "Aufnahme läuft nach load_replay weiter");
}

#[test]
fn perturbed_state_is_reproducible_and_buildable() {
    let base = StartState::from_game(&played().game);
    let e = env();
    let a = e.perturbed_state(&base, 5).unwrap();
    assert_eq!(a, e.perturbed_state(&base, 5).unwrap(), "gleicher Seed, gleicher Zustand");
    let differ = (0..20).filter(|&s| e.perturbed_state(&base, s).unwrap() != base).count();
    assert!(differ >= 18, "fast immer gestört ({differ})");
    let mut f = env();
    let mut obs = vec![0.0; f.layout.obs_size];
    f.reset_from(&mut obs, &a).unwrap();
}
