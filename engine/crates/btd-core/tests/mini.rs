//! Engine-Verhalten auf der synthetischen Fixture `data/fixtures/game_v1_mini.json`
//! (3 Upgrade-Pfade, Polygon-Platzierung, Camo, mehr-HP-Bloons, 60 Ticks/s). Läuft in der CI,
//! wo der echte Spiel-Dump nicht vorhanden ist.

use std::path::PathBuf;
use std::sync::Arc;

use btd_core::{Action, ActionError, Game, Money};
use btd_data::v1::GameV1;

fn data() -> GameV1 {
    GameV1::load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/fixtures/game_v1_mini.json"))
        .unwrap()
}

fn game() -> Game {
    Game::new(Arc::new(data())).unwrap()
}

const PIN: u8 = 0;
const BOMB: u8 = 1;

#[test]
fn polygon_placement() {
    let g = game();
    assert!(g.can_place_at(PIN, 0.0, 20.0), "Land");
    assert!(!g.can_place_at(PIN, 0.0, 2.0), "Track");
    assert!(!g.can_place_at(PIN, 40.0, 30.0), "gesperrte Fläche");
    assert!(!g.can_place_at(PIN, 48.0, -20.0), "Footprint ragt über den Rand");
}

#[test]
fn three_paths_with_crosspath_limit_from_variants() {
    let mut g = game();
    g.cash = Money::from_dollars(10_000);
    let id = g.apply(Action::Place { kind: PIN, x: 0.0, y: 20.0 }).unwrap().unwrap();
    g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
    g.apply(Action::Upgrade { tower: id, path: 2 }).unwrap();
    assert_eq!(
        g.apply(Action::Upgrade { tower: id, path: 1 }),
        Err(ActionError::UpgradeUnavailable),
        "1-1-1"
    );
    g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
    assert_eq!(g.tower(id).unwrap().tiers, [2, 0, 1]);
    assert_eq!(g.tower(id).unwrap().attacks[0].pierce, 6);
    assert_eq!(g.tower(id).unwrap().spent, Money::from_dollars(100 + 50 + 40 + 100));
}

fn play_three_rounds(build: impl Fn(&mut Game)) -> (Game, Vec<btd_core::RoundReport>) {
    let mut g = game();
    build(&mut g);
    let reports = (0..3).map(|_| g.play_round()).collect();
    (g, reports)
}

#[test]
fn camo_needs_detection_and_lead_needs_the_right_damage() {
    // Basis-Pin: kein Camo, kann kein Lead
    let (_, base) = play_three_rounds(|g| {
        g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
    });
    // Pin mit Camo (Pfad 2) plus Bomb gegen Lead
    let (_, strong) = play_three_rounds(|g| {
        g.cash = Money::from_dollars(10_000);
        let id = g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap().unwrap();
        g.apply(Action::Upgrade { tower: id, path: 1 }).unwrap();
        g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
        g.apply(Action::Upgrade { tower: id, path: 0 }).unwrap();
        g.apply(Action::Place { kind: BOMB, x: -20.0, y: -15.0 }).unwrap();
    });
    assert!(base[1].leaked >= 3, "Camo-Reds kommen am Basis-Pin vorbei: {:?}", base[1]);
    assert_eq!(strong[1].leaked, 0, "{:?}", strong[1]);
    assert!(base[2].leaked > strong[2].leaked, "Lead/Ceramic: {:?} vs {:?}", base[2], strong[2]);
}

#[test]
fn ceramic_takes_several_hits_before_it_pops() {
    // Ceramic (10 HP) ist ~2,4 s in Reichweite, der Pin schießt alle 0,5 s: rund 5 Treffer
    let run = |damage_upgrade: bool| {
        let mut d = data();
        d.rounds[0] = vec![btd_data::v1::SpawnGroup {
            kind: 3,
            flags: Default::default(),
            count: 1,
            start: 0.0,
            end: 0.0,
        }];
        let mut g = Game::new(Arc::new(d)).unwrap();
        g.cash = Money::from_dollars(1_000);
        let id = g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap().unwrap();
        if damage_upgrade {
            g.apply(Action::Upgrade { tower: id, path: 1 }).unwrap(); // 2 Schaden
        }
        g.play_round()
    };
    let weak = run(false);
    assert_eq!((weak.pops, weak.leaked), (0, 15), "5 Treffer à 1 reichen nicht: {weak:?}");
    let strong = run(true);
    assert!(strong.pops >= 1, "5 Treffer à 2 = 10 HP: {strong:?}");
    assert!(strong.leaked < 15, "{strong:?}");
}

#[test]
fn deterministic_and_reset_clean() {
    let run = || {
        play_three_rounds(|g| {
            g.cash = Money::from_dollars(2_000);
            g.apply(Action::Place { kind: PIN, x: -10.0, y: 15.0 }).unwrap();
            g.apply(Action::Place { kind: BOMB, x: 10.0, y: -15.0 }).unwrap();
        })
    };
    let (a, ra) = run();
    let (b, rb) = run();
    assert_eq!(ra, rb);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn splash_without_camo_detection_never_fires_at_pure_camo_group() {
    let mut d = data();
    d.rounds[0] = vec![btd_data::v1::SpawnGroup {
        kind: 0,
        flags: btd_data::v1::Flags(btd_data::v1::Flags::CAMO),
        count: 5,
        start: 0.0,
        end: 1.0,
    }];
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind: BOMB, x: 0.0, y: -15.0 }).unwrap();
    let r = g.play_round();
    assert_eq!((r.pops, r.leaked), (0, 5));
}

const DART: u8 = 2;
const TACK: u8 = 3;
const BOMBP: u8 = 4;

fn one_round(groups: Vec<btd_data::v1::SpawnGroup>, build: impl Fn(&mut Game)) -> btd_core::RoundReport {
    let mut d = data();
    d.rounds[0] = groups;
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(10_000);
    build(&mut g);
    g.play_round()
}

fn group(kind: u8, count: u32, start: f64, end: f64) -> btd_data::v1::SpawnGroup {
    btd_data::v1::SpawnGroup { kind, flags: Default::default(), count, start, end }
}

#[test]
fn projectile_dart_pops_reds() {
    let r = one_round(vec![group(0, 5, 0.0, 4.0)], |g| {
        g.apply(Action::Place { kind: DART, x: 0.0, y: 15.0 }).unwrap();
    });
    assert_eq!((r.pops, r.leaked), (5, 0), "{r:?}");
}

/// Nur ein einziger Schuss pro Runde (riesiger Cooldown), damit jede Wirkung einem Projektil zuzuordnen ist.
fn single_shot(mut d: GameV1, kind: u8, x: f64, y: f64) -> btd_core::RoundReport {
    for v in &mut d.towers[kind as usize].variants {
        for a in &mut v.attacks {
            a.cooldown = 1000.0;
        }
    }
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind, x, y }).unwrap();
    g.play_round()
}

#[test]
fn projectile_does_not_hit_children_of_bloons_it_popped() {
    // Dart mit Pierce 2 gegen einen Blue: das Kind (Red) gilt als schon getroffen und kommt durch
    let mut d = data();
    d.rounds[0] = vec![group(1, 1, 0.0, 0.0)];
    let r = single_shot(d, DART, 0.0, 15.0);
    assert_eq!((r.pops, r.leaked), (1, 1), "{r:?}");
}

#[test]
fn projectile_hits_each_bloon_at_most_once() {
    // ein Dart (Pierce 2, 1 Schaden) bleibt an einer Ceramic (10 HP) nicht "kleben"
    let mut d = data();
    d.rounds[0] = vec![group(3, 1, 0.0, 0.0)];
    let r = single_shot(d, DART, 0.0, 15.0);
    assert_eq!((r.pops, r.leaked), (0, 15), "{r:?}");
}

#[test]
fn explosion_and_direct_damage_do_not_pop_twice() {
    // Bomb-Projektil mit eigenem Schaden UND Explosion gegen einen Blue (2 Schichten)
    let mut d = data();
    d.rounds[0] = vec![group(1, 1, 0.0, 0.0)];
    d.towers[BOMBP as usize].variants[0].attacks[0].damage = 1.0;
    let r = single_shot(d, BOMBP, 0.0, 18.0);
    // die Explosion platzt den Blue; sein Kind entsteht danach und bleibt unberührt, der
    // Direktschaden auf den schon geplatzten Blue entfällt (vor dem Fix: 5 Pops)
    assert_eq!((r.pops, r.leaked), (1, 1), "{r:?}");
}

#[test]
fn projectile_lifespan_limits_its_reach() {
    let mut d = data();
    d.rounds[0] = vec![group(0, 3, 0.0, 2.0)];
    // Lebensdauer so kurz, dass der Dart die Bloons nie erreicht
    if let btd_data::v1::Delivery::Projectile { lifespan, .. } =
        &mut d.towers[2].variants[0].attacks[0].delivery
    {
        *lifespan = 0.01;
    }
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind: DART, x: 0.0, y: 25.0 }).unwrap();
    let r = g.play_round();
    assert_eq!((r.pops, r.leaked), (0, 3), "{r:?}");
}

#[test]
fn tack_fires_in_fixed_directions() {
    // Tack 13 Einheiten über dem Track (Track verläuft in x-Richtung, y = 0, y zeigt nach unten)
    let run = |count: u32| {
        let mut d = data();
        d.rounds[0] = vec![group(0, 10, 0.0, 9.0)];
        if let btd_data::v1::Delivery::Projectile { emission, .. } =
            &mut d.towers[3].variants[0].attacks[0].delivery
        {
            *emission = btd_data::v1::Emission::Arc { count, angle: 360.0, offset: 0.0, fixed: true };
        }
        let mut g = Game::new(Arc::new(d)).unwrap();
        g.apply(Action::Place { kind: TACK, x: 0.0, y: -13.0 }).unwrap();
        g.play_round()
    };
    let one = run(1); // eine Nadel fest Richtung +x: fliegt am Track vorbei
    assert_eq!(one.pops, 0, "{one:?}");
    let eight = run(8); // u.a. eine Nadel Richtung +y, also zum Track
    assert_eq!((eight.pops, eight.leaked), (10, 0), "{eight:?}");
}

#[test]
fn bomb_projectile_explodes_on_contact() {
    // dichte Gruppe Leads: Darts sind wirkungslos, die Explosion (ohne Lead-Immunität) platzt sie
    let r = one_round(vec![group(2, 6, 0.0, 0.5)], |g| {
        g.apply(Action::Place { kind: BOMBP, x: 0.0, y: 18.0 }).unwrap();
    });
    assert!(r.pops >= 6, "Explosion trifft mehrere Leads auf einmal: {r:?}");
    let darts = one_round(vec![group(2, 6, 0.0, 0.5)], |g| {
        g.apply(Action::Place { kind: DART, x: 0.0, y: 15.0 }).unwrap();
    });
    assert_eq!(darts.pops, 0, "Darts können kein Lead: {darts:?}");
}

const FRAG: u8 = 5;

#[test]
fn shards_spawn_at_contact_and_pop_more_bloons() {
    // der Hauptschuss macht keinen Schaden; nur die 8 Splitter ab der Kontaktstelle platzen Bloons
    let r = one_round(vec![group(0, 12, 0.0, 1.0)], |g| {
        g.apply(Action::Place { kind: FRAG, x: 0.0, y: 18.0 }).unwrap();
    });
    assert!(r.pops >= 2, "mehrere Splitter treffen: {r:?}");
    let mut d = data();
    d.rounds[0] = vec![group(0, 12, 0.0, 1.0)];
    if let btd_data::v1::Delivery::Projectile { on_contact, .. } =
        &mut d.towers[5].variants[0].attacks[0].delivery
    {
        on_contact.clear();
    }
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind: FRAG, x: 0.0, y: 18.0 }).unwrap();
    assert_eq!(g.play_round().pops, 0, "ohne Splitter kein Schaden");
}

const ICE: u8 = 6;
const GLUE: u8 = 7;
const BOOM: u8 = 8;

#[test]
fn ice_freezes_and_stalls_bloons() {
    // ein Blue: Ice platzt ihn (1 Schaden) zu einem Red, der 1,5 s eingefroren stehen bleibt
    let mut d = data();
    d.rounds[0] = vec![group(1, 1, 0.0, 0.0)];
    let base = Game::new(Arc::new(d.clone())).unwrap().play_round();
    let iced = single_shot(d, ICE, -40.0, 14.0);
    assert_eq!((iced.pops, iced.leaked), (1, 1), "{iced:?}");
    // der Red ist langsamer als der Blue und steht zusätzlich 1,5 s
    assert!(iced.seconds > base.seconds + 1.5, "{iced:?} vs {base:?}");
}

#[test]
fn ice_cannot_damage_lead_and_skips_it() {
    let r = one_round(vec![group(2, 3, 0.0, 0.5)], |g| {
        g.apply(Action::Place { kind: ICE, x: -40.0, y: 14.0 }).unwrap();
    });
    assert_eq!(r.pops, 0, "Lead ist für Ice immun und wird gar nicht erst eingefroren: {r:?}");
}

#[test]
fn glue_slows_without_damage() {
    let base = one_round(vec![group(0, 3, 0.0, 0.5)], |_| {});
    let glued = one_round(vec![group(0, 3, 0.0, 0.5)], |g| {
        g.apply(Action::Place { kind: GLUE, x: -40.0, y: 15.0 }).unwrap();
    });
    assert_eq!(glued.pops, 0, "Glue macht keinen Schaden");
    assert_eq!(glued.leaked, base.leaked);
    assert!(glued.seconds > base.seconds + 1.0, "verklebte Bloons sind langsamer: {glued:?} vs {base:?}");
}

#[test]
fn glue_filter_excludes_tagged_bloons() {
    let base = one_round(vec![group(3, 1, 0.0, 0.0)], |_| {});
    let r = one_round(vec![group(3, 1, 0.0, 0.0)], |g| {
        g.apply(Action::Place { kind: GLUE, x: -40.0, y: 15.0 }).unwrap();
    });
    assert_eq!(r.seconds, base.seconds, "Ceramic ist per Tag ausgeschlossen");
}

#[test]
fn boomerang_follows_its_curve() {
    let r = one_round(vec![group(0, 8, 0.0, 6.0)], |g| {
        g.apply(Action::Place { kind: BOOM, x: 0.0, y: -20.0 }).unwrap();
    });
    assert!(r.pops > 0, "Kurve führt über den Track: {r:?}");
    // ohne Kurve (Punkt-Bahn) trifft nichts
    let mut d = data();
    d.rounds[0] = vec![group(0, 8, 0.0, 6.0)];
    if let btd_data::v1::Delivery::Projectile { path, .. } = &mut d.towers[8].variants[0].attacks[0].delivery
    {
        *path = Some(vec![[0.0, 0.0], [0.0, 1.0]]);
    }
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind: BOOM, x: 0.0, y: -20.0 }).unwrap();
    assert_eq!(g.play_round().pops, 0);
}

#[test]
fn bonus_damage_against_tags() {
    // Ceramic (10 HP), Pin mit 1 Schaden: erst mit +9 gegen "Ceramic" reicht ein Treffer
    let run = |bonus: f64| {
        let mut d = data();
        d.rounds[0] = vec![group(3, 1, 0.0, 0.0)];
        d.towers[0].variants[0].attacks[0].extras.damage_vs =
            vec![btd_data::v1::Bonus { tag: "Ceramic".into(), add: bonus }];
        let mut g = Game::new(Arc::new(d)).unwrap();
        g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
        g.play_round()
    };
    assert_eq!(run(0.0).pops, 0);
    assert!(run(9.0).pops >= 1);
}

#[test]
fn shards_can_inherit_the_hit_set() {
    // ein Red: der Bomb-Hauptschuss macht keinen Schaden; erbt der Splitter das Treffer-Set,
    // darf er den Kontakt-Bloon nicht treffen (Spiel: passOnCollidedWith)
    let run = |inherit: bool| {
        let mut d = data();
        d.rounds[0] = vec![group(0, 1, 0.0, 0.0)];
        if let btd_data::v1::Delivery::Projectile { on_contact, .. } =
            &mut d.towers[FRAG as usize].variants[0].attacks[0].delivery
        {
            if let btd_data::v1::Contact::Shards { inherit_hit, .. } = &mut on_contact[0] {
                *inherit_hit = inherit;
            }
        }
        single_shot(d, FRAG, 0.0, 18.0)
    };
    assert_eq!(run(false).pops, 1);
    assert_eq!(run(true).pops, 0);
}
