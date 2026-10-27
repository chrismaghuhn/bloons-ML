//! Kommandozeilen-Werkzeuge (reines Rust, kein Python).
//!
//!     btd-tools validate-data [DATEN]
//!     btd-tools replay DATEI.jsonl [--data DATEN] [--compare]
//!     btd-tools bench [--data DATEN] [--envs N]
//!     btd-tools demo [--data DATEN] [--tower NAME] [--max N]
//!     btd-tools script [--data DATEN] [--plan DATEI]   (Standard-DATEN: data/local/…_medium.json)
//!
//! DATEN ist standardmäßig `data/sim_v0.game_v1.json` (relativ zum Repo-Root).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use btd_env::replay::RoundResult;
use btd_env::{Env, EnvConfig, EnvSpec, Replay, VecEnv, VecStepBuffers};

const USAGE: &str = "Nutzung:
  btd-tools validate-data [DATEN]
  btd-tools replay DATEI.jsonl [--data DATEN] [--compare]
  btd-tools bench [--data DATEN] [--envs N]
  btd-tools demo [--data DATEN] [--tower NAME] [--max N]
  btd-tools script [--data DATEN] [--plan DATEI]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("validate-data") => validate(&args[1..]),
        Some("replay") => replay(&args[1..]),
        Some("bench") => bench(&args[1..]),
        Some("demo") => demo(&args[1..]),
        Some("script") => script(&args[1..]),
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.game_v1.json")
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
            for e in &c.action_errors {
                println!("           übersprungen: {e}");
            }
        }
        let bad = checks.iter().filter(|c| !c.matches).count();
        println!("{} Runden, {bad} mit Abweichung", checks.len());
        let total = replay.rounds.len();
        if checks.len() < total {
            println!("Achtung: Das Sim-Spiel endete nach {} von {total} Runden des Protokolls", checks.len());
        }
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
    let (mut build_time, mut round_time, mut mask_time) = (0.0f64, 0.0f64, 0.0f64);
    env.reset(&mut obs);
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < 3.0 {
        let t0 = Instant::now();
        env.action_mask(&mut mask);
        mask_time += t0.elapsed().as_secs_f64();
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
    println!("  Masken/s                 {:>12.0}", steps as f64 / mask_time.max(1e-9));
    println!("  Bau-Aktionen/s           {:>12.0}", (steps - rounds) as f64 / build_time.max(1e-9));
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
    let (start, mut steps, mut env_time) = (Instant::now(), 0u64, 0.0f64);
    while start.elapsed().as_secs_f64() < 3.0 {
        let t0 = Instant::now();
        v.action_masks(&mut masks);
        env_time += t0.elapsed().as_secs_f64();
        // Zufallspolicy (seriell, gehört nicht zur Env-Zeit)
        for (i, a) in actions.iter_mut().enumerate() {
            *a = rngs[i].pick(&masks[i * na..(i + 1) * na]);
        }
        let t0 = Instant::now();
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
        env_time += t0.elapsed().as_secs_f64();
        steps += n_envs as u64;
    }
    let total = start.elapsed().as_secs_f64();
    println!("VecEnv mit {n_envs} Envs (rayon, {} Threads):", rayon::current_num_threads());
    println!("  Env-Schritte/s gesamt    {:>12.0}", steps as f64 / total);
    println!("  davon nur Env (step+mask){:>12.0}", steps as f64 / env_time.max(1e-9));
    Ok(())
}

/// Einfache Skript-Strategie zum Plausibilisieren neuer Daten: nach jeder Runde einen Turm auf
/// die bebaubare Zelle mit den meisten Pfadzellen in Reichweite setzen (bis `--max`), sonst
/// upgraden, was bezahlbar ist. Gibt pro Runde Leben, Cash, Pops und Rundendauer aus.
fn demo(args: &[String]) -> Result<(), String> {
    use btd_core::{Action, Game};
    let spec = load_spec(args)?;
    let data = spec.data.clone();
    let tower = opt(args, "--tower").unwrap_or("dart");
    let kind =
        data.towers.iter().position(|t| t.name == tower).ok_or(format!("Turm {tower:?} unbekannt"))? as u8;
    let max: usize = opt(args, "--max").map_or(Ok(8), str::parse).map_err(|e| format!("--max: {e}"))?;
    let mut g = Game::new(data.clone()).map_err(|e| e.to_string())?;
    let grid = &data.map.grid;
    let range = data.towers[kind as usize].variant([0, 0, 0]).unwrap().range;
    // Zellen nach Anzahl Pfadzellen in Reichweite sortieren
    let center = |c: usize| {
        (grid.x0 + ((c % grid.w) as f64 + 0.5) * grid.cell, grid.y0 + ((c / grid.w) as f64 + 0.5) * grid.cell)
    };
    let mut cells: Vec<(usize, usize)> = (0..grid.w * grid.h)
        .map(|c| {
            let (x, y) = center(c);
            let cover = (0..grid.w * grid.h)
                .filter(|&o| {
                    grid.path[o] && {
                        let (ox, oy) = center(o);
                        (ox - x).hypot(oy - y) <= range
                    }
                })
                .count();
            (c, cover)
        })
        .collect();
    cells.sort_by_key(|c| std::cmp::Reverse(c.1));
    let start = std::time::Instant::now();
    while !g.over() {
        if g.towers.len() < max {
            for &(c, _) in &cells {
                let (x, y) = center(c);
                if g.apply(Action::Place { kind, x, y }).is_ok() {
                    break;
                }
            }
        }
        let ids: Vec<u32> = g.towers.iter().map(|t| t.id).collect();
        for id in ids {
            for path in [0u8, 2, 1] {
                let _ = g.apply(Action::Upgrade { tower: id, path });
            }
        }
        let played = g.round + 1;
        let r = g.play_round();
        println!(
            "Runde {:>3}: Leben {:>4}  Cash {:>6}  Pops {:>5}  {:>5.1} s  Türme {}",
            played,
            g.lives,
            g.cash.dollars(),
            r.pops,
            r.seconds,
            g.towers
                .iter()
                .map(|t| format!("{}{}{}", t.tiers[0], t.tiers[1], t.tiers[2]))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    let result = if g.won() {
        format!("Gewonnen, alle {} Runden", g.round)
    } else {
        format!("Verloren in Runde {}", g.round + 1)
    };
    println!("{result} ({:.2} s Rechenzeit)", start.elapsed().as_secs_f64());
    Ok(())
}

/// Bau-Plan für Monkey Meadow (Medium, Runde 1-40) aus den 6 Primary-Türmen: Benchmark (a).
/// Eine Zeile pro Schritt: `place TURM` oder `up NR PFAD` (NR = n-ter gebauter Turm, ab 0).
const MEADOW_PLAN: &str = include_str!("meadow_plan.txt");

/// Spielt einen festen Bau-Plan: vor jeder Runde so viele Schritte wie bezahlbar, in Reihenfolge
/// (ein unbezahlbarer Schritt hält den Rest auf, damit gespart wird). Anders als die RL-Env ohne
/// Limit an Bauaktionen pro Runde. Türme kommen auf die freie
/// Zelle mit den meisten Pfadzellen in Reichweite.
fn script(args: &[String]) -> Result<(), String> {
    use btd_core::{Action, ActionError, Game};
    // der eingebaute Plan ist für Monkey Meadow: Standard sind die lokalen Spieldaten
    let path = opt(args, "--data").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/local/game_v1_tutorial_medium.json")
    });
    let spec = EnvSpec::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let data = spec.data.clone();
    let plan_text = match opt(args, "--plan") {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?,
        None => MEADOW_PLAN.to_string(),
    };
    enum Step {
        Place(u8),
        Up(usize, u8),
    }
    let mut plan = Vec::new();
    for (i, line) in plan_text.lines().enumerate() {
        let line = line.split('#').next().unwrap().trim();
        let w: Vec<&str> = line.split_whitespace().collect();
        let bad = || format!("Plan Zeile {}: {line:?}", i + 1);
        match w.as_slice() {
            [] => {}
            ["place", name] => plan
                .push(Step::Place(data.towers.iter().position(|t| t.name == *name).ok_or_else(bad)? as u8)),
            ["up", n, p] => plan.push(Step::Up(n.parse().map_err(|_| bad())?, p.parse().map_err(|_| bad())?)),
            _ => return Err(bad()),
        }
    }
    let grid = &data.map.grid;
    let center = |c: usize| {
        (grid.x0 + ((c % grid.w) as f64 + 0.5) * grid.cell, grid.y0 + ((c / grid.w) as f64 + 0.5) * grid.cell)
    };
    let path_cells: Vec<(f64, f64)> = (0..grid.w * grid.h).filter(|&c| grid.path[c]).map(center).collect();
    let mut g = Game::new(data.clone()).map_err(|e| e.to_string())?;
    let mut ids: Vec<u32> = Vec::new();
    let mut next = 0;
    while !g.over() {
        while next < plan.len() {
            let res = match plan[next] {
                Step::Place(kind) => {
                    let range = data.towers[kind as usize].variant([0, 0, 0]).unwrap().range;
                    let best = (0..grid.w * grid.h)
                        .filter(|&c| {
                            let (x, y) = center(c);
                            g.can_place_at(kind, x, y)
                        })
                        .max_by_key(|&c| {
                            let (x, y) = center(c);
                            path_cells.iter().filter(|(px, py)| (px - x).hypot(py - y) <= range).count()
                        })
                        .ok_or(ActionError::NotPlaceable);
                    best.and_then(|c| {
                        let (x, y) = center(c);
                        g.apply(Action::Place { kind, x, y })
                    })
                    .map(|id| ids.push(id.unwrap()))
                }
                Step::Up(n, path) => {
                    let tower = *ids
                        .get(n)
                        .ok_or(format!("Plan-Schritt {}: Turm {n} gibt es (noch) nicht", next + 1))?;
                    g.apply(Action::Upgrade { tower, path }).map(|_| ())
                }
            };
            match res {
                Ok(()) => next += 1,
                Err(ActionError::NotEnoughCash) => break,
                Err(e) => return Err(format!("Plan-Schritt {}: {e:?}", next + 1)),
            }
        }
        let played = g.round + 1;
        let r = g.play_round();
        println!(
            "Runde {:>3}: Leben {:>4}  Cash {:>6}  Pops {:>5}  {:>5.1} s  Türme {}",
            played,
            g.lives,
            g.cash.dollars(),
            r.pops,
            r.seconds,
            g.towers
                .iter()
                .map(|t| format!(
                    "{}:{}{}{}",
                    data.towers[t.kind as usize].name, t.tiers[0], t.tiers[1], t.tiers[2]
                ))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    if g.won() {
        println!(
            "Gewonnen, alle {} Runden, {} Leben übrig (Plan-Schritte {}/{})",
            g.round,
            g.lives,
            next,
            plan.len()
        );
        Ok(())
    } else {
        Err(format!("Verloren in Runde {} (Plan-Schritte {}/{})", g.round + 1, next, plan.len()))
    }
}
