use std::path::PathBuf;

use btd_env::replay::{ReplayAction, RoundResult};
use btd_env::{CellAction, Env, EnvConfig, EnvSpec, Replay, ReplayError, VecEnv, VecStepBuffers};

fn spec() -> EnvSpec {
    EnvSpec::load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.game_v1.json"))
        .unwrap()
}

fn env(record: bool) -> Env {
    Env::new(spec(), EnvConfig { record, ..Default::default() }).unwrap()
}

/// Kleiner deterministischer Zufallsgenerator für Testpolicies.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick(&mut self, mask: &[bool]) -> usize {
        let valid: Vec<usize> = (0..mask.len()).filter(|&i| mask[i]).collect();
        valid[(self.next() % valid.len() as u64) as usize]
    }
}

/// Spielt eine Episode mit zufälligen, gültigen Aktionen.
fn random_episode(e: &mut Env, seed: u64) -> u32 {
    let mut rng = Rng(seed);
    let mut obs = vec![0.0; e.layout.obs_size];
    let mut mask = vec![false; e.layout.n_actions];
    e.reset(&mut obs);
    loop {
        e.action_mask(&mut mask);
        let info = e.step(rng.pick(&mask), &mut obs);
        assert!(!info.invalid, "Maske erlaubt nur gültige Aktionen");
        if info.terminated {
            return e.game.round;
        }
    }
}

#[test]
fn layout_matches_python_env() {
    let e = env(false);
    let l = e.layout;
    assert_eq!((l.cells, l.towers), (300, 4));
    assert_eq!(l.n_actions, 1 + 4 * 300 + 2 * 300 + 300);
    assert_eq!(l.obs_size, 8 * 300 + 4);
    assert_eq!(l.decode(0), None);
    assert_eq!(l.paths, 2, "sim_v0 hat zwei Upgrade-Pfade");
    assert_eq!(l.decode(1 + 3 * 300 + 4 * 20 + 7), Some(CellAction::Place { kind: 3, cell: 87 }));
    assert_eq!(l.decode(l.a_upgrade + 300 + 5), Some(CellAction::Upgrade { cell: 5, path: 1 }));
    assert_eq!(l.decode(l.a_sell + 21), Some(CellAction::Sell { cell: 21 }));
    assert_eq!(l.center(87), (7.5, 4.5));
    assert_eq!(l.cell_of(7.5, 4.5), Some(87));
    assert_eq!(l.cell_of(-0.1, 4.5), None);
    assert_eq!(l.decode(l.n_actions), None);
}

#[test]
fn masks_follow_cash_and_build_limit() {
    let mut e = env(false);
    let l = e.layout;
    let mut obs = vec![0.0; l.obs_size];
    let mut mask = vec![false; l.n_actions];
    e.reset(&mut obs);
    e.action_mask(&mut mask);
    let place_dart = l.a_place + 4 * 20 + 7;
    let place_bomb_next_to_it = l.a_place + 3 * 300 + 4 * 20 + 8;
    assert!(mask[0] && mask[place_dart] && mask[place_bomb_next_to_it], "650 $ reichen für Dart und Bomb");
    e.step(place_dart, &mut obs); // 450 $ übrig
    e.action_mask(&mut mask);
    assert!(mask[l.a_sell + 4 * 20 + 7]);
    assert!(!mask[place_dart], "Zelle ist belegt");
    assert!(!mask[place_bomb_next_to_it], "Bomb (525 $) passt nicht mehr");
    // insgesamt 6 Bauaktionen (Verkaufen zählt mit), danach ist nur noch "Runde starten" erlaubt
    for _ in 0..2 {
        e.step(l.a_sell + 4 * 20 + 7, &mut obs);
        e.step(place_dart, &mut obs);
    }
    e.step(l.a_sell + 4 * 20 + 7, &mut obs);
    assert_eq!(e.builds_left, 0);
    e.action_mask(&mut mask);
    assert_eq!(mask.iter().filter(|&&m| m).count(), 1, "Bau-Limit erreicht: nur noch Runde starten");
    assert!(mask[0]);
}

#[test]
fn replay_roundtrip_and_strict_play() {
    let mut e = env(true);
    let rounds = random_episode(&mut e, 7);
    let replay = e.last_replay().expect("Replay nach Spielende").clone();
    assert!(!replay.rounds.is_empty());
    let text = replay.to_jsonl();
    let loaded = Replay::from_jsonl(&text).unwrap();
    assert_eq!(loaded, replay);

    let mut checked = 0;
    let game = loaded
        .play(e.spec(), None, |c| {
            assert!(c.matches);
            checked += 1;
        })
        .unwrap();
    assert_eq!(checked, replay.rounds.len());
    assert_eq!(game.state_hash(), e.game.state_hash());
    assert_eq!(game.round, rounds);
}

#[test]
fn resume_from_round_continues_identically() {
    let mut a = env(true);
    random_episode(&mut a, 11);
    let full = a.last_replay().unwrap().clone();
    assert!(full.rounds.len() > 6);

    let mut b = env(true);
    b.load_replay(&full, Some(5), true).unwrap();
    assert_eq!(b.game.round, 5);
    assert_eq!(b.replay().unwrap().rounds.len(), 5);
    // Rest des Originals von Runde 6 an anwenden -> gleiches Ende
    let mut obs = vec![0.0; b.layout.obs_size];
    for entry in &full.rounds[5..] {
        for act in &entry.actions {
            let idx = action_index(&b, act);
            assert!(!b.step(idx, &mut obs).invalid);
        }
        b.step(0, &mut obs);
    }
    assert_eq!(b.game.state_hash(), a.game.state_hash());
    assert_eq!(b.replay().unwrap().rounds, full.rounds);
}

fn action_index(e: &Env, a: &ReplayAction) -> usize {
    let l = e.layout;
    match a {
        ReplayAction::Place { tower, x, y } => {
            let k = e.game.data().towers.iter().position(|t| &t.name == tower).unwrap();
            l.a_place + k * l.cells + l.cell_of(*x, *y).unwrap()
        }
        ReplayAction::Upgrade { x, y, path } => {
            l.a_upgrade + *path as usize * l.cells + l.cell_of(*x, *y).unwrap()
        }
        ReplayAction::Sell { x, y } => l.a_sell + l.cell_of(*x, *y).unwrap(),
    }
}

#[test]
fn rejects_replay_for_other_data() {
    let mut e = env(true);
    random_episode(&mut e, 3);
    let mut replay = e.last_replay().unwrap().clone();
    replay.header.data_hash = Some("0000000000000000".into());
    assert!(matches!(replay.play(e.spec(), None, |_| {}), Err(ReplayError::DataMismatch(_))));
}

#[test]
fn hand_written_partial_replay_is_compared_not_rejected() {
    // so könnte ein echtes Spiel aufgeschrieben werden: nur Leben und Cash
    let text = r#"
{"type":"header","format":"btd-replay/2","data":"sim_v0.game_v1.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7.5,"y":4.5}}],"result":{"lives":150}}
{"type":"round","round":2,"result":{"lives":150,"cash":12345}}
"#;
    let replay = Replay::from_jsonl(text).unwrap();
    let checks = replay.compare(&spec()).unwrap();
    assert_eq!(checks.len(), 2);
    assert!(checks[0].matches, "{:?}", checks[0]);
    assert!(!checks[1].matches, "Cash weicht ab");
    assert!(matches!(replay.play(&spec(), None, |_| {}), Err(ReplayError::Mismatch { round: 2, .. })));
}

#[test]
fn malformed_replays_are_rejected() {
    for (text, needle) in [
        ("", "leere Datei"),
        (r#"{"type":"round","round":1}"#, "Header fehlt"),
        (r#"{"type":"header","format":"x","data":"d"}"#, "Format"),
        ("{\"type\":\"header\",\"format\":\"btd-replay/2\",\"data\":\"d\"}\n{\"type\":\"round\",\"round\":0}", "ab 1"),
    ] {
        let err = Replay::from_jsonl(text).unwrap_err().to_string();
        assert!(err.contains(needle), "{err} enthält nicht {needle}");
    }
    let bad_action = "{\"type\":\"header\",\"format\":\"btd-replay/2\",\"data\":\"d\"}\n\
        {\"type\":\"round\",\"round\":1,\"actions\":[{\"place\":{\"tower\":\"dart\",\"x\":0,\"y\":5}}]}";
    let r = Replay::from_jsonl(bad_action).unwrap();
    // strikt: Abbruch; nicht-strikt: Aktion übersprungen und vermerkt
    assert!(matches!(r.play(&spec(), None, |_| {}), Err(ReplayError::Action { round: 1, .. })));
    let checks = r.compare(&spec()).unwrap();
    assert_eq!(checks.len(), 1);
    assert!(!checks[0].matches && checks[0].action_errors[0].contains("NotPlaceable"));
    let _ = RoundResult::default();
}

#[test]
fn vec_env_auto_resets_and_matches_single_env() {
    let spec = spec();
    let mut v = VecEnv::new(spec.clone(), EnvConfig::default(), 3).unwrap();
    let mut single = Env::new(spec, EnvConfig::default()).unwrap();
    let (n_obs, n_act) = (v.layout.obs_size, v.layout.n_actions);
    let mut obs = vec![0.0; 3 * n_obs];
    let mut tobs = vec![0.0; 3 * n_obs];
    let (mut rew, mut done, mut rounds) = (vec![0.0; 3], vec![false; 3], vec![0; 3]);
    let mut kinds = vec![0u8; 3];
    let mut masks = vec![false; 3 * n_act];
    let mut sobs = vec![0.0; n_obs];
    v.reset(&mut obs);
    single.reset(&mut sobs);
    assert_eq!(&obs[..n_obs], &sobs[..]);

    // nur "Runde starten": Env 0 wird verlieren, dann automatisch zurückgesetzt
    let mut steps = 0;
    loop {
        v.action_masks(&mut masks);
        v.step(
            &[0, 0, 0],
            VecStepBuffers {
                obs: &mut obs,
                rewards: &mut rew,
                dones: &mut done,
                terminal_obs: &mut tobs,
                rounds: &mut rounds,
                start_kinds: &mut kinds,
            },
        );
        let info = single.step(0, &mut sobs);
        assert_eq!(rew[0], info.reward);
        steps += 1;
        if done[0] {
            assert!(info.terminated);
            assert_eq!(&tobs[..n_obs], &sobs[..], "terminal_obs = Zustand vor dem Reset");
            assert_eq!(v.envs[0].game.round, 0, "nach Auto-Reset wieder am Anfang");
            break;
        }
        assert!(steps < 100);
    }
}

#[test]
fn compare_continues_after_impossible_actions() {
    // echtes Spiel mit mehr Cash als in der Sim: Bomb in Runde 1 ist nicht bezahlbar (Dart schon gekauft)
    let text = r#"
{"type":"header","format":"btd-replay/2","data":"sim_v0.game_v1.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7.5,"y":4.5}},{"place":{"tower":"bomb","x":8.5,"y":4.5}}]}
{"type":"round","round":2,"actions":[{"place":{"tower":"dart","x":5.5,"y":6.5}}],"result":{"lives":150,"round":2}}
"#;
    let checks = Replay::from_jsonl(text).unwrap().compare(&spec()).unwrap();
    assert_eq!(checks.len(), 2, "Vergleich läuft trotz unmöglicher Aktion weiter");
    assert!(!checks[0].matches && checks[0].action_errors.len() == 1);
    assert!(checks[1].matches && checks[1].action_errors.is_empty());
}

#[test]
fn resume_after_loss_keeps_losing_round() {
    let mut a = env(true);
    let mut obs = vec![0.0; a.layout.obs_size];
    a.reset(&mut obs);
    while !a.step(0, &mut obs).terminated {}
    let lost = a.last_replay().unwrap().clone();
    let n = lost.rounds.len();

    for until in [None, Some(n as u32), Some(999)] {
        let mut b = env(true);
        let checks = b.load_replay(&lost, until, true).unwrap();
        assert_eq!(checks.len(), n);
        assert!(b.game.over());
        assert_eq!(b.replay().unwrap().rounds.len(), n, "verlorene Runde bleibt im Protokoll");
        assert_eq!(b.last_replay().unwrap(), &lost);
        assert!(b.step(0, &mut obs).terminated);
    }
}

#[test]
fn replays_from_other_float_build_need_non_strict_mode() {
    let mut e = env(true);
    random_episode(&mut e, 5);
    let mut replay = e.last_replay().unwrap().clone();
    replay.header.float = Some(if btd_env::replay::FLOAT == "f32" { "f64" } else { "f32" }.into());
    assert!(matches!(replay.play(e.spec(), None, |_| {}), Err(ReplayError::FloatMismatch { .. })));
    assert!(replay.compare(e.spec()).is_ok());
}

#[test]
fn rejects_zero_build_limit() {
    assert!(Env::new(spec(), EnvConfig { max_builds_per_round: 0, ..Default::default() }).is_err());
}

#[test]
fn resume_after_non_strict_load_records_only_what_happened() {
    let text = r#"
{"type":"header","format":"btd-replay/2","data":"sim_v0.game_v1.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7.5,"y":4.5}},{"place":{"tower":"bomb","x":8.5,"y":4.5}}],"result":{"cash":9999}}
{"type":"round","round":2,"result":{"lives":150}}
"#;
    let real_game = Replay::from_jsonl(text).unwrap();
    let mut e = env(true);
    assert!(e.load_replay(&real_game, None, true).is_err(), "strikt geht das nicht");
    let checks = e.load_replay(&real_game, None, false).unwrap();
    assert_eq!(checks[0].action_errors.len(), 1);
    let recorded = e.replay().unwrap().clone();
    assert_eq!(recorded.rounds[0].actions.len(), 1, "übersprungene Bomb ist nicht im Protokoll");
    assert_ne!(recorded.rounds[0].result.cash, Some(9999), "eigenes Ergebnis statt fremdem");
    // das fortgesetzte Protokoll ist in sich stimmig und strikt nachspielbar
    let game = recorded.play(e.spec(), None, |_| {}).unwrap();
    assert_eq!(game.state_hash(), e.game.state_hash());
}

#[test]
fn strict_play_rejects_rounds_after_game_end() {
    let mut a = env(true);
    let mut obs = vec![0.0; a.layout.obs_size];
    a.reset(&mut obs);
    while !a.step(0, &mut obs).terminated {}
    let mut lost = a.last_replay().unwrap().clone();
    let extra = btd_env::replay::RoundEntry {
        round: lost.rounds.len() as u32 + 1,
        actions: vec![],
        result: Default::default(),
    };
    lost.rounds.push(extra);
    assert!(matches!(lost.play(a.spec(), None, |_| {}), Err(ReplayError::TrailingRounds { .. })));
    assert_eq!(lost.compare(a.spec()).unwrap().len(), lost.rounds.len() - 1);
}

#[test]
fn three_upgrade_paths_on_mini_fixture() {
    let spec = EnvSpec::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json"),
    )
    .unwrap();
    let mut e = Env::new(spec, EnvConfig::default()).unwrap();
    let l = e.layout;
    assert_eq!((l.cells, l.paths, l.max_tier), (80, 3, 2));
    assert_eq!(l.towers, e.game.data().towers.len());
    assert_eq!(l.n_actions, 1 + l.towers * 80 + 3 * 80 + 80);
    let mut obs = vec![0.0; l.obs_size];
    let mut mask = vec![false; l.n_actions];
    e.reset(&mut obs);
    e.action_mask(&mut mask);
    let cell = l.cell_of(5.0, 25.0).unwrap();
    assert!(mask[l.a_place + cell], "Land neben dem Track");
    assert!(!mask[l.a_place + l.cell_of(5.0, 5.0).unwrap()], "Track");
    e.step(l.a_place + cell, &mut obs);
    e.action_mask(&mut mask);
    for p in 0..3 {
        assert!(mask[l.a_upgrade + p * l.cells + cell], "Pfad {p}");
    }
    e.step(l.a_upgrade + 2 * l.cells + cell, &mut obs); // Pfad 3
    let tower_obs = obs[(2 + l.towers + 2) * l.cells + cell];
    assert_eq!(tower_obs, 0.5, "Tier 1 von max. 2");
}

/// Lädt die mini-Fixture mit geänderten Daten über eine temporäre Datei.
fn spec_with(f: impl FnOnce(&mut serde_json::Value)) -> EnvSpec {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(src).unwrap()).unwrap();
    f(&mut v);
    let path = std::env::temp_dir().join(format!(
        "btd-env-test-{}-{:?}.json",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, v.to_string()).unwrap();
    let spec = EnvSpec::load(&path).unwrap();
    std::fs::remove_file(path).ok();
    spec
}

#[test]
fn replays_use_real_round_numbers_when_starting_later() {
    let spec = spec_with(|v| v["rules"]["start_round"] = serde_json::json!(2));
    let mut e = Env::new(spec.clone(), EnvConfig { record: true, ..Default::default() }).unwrap();
    let mut obs = vec![0.0; e.layout.obs_size];
    e.reset(&mut obs);
    assert_eq!(e.game.round, 1);
    while !e.step(0, &mut obs).terminated {}
    let replay = e.last_replay().unwrap().clone();
    assert_eq!(replay.rounds.first().unwrap().round, 2, "erste Runde ist die Startrunde");
    assert_eq!(replay.rounds.first().unwrap().result.round, Some(2));
    // strikt nachspielbar und ab einer echten Rundennummer fortsetzbar
    replay.play(&spec, None, |_| {}).unwrap();
    let mut b = Env::new(spec.clone(), EnvConfig { record: true, ..Default::default() }).unwrap();
    b.load_replay(&replay, Some(2), true).unwrap();
    assert_eq!(b.game.round, 2);
    // ein Protokoll, das bei Runde 1 beginnt, passt nicht zu Daten mit Startrunde 2
    let text =
        "{\"type\":\"header\",\"format\":\"btd-replay/2\",\"data\":\"d\"}\n{\"type\":\"round\",\"round\":1}";
    assert!(matches!(Replay::from_jsonl(text).unwrap().compare(&spec), Err(ReplayError::DataMismatch(_))));
}

#[test]
fn off_center_tower_from_replay_is_usable_through_its_cell() {
    let text = "{\"type\":\"header\",\"format\":\"btd-replay/2\",\"data\":\"d\"}\n\
        {\"type\":\"round\",\"round\":1,\"actions\":[{\"place\":{\"tower\":\"dart\",\"x\":7.1,\"y\":4.1}}]}";
    let replay = Replay::from_jsonl(text).unwrap();
    let mut e = env(false);
    e.load_replay(&replay, None, false).unwrap();
    let l = e.layout;
    let cell = l.cell_of(7.1, 4.1).unwrap();
    let mut mask = vec![false; l.n_actions];
    e.action_mask(&mut mask);
    assert!(mask[l.a_sell + cell] && mask[l.a_upgrade + cell], "Turm zählt zu seiner Zelle");
    let mut obs = vec![0.0; l.obs_size];
    let info = e.step(l.a_upgrade + cell, &mut obs);
    assert!(!info.invalid, "maskierte Aktion ist auch ausführbar");
    assert_eq!(e.game.towers[0].tiers[0], 1);
}

#[test]
fn round_preview_counts_down_to_special_bloons() {
    use btd_env::preview::CATEGORIES;
    let spec = EnvSpec::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json"),
    )
    .unwrap();
    let plain = Env::new(spec.clone(), EnvConfig::default()).unwrap();
    let mut e = Env::new(spec, EnvConfig { preview_rounds: 2, ..Default::default() }).unwrap();
    let extra = btd_env::RoundPreview::len(2);
    assert_eq!(e.layout.obs_size, plain.layout.obs_size + extra);
    let mut obs = vec![0.0; e.layout.obs_size];
    e.reset(&mut obs);
    let tail = &obs[obs.len() - extra..];
    let cat = |name: &str| CATEGORIES.iter().position(|c| *c == name).unwrap();
    let until = |tail: &[f32], name: &str| tail[2 * (CATEGORIES.len() + 2) + cat(name)];
    // Runde 1: 10 Reds; Runde 2: Camo; Runde 3: Ceramic + Lead
    assert!(tail[cat("camo")] == 0.0 && tail[CATEGORIES.len()] > 0.0, "Runde 1 ohne Camo, RBE > 0");
    assert!(tail[CATEGORIES.len() + 2 + cat("camo")] > 0.0, "Runde 2 mit Camo");
    assert_eq!((until(tail, "camo"), until(tail, "lead"), until(tail, "moab")), (0.1, 0.2, 1.0));
    e.step(0, &mut obs);
    let tail = &obs[obs.len() - extra..];
    assert_eq!((until(tail, "camo"), until(tail, "ceramic")), (0.0, 0.1));
    assert!(tail.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn round_preview_counts_descendants_and_fortified_tag() {
    use btd_env::preview::CATEGORIES;
    let mut data = btd_data::v1::GameV1::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json"),
    )
    .unwrap();
    // Runde 1: nur Ceramics, als Fortified-Variante markiert und mit einem Lead als Kind
    let ceramic = data.bloons.iter().position(|b| b.name == "ceramic").unwrap();
    let lead = data.bloons.iter().position(|b| b.name == "lead").unwrap();
    data.bloons[ceramic].tags.push("Fortified".into());
    data.bloons[ceramic].children = vec![(lead as u8, 1)];
    data.rounds[0] = vec![btd_data::v1::SpawnGroup {
        kind: ceramic as u8,
        flags: Default::default(),
        count: 2,
        start: 0.0,
        end: 30.0,
    }];
    let p = btd_env::RoundPreview::new(&data, 1);
    let mut out = vec![0.0; btd_env::RoundPreview::len(1)];
    p.write(0, &mut out);
    let cat = |n: &str| CATEGORIES.iter().position(|c| *c == n).unwrap();
    assert!(out[cat("fortified")] > 0.0 && out[cat("ceramic")] > 0.0);
    assert!(out[cat("lead")] > 0.0, "Nachkommen zählen mit");
    assert_eq!(out[cat("camo")], 0.0);
    assert_eq!(out[CATEGORIES.len() + 1], 0.5, "Dauer 30 s / 60 s");
    let n = out.len();
    assert_eq!(out[n - CATEGORIES.len() + cat("fortified")], 0.0, "Runden bis Fortified = 0");
}

#[test]
fn tower_reward_only_for_placements() {
    let mut e = Env::new(spec(), EnvConfig { reward_tower: -0.5, ..Default::default() }).unwrap();
    let mut obs = vec![0.0; e.layout.obs_size];
    let mut mask = vec![false; e.layout.n_actions];
    e.reset(&mut obs);
    e.action_mask(&mut mask);
    let place = (e.layout.a_place..e.layout.a_upgrade).find(|&a| mask[a]).unwrap();
    let info = e.step(place, &mut obs);
    assert_eq!(info.reward, -0.5, "Bauen kostet reward_tower");
    e.action_mask(&mut mask);
    let upgrade = (e.layout.a_upgrade..e.layout.a_sell).find(|&a| mask[a]).unwrap();
    let info = e.step(upgrade, &mut obs);
    assert!(!info.invalid);
    assert_eq!(info.reward, 0.0, "Upgrades sind frei");
    let sell = (e.layout.a_sell..e.layout.n_actions).find(|&a| mask[a]).unwrap();
    assert_eq!(e.step(sell, &mut obs).reward, 0.0, "Verkaufen bringt die Strafe nicht zurück");
}

fn banning(max: usize, seed: u64) -> Env {
    Env::new(spec(), EnvConfig { max_banned_towers: max, seed, ..Default::default() }).unwrap()
}

#[test]
fn tower_bans_off_by_default() {
    let e = env(false);
    assert_eq!(e.layout.obs_size, 8 * 300 + 4, "ohne Sperren keine Extra-Werte (Parität)");
    assert!(e.banned().iter().all(|&b| !b));
    assert!(Env::new(spec(), EnvConfig { max_banned_towers: 4, ..Default::default() }).is_err());
}

#[test]
fn tower_bans_mask_obs_and_step() {
    let mut e = banning(2, 7);
    let l = e.layout;
    assert_eq!(l.obs_size, 8 * 300 + 4 + 4, "ein Wert 'verfügbar' je Turm");
    let (mut obs, mut mask) = (vec![0.0; l.obs_size], vec![false; l.n_actions]);
    let mut seen = [0usize; 3];
    let mut ever = [false; 4];
    for _ in 0..200 {
        e.reset(&mut obs);
        let b = e.banned().to_vec();
        let k = b.iter().filter(|&&x| x).count();
        seen[k] += 1;
        for (t, &x) in b.iter().enumerate() {
            ever[t] |= x;
            assert_eq!(obs[l.obs_size - 4 + t], if x { 0.0 } else { 1.0 });
        }
        e.action_mask(&mut mask);
        for (t, &x) in b.iter().enumerate() {
            let place = &mask[l.a_place + t * l.cells..l.a_place + (t + 1) * l.cells];
            assert_eq!(place.iter().any(|&m| m), !x, "Turm {t}: gesperrt = nicht baubar");
        }
    }
    assert!(seen.iter().all(|&n| n > 30), "0, 1 und 2 Sperren kommen vor: {seen:?}");
    assert!(ever.iter().all(|&x| x), "jeder Turm wird mal gesperrt");

    // gezielt gesperrter Turm: Bauen ist ungültig und kostet nichts
    e.reset_with_bans(&mut obs, &[true, false, false, false]);
    let cash = e.game.cash;
    let cell = (0..l.cells).find(|&c| e.game.can_place_at(0, l.center(c).0, l.center(c).1)).unwrap();
    let info = e.step(l.a_place + cell, &mut obs);
    assert!(info.invalid);
    assert_eq!((e.game.cash, e.game.towers.len()), (cash, 0));
}

#[test]
fn tower_bans_reproducible_and_differ_between_vec_envs() {
    let draws = |seed| {
        let mut e = banning(2, seed);
        let mut obs = vec![0.0; e.layout.obs_size];
        (0..20)
            .map(|_| {
                e.reset(&mut obs);
                e.banned().to_vec()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(draws(1), draws(1), "gleicher Seed, gleiche Sperren");
    assert_ne!(draws(1), draws(2));

    let cfg = EnvConfig { max_banned_towers: 2, seed: 3, ..Default::default() };
    let mut v = VecEnv::new(spec(), cfg, 8).unwrap();
    let mut obs = vec![0.0; 8 * v.layout.obs_size];
    v.reset(&mut obs);
    let n = v.layout.obs_size;
    let avail: Vec<&[f32]> = (0..8).map(|i| &obs[i * n + n - 4..(i + 1) * n]).collect();
    assert!(avail.iter().any(|a| *a != avail[0]), "Envs würfeln unterschiedlich");
}

#[test]
fn last_replay_survives_the_next_reset() {
    let mut e = env(true);
    let mut obs = vec![0.0; e.layout.obs_size];
    e.reset(&mut obs);
    while !e.step(0, &mut obs).terminated {}
    let done = e.last_replay().cloned().expect("Replay der beendeten Episode");
    e.reset(&mut obs);
    assert_eq!(e.last_replay(), Some(&done), "bleibt nach dem Auto-Reset abrufbar");
}

#[test]
fn randomness_gives_each_env_its_own_game_seed() {
    let cfg = |randomness| EnvConfig { randomness, seed: 4, ..Default::default() };
    let mut v = VecEnv::new(spec(), cfg(true), 4).unwrap();
    let mut obs = vec![0.0; 4 * v.layout.obs_size];
    v.reset(&mut obs);
    let seeds: Vec<_> = v.envs.iter().map(|e| e.game.rng_state()).collect();
    assert!(seeds.iter().all(Option::is_some));
    assert!(seeds.windows(2).all(|w| w[0] != w[1]), "{seeds:?}");
    let first = seeds[0];
    v.reset(&mut obs);
    assert_ne!(v.envs[0].game.rng_state(), first, "jede Episode ein neuer Seed");
    // gleiche Konfiguration, gleiche Seeds
    let mut w = VecEnv::new(spec(), cfg(true), 4).unwrap();
    w.reset(&mut obs);
    assert_eq!(w.envs[0].game.rng_state(), first);
    // ohne Zufall kein Seed
    let mut off = VecEnv::new(spec(), cfg(false), 2).unwrap();
    off.reset(&mut vec![0.0; 2 * off.layout.obs_size]);
    assert!(off.envs.iter().all(|e| e.game.rng_state().is_none()));
    // Replays mit Zufall gibt es (noch) nicht
    assert!(Env::new(spec(), EnvConfig { record: true, randomness: true, ..Default::default() }).is_err());
}
