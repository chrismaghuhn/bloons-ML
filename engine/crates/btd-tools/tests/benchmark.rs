//! Benchmark (a) aus docs/m2-plan.md auf den echten Spieldaten (nur lokal, der Dump ist nicht im
//! Repo): der Bau-Plan `src/meadow_plan.txt` gewinnt Monkey Meadow Medium bis Runde 40.
//!
//!     cargo test -p btd-tools --release --test benchmark -- --ignored

use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "braucht data/local/game_v1_tutorial_medium.json (tools/build_game_v1.py)"]
fn meadow_plan_wins_medium_round_40() {
    let data =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/local/game_v1_tutorial_medium.json");
    assert!(data.exists(), "{} fehlt: erst tools/build_game_v1.py laufen lassen", data.display());
    let out =
        Command::new(env!("CARGO_BIN_EXE_btd-tools")).args(["script", "--data"]).arg(&data).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("Gewonnen, alle 40 Runden"), "{stdout}");
}
