use std::path::PathBuf;

use btd_core::Action;
use btd_env::replay::{ReplayAction, RoundResult};
use btd_env::{Env, EnvConfig, EnvSpec, Replay, ReplayError, VecEnv, VecStepBuffers};

fn spec() -> EnvSpec {
    EnvSpec::load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.json")).unwrap()
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
    assert_eq!(l.decode(1 + 3 * 300 + 4 * 20 + 7), Some(Action::Place { kind: 3, x: 7, y: 4 }));
    assert_eq!(l.decode(l.a_upgrade + 300 + 5), Some(Action::Upgrade { x: 5, y: 0, path: 1 }));
    assert_eq!(l.decode(l.a_sell + 21), Some(Action::Sell { x: 1, y: 1 }));
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
            l.a_place + k * l.cells + *y as usize * l.grid_w + *x as usize
        }
        ReplayAction::Upgrade { x, y, path } => {
            l.a_upgrade + *path as usize * l.cells + *y as usize * l.grid_w + *x as usize
        }
        ReplayAction::Sell { x, y } => l.a_sell + *y as usize * l.grid_w + *x as usize,
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
{"type":"header","format":"btd-replay/1","data":"sim_v0.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7,"y":4}}],"result":{"lives":150}}
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
        ("{\"type\":\"header\",\"format\":\"btd-replay/1\",\"data\":\"d\"}\n{\"type\":\"round\",\"round\":2}", "erste Runde"),
    ] {
        let err = Replay::from_jsonl(text).unwrap_err().to_string();
        assert!(err.contains(needle), "{err} enthält nicht {needle}");
    }
    let bad_action = "{\"type\":\"header\",\"format\":\"btd-replay/1\",\"data\":\"d\"}\n\
        {\"type\":\"round\",\"round\":1,\"actions\":[{\"place\":{\"tower\":\"dart\",\"x\":0,\"y\":5}}]}";
    let r = Replay::from_jsonl(bad_action).unwrap();
    // strikt: Abbruch; nicht-strikt: Aktion übersprungen und vermerkt
    assert!(matches!(r.play(&spec(), None, |_| {}), Err(ReplayError::Action { round: 1, .. })));
    let checks = r.compare(&spec()).unwrap();
    assert_eq!(checks.len(), 1);
    assert!(!checks[0].matches && checks[0].action_errors[0].contains("NotBuildable"));
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
{"type":"header","format":"btd-replay/1","data":"sim_v0.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7,"y":4}},{"place":{"tower":"bomb","x":8,"y":4}}]}
{"type":"round","round":2,"actions":[{"place":{"tower":"dart","x":5,"y":6}}],"result":{"lives":150,"round":2}}
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
{"type":"header","format":"btd-replay/1","data":"sim_v0.json"}
{"type":"round","round":1,"actions":[{"place":{"tower":"dart","x":7,"y":4}},{"place":{"tower":"bomb","x":8,"y":4}}],"result":{"cash":9999}}
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
