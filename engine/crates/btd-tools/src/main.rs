//! Kommandozeilen-Werkzeuge (reines Rust, kein Python).
//!
//!     btd-tools validate-data [DATEN]
//!     btd-tools replay DATEI.jsonl [--data DATEN] [--compare]
//!     btd-tools bench [--data DATEN] [--envs N]
//!
//! DATEN ist standardmäßig `data/sim_v0.json` (relativ zum Repo-Root).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use btd_env::replay::RoundResult;
use btd_env::{Env, EnvConfig, EnvSpec, Replay, VecEnv, VecStepBuffers};

const USAGE: &str = "Nutzung:
  btd-tools validate-data [DATEN]
  btd-tools replay DATEI.jsonl [--data DATEN] [--compare]
  btd-tools bench [--data DATEN] [--envs N]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("validate-data") => validate(&args[1..]),
        Some("replay") => replay(&args[1..]),
        Some("bench") => bench(&args[1..]),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn default_data() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.json")
}

/// Wert hinter `--name`, falls vorhanden.
fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn load_spec(args: &[String]) -> Result<EnvSpec, String> {
    let path = opt(args, "--data").map(PathBuf::from).unwrap_or_else(default_data);
    EnvSpec::load(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn validate(args: &[String]) -> Result<(), String> {
    let path = args.first().map(PathBuf::from).unwrap_or_else(default_data);
    let spec = EnvSpec::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let d = &spec.data;
    println!(
        "{}: gültig ({} Runden, {} Türme, {} Bloons, data_hash {:016x})",
        path.display(),
        d.rounds.len(),
        d.towers.len(),
        d.bloons.len(),
        spec.data_hash
    );
    Ok(())
}

fn fmt_opt<T: std::fmt::Display>(v: &Option<T>) -> String {
    v.as_ref().map_or("-".into(), |v| v.to_string())
}

fn replay(args: &[String]) -> Result<(), String> {
    let file = args.first().filter(|a| !a.starts_with("--")).ok_or(USAGE)?;
    let spec = load_spec(args)?;
    let replay = Replay::load(file).map_err(|e| e.to_string())?;
    let show = |got: &RoundResult, expected: &RoundResult, ok: bool| {
        format!(
            "Leben {:>4} (erw. {:>4})  Cash {:>6} (erw. {:>6})  Pops {:>5}  {}",
            fmt_opt(&got.lives),
            fmt_opt(&expected.lives),
            fmt_opt(&got.cash),
            fmt_opt(&expected.cash),
            fmt_opt(&got.pops),
            if ok { "ok" } else { "ABWEICHUNG" }
        )
    };
    if args.iter().any(|a| a == "--compare") {
        let checks = replay.compare(&spec).map_err(|e| e.to_string())?;
        for c in &checks {
            println!("Runde {:>3}: {}", c.round, show(&c.got, &c.expected, c.matches));
        }
        let bad = checks.iter().filter(|c| !c.matches).count();
        println!("{} Runden, {bad} mit Abweichung", checks.len());
    } else {
        let game = replay
            .play(&spec, None, |c| println!("Runde {:>3}: {}", c.round, show(&c.got, &c.expected, c.matches)))
            .map_err(|e| e.to_string())?;
        println!(
            "Replay stimmt. Endstand: Runde {}, Leben {}, Cash {}",
            game.round,
            game.lives,
            game.cash.dollars()
        );
    }
    Ok(())
}

/// Kleiner deterministischer Zufallsgenerator für die Bench-Policy.
struct Rng(u64);
impl Rng {
    fn pick(&mut self, mask: &[bool]) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        let n = mask.iter().filter(|&&m| m).count();
        let k = (self.0 % n as u64) as usize;
        mask.iter().enumerate().filter(|(_, &m)| m).nth(k).unwrap().0
    }
}

fn bench(args: &[String]) -> Result<(), String> {
    let spec = load_spec(args)?;
    let n_envs: usize = opt(args, "--envs").map_or(Ok(16), str::parse).map_err(|e| format!("--envs: {e}"))?;
    println!("Build: {}", btd_env::replay::FLOAT);

    // 1) Eine Env, zufällige gültige Aktionen: Aktionen/s, Runden/s, simulierte Ticks/s
    let mut env = Env::new(spec.clone(), EnvConfig::default()).map_err(|e| e.to_string())?;
    let (mut obs, mut mask) = (vec![0.0; env.layout.obs_size], vec![false; env.layout.n_actions]);
    let mut rng = Rng(42);
    let (mut steps, mut rounds, mut ticks, mut episodes) = (0u64, 0u64, 0u64, 0u64);
    let (mut build_time, mut round_time) = (0.0f64, 0.0f64);
    env.reset(&mut obs);
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < 3.0 {
        env.action_mask(&mut mask);
        let a = rng.pick(&mask);
        let t0 = Instant::now();
        let info = env.step(a, &mut obs);
        let dt = t0.elapsed().as_secs_f64();
        steps += 1;
        match info.round_played {
            Some(r) => {
                rounds += 1;
                ticks += r.ticks as u64;
                round_time += dt;
            }
            None => build_time += dt,
        }
        if info.terminated {
            episodes += 1;
            env.reset(&mut obs);
        }
    }
    let total = start.elapsed().as_secs_f64();
    println!("1 Env, Zufallspolicy ({total:.1} s, {episodes} Spiele):");
    println!("  Env-Schritte/s gesamt    {:>12.0}", steps as f64 / total);
    println!("  Bau-Aktionen/s (+Maske)  {:>12.0}", (steps - rounds) as f64 / build_time.max(1e-9));
    println!("  Runden/s                 {:>12.0}", rounds as f64 / round_time.max(1e-9));
    println!("  simulierte Ticks/s       {:>12.0}", ticks as f64 / round_time.max(1e-9));

    // 2) Vektor-Env mit rayon: Env-Schritte/s über alle Kerne
    let mut v = VecEnv::new(spec, EnvConfig::default(), n_envs).map_err(|e| e.to_string())?;
    let (no, na) = (v.layout.obs_size, v.layout.n_actions);
    let (mut obs, mut tobs) = (vec![0.0; n_envs * no], vec![0.0; n_envs * no]);
    let (mut rew, mut done, mut rnd) = (vec![0.0; n_envs], vec![false; n_envs], vec![0; n_envs]);
    let mut masks = vec![false; n_envs * na];
    let mut actions = vec![0usize; n_envs];
    let mut rngs: Vec<Rng> = (0..n_envs as u64).map(|i| Rng(1000 + i)).collect();
    v.reset(&mut obs);
    let (start, mut steps) = (Instant::now(), 0u64);
    while start.elapsed().as_secs_f64() < 3.0 {
        v.action_masks(&mut masks);
        for (i, a) in actions.iter_mut().enumerate() {
            *a = rngs[i].pick(&masks[i * na..(i + 1) * na]);
        }
        v.step(
            &actions,
            VecStepBuffers {
                obs: &mut obs,
                rewards: &mut rew,
                dones: &mut done,
                terminal_obs: &mut tobs,
                rounds: &mut rnd,
            },
        );
        steps += n_envs as u64;
    }
    let total = start.elapsed().as_secs_f64();
    println!("VecEnv mit {n_envs} Envs (rayon, {} Threads):", rayon::current_num_threads());
    println!("  Env-Schritte/s gesamt    {:>12.0}", steps as f64 / total);
    Ok(())
}
