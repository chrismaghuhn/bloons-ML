//! Plausibilitätstests gegen den echten Spiel-Dump (data/local/, nur lokal vorhanden).
//! Ohne `data/local/game_v1_tutorial_medium.json` (tools/build_game_v1.py) werden sie übersprungen.

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
