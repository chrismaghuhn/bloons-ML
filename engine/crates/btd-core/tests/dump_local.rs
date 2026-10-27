//! Plausibilitätstests gegen den echten Spiel-Dump (data/local/, nur lokal vorhanden).
//! Opt-in, weil die Daten nicht im Repo liegen: `cargo test -p btd-core --test dump_local -- --ignored`
//! (vorher `tools/build_game_v1.py`).

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, Game};
use btd_data::v1::GameV1;

fn local() -> Option<GameV1> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/local/game_v1_tutorial_medium.json");
    if !path.exists() {
        eprintln!("übersprungen: {} fehlt (tools/build_game_v1.py)", path.display());
        return None;
    }
    Some(GameV1::load(path).expect("lokaler Datensatz gültig"))
}

#[test]
#[ignore = "braucht den lokalen Spiel-Dump (data/local/)"]
fn meadow_data_matches_known_game_facts() {
    let Some(d) = local() else { return };
    let bloon = |n: &str| d.bloons.iter().find(|b| b.name == n).unwrap();
    assert_eq!((bloon("Red").leak, bloon("Ceramic").leak), (1, 104));
    assert_eq!(bloon("Ceramic").hp, 10.0);
    assert!(bloon("GreenCamo").camo);
    let dart = d.towers.iter().find(|t| t.name == "dart").unwrap();
    assert_eq!(dart.cost, 200);
    let v = dart.variant([0, 0, 0]).unwrap();
    assert_eq!((v.range, v.attacks[0].pierce, v.attacks[0].cooldown), (32.0, 2, 0.95));
    assert_eq!(d.rounds.len(), 40);
    assert_eq!(d.rounds[0][0].count, 20, "Runde 1: 20 Reds");
}

/// Zellen nach Anzahl Pfadzellen in Reichweite (wie `btd-tools demo`).
fn best_cells(d: &GameV1, range: f64) -> Vec<(f64, f64)> {
    let g = &d.map.grid;
    let center =
        |c: usize| (g.x0 + ((c % g.w) as f64 + 0.5) * g.cell, g.y0 + ((c / g.w) as f64 + 0.5) * g.cell);
    let mut cells: Vec<(usize, usize)> = (0..g.w * g.h)
        .map(|c| {
            let (x, y) = center(c);
            (
                c,
                (0..g.w * g.h)
                    .filter(|&o| g.path[o] && (center(o).0 - x).hypot(center(o).1 - y) <= range)
                    .count(),
            )
        })
        .collect();
    cells.sort_by_key(|c| std::cmp::Reverse(c.1));
    cells.into_iter().map(|(c, _)| center(c)).collect()
}

#[test]
#[ignore = "braucht den lokalen Spiel-Dump (data/local/)"]
fn darts_alone_hold_until_the_lead_round() {
    let Some(d) = local() else { return };
    let cells = best_cells(&d, 32.0);
    let mut g = Game::new(Arc::new(d)).unwrap();
    let dart = g.data().towers.iter().position(|t| t.name == "dart").unwrap() as u8;
    while g.round < 28 && !g.over() {
        if g.towers.len() < 8 {
            for &(x, y) in &cells {
                if g.apply(Action::Place { kind: dart, x, y }).is_ok() {
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
        let lives = g.lives;
        g.play_round();
        if g.round <= 27 {
            assert_eq!(g.lives, lives, "Runde {}: Darts sollten halten", g.round);
        }
    }
    // Runde 28: 6 Leads, Darts (spitz) können sie nicht platzen -> 6 x 23 RBE
    assert!(g.lives <= 150 - 6 * 23, "Leads müssen durchkommen, Leben {}", g.lives);
}

/// Mortar (docs/design-towers-step3.md, Teil 1a) auf den t21-Daten: fester Zielpunkt auf der Strecke, ein
/// Mortar irgendwo auf der Karte platzt Bloons dort.
#[test]
#[ignore = "braucht den lokalen Spiel-Dump (data/local/)"]
fn mortar_hits_its_fixed_track_point() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data/local/game_v1_tutorial_medium60_t21.json");
    if !path.exists() {
        eprintln!("übersprungen: {} fehlt (tools/build_game_v1.py --towers t21)", path.display());
        return;
    }
    let d = GameV1::load(path).expect("lokaler Datensatz gültig");
    let mut g = Game::new(Arc::new(d)).unwrap();
    let points = g.track_best_points().to_vec();
    assert!(!points.is_empty());
    for (r, (x, y)) in &points {
        eprintln!("Zielpunkt Radius {r}: ({x:.1}, {y:.1})");
        assert!((-150.0..=150.0).contains(x) && (-116.0..=116.0).contains(y));
    }
    let mortar = g.data().towers.iter().position(|t| t.name == "mortar").unwrap() as u8;
    let spot = best_cells(g.data(), 1.0).into_iter().rev().find(|&(x, y)| g.can_place_at(mortar, x, y));
    let (x, y) = spot.expect("ein Bauplatz");
    g.cash = btd_core::Money::from_dollars(10_000);
    g.apply(Action::Place { kind: mortar, x, y }).unwrap();
    let r = g.play_round();
    eprintln!("Runde 1 mit Mortar bei ({x:.0}, {y:.0}): {} Pops, {} durch", r.pops, r.leaked);
    assert!(r.pops > 0, "Mortar trifft am Zielpunkt");
}

/// Alchemist 4-0-1 (Teil 1b) neben einem Dart auf den t21-Daten: Tränke und Säure laufen ohne Fehler, und mit
/// Dip und Brew lässt der Dart weniger durch als ohne (gleiche Aufstellung, Alchemist 1-0-1 mit denselben
/// Spritzern, aber ohne Tränke).
#[test]
#[ignore = "braucht den lokalen Spiel-Dump (data/local/)"]
fn alchemist_brew_helps_darts() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data/local/game_v1_tutorial_medium60_t21.json");
    if !path.exists() {
        eprintln!("übersprungen: {} fehlt (tools/build_game_v1.py --towers t21)", path.display());
        return;
    }
    let d = Arc::new(GameV1::load(path).expect("lokaler Datensatz gültig"));
    let cells = best_cells(&d, 32.0);
    let run = |top: u8| {
        let mut g = Game::new(Arc::clone(&d)).unwrap();
        g.cash = btd_core::Money::from_dollars(100_000);
        let kind = |n: &str| g.data().towers.iter().position(|t| t.name == n).unwrap() as u8;
        let (dart, alch) = (kind("dart"), kind("alchemist"));
        let (dx, dy) =
            *cells.iter().find(|&&(x, y)| g.apply(Action::Place { kind: dart, x, y }).is_ok()).unwrap();
        // Alchemist so nah wie möglich am Dart (Trank-Reichweite 45)
        let mut near = cells.clone();
        near.sort_by(|p, q| (p.0 - dx).hypot(p.1 - dy).total_cmp(&(q.0 - dx).hypot(q.1 - dy)));
        let a = near
            .iter()
            .find_map(|&(x, y)| g.apply(Action::Place { kind: alch, x, y }).ok())
            .unwrap()
            .unwrap();
        let (ax, ay) = g.tower(a).unwrap().pos();
        assert!((ax - dx).hypot(ay - dy) < 45.0, "Dart in Trank-Reichweite");
        for _ in 0..top {
            g.apply(Action::Upgrade { tower: a, path: 0 }).unwrap();
        }
        g.apply(Action::Upgrade { tower: a, path: 2 }).unwrap();
        (0..20).map(|_| g.play_round().leaked).sum::<i32>()
    };
    let (without, with) = (run(1), run(4));
    eprintln!("Durchgelassen in 20 Runden: Alchemist 1-0-1 {without}, 4-0-1 {with}");
    assert!(with < without);
}
