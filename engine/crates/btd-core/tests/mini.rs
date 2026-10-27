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

// ---------------------------------------------------------------- M2c: Effekte

use btd_data::v1::{
    Burst, Contact, Crit, Delivery, Dot, Emission, Freeze, Homing, Push, Slow, TagFactor, Zone,
};

fn atk(d: &mut GameV1, kind: u8) -> &mut btd_data::v1::AttackV1 {
    &mut d.towers[kind as usize].variants[0].attacks[0]
}

fn run(d: GameV1, build: impl Fn(&mut Game)) -> btd_core::RoundReport {
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.cash = Money::from_dollars(10_000);
    build(&mut g);
    g.play_round()
}

fn with_round(groups: Vec<btd_data::v1::SpawnGroup>) -> GameV1 {
    let mut d = data();
    d.rounds[0] = groups;
    d
}

fn place(kind: u8, x: f64, y: f64) -> impl Fn(&mut Game) {
    move |g: &mut Game| {
        g.apply(Action::Place { kind, x, y }).unwrap();
    }
}

fn corrosive(layers: u32) -> Dot {
    Dot { damage: 1.0, interval: 0.5, seconds: 10.0, layers, level: 1, factor_for: vec![] }
}

#[test]
fn damage_over_time_pops_and_passes_to_children() {
    // Glue macht keinen Direktschaden; Schaden über Zeit platzt den Blue und danach sein Kind
    let glue = |layers: Option<u32>| {
        let mut d = with_round(vec![group(1, 1, 0.0, 0.0)]);
        atk(&mut d, GLUE).extras.dot = layers.map(corrosive);
        run(d, place(GLUE, -40.0, 15.0))
    };
    assert_eq!(glue(None).pops, 0);
    assert_eq!((glue(Some(9)).pops, glue(Some(9)).leaked), (2, 0), "Kind erbt den Effekt");
    assert_eq!((glue(Some(1)).pops, glue(Some(1)).leaked), (1, 1), "nur eine Schicht");
}

#[test]
fn damage_over_time_factor_per_tag() {
    let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
    atk(&mut d, GLUE).extras.dot =
        Some(Dot { factor_for: vec![TagFactor { tag: "Red".into(), factor: 0.0 }], ..corrosive(9) });
    assert_eq!(run(d, place(GLUE, -40.0, 15.0)).pops, 0);
}

#[test]
fn push_back_delays_damaged_bloons() {
    // Pin gegen eine Ceramic (10 HP): jeder Treffer schiebt sie zurück
    let pin = |push: Option<Push>| {
        let mut d = with_round(vec![group(3, 1, 0.0, 0.0)]);
        atk(&mut d, PIN).extras.push = push;
        run(d, place(PIN, 0.0, 15.0))
    };
    let base = pin(None);
    let pushed = pin(Some(Push { amount: 5.0, factor_for: vec![] }));
    let immune =
        pin(Some(Push { amount: 5.0, factor_for: vec![TagFactor { tag: "Ceramic".into(), factor: 0.0 }] }));
    assert!(pushed.seconds > base.seconds + 0.5, "{pushed:?} vs {base:?}");
    assert_eq!(immune.seconds, base.seconds);
}

#[test]
fn crit_hits_on_every_nth_shot() {
    let pin = |crit: Option<Crit>| {
        let mut d = with_round(vec![group(3, 1, 0.0, 0.0)]);
        atk(&mut d, PIN).crit = crit;
        run(d, place(PIN, 0.0, 15.0))
    };
    assert_eq!(pin(None).pops, 0, "1 Schaden pro Schuss reicht nicht für 10 HP");
    assert!(pin(Some(Crit { damage: 10.0, every: 2 })).pops >= 1);
}

#[test]
fn parallel_emission_fires_several_projectiles() {
    let dart = |emission: Emission| {
        let mut d = with_round(vec![group(0, 12, 0.0, 0.6)]);
        if let Delivery::Projectile { emission: e, .. } = &mut atk(&mut d, DART).delivery {
            *e = emission;
        }
        single_shot(d, DART, 0.0, 15.0)
    };
    assert!(dart(Emission::Single {}).pops <= 2, "Pierce 2");
    assert!(dart(Emission::Parallel { count: 3, spread: 3.0 }).pops > 2);
}

#[test]
fn clear_hits_lets_the_projectile_hit_again() {
    // langsamer Dart gegen einen Blue: ohne Reset gilt das Kind als getroffen
    let dart = |clear: Option<f64>| {
        let mut d = with_round(vec![group(1, 1, 0.0, 0.0)]);
        if let Delivery::Projectile { speed, lifespan, clear_hits, .. } = &mut atk(&mut d, DART).delivery {
            (*speed, *lifespan, *clear_hits) = (60.0, 1.0, clear);
        }
        single_shot(d, DART, 0.0, 15.0)
    };
    assert_eq!((dart(None).pops, dart(None).leaked), (1, 1));
    assert_eq!((dart(Some(1.0 / 60.0)).pops, dart(Some(1.0 / 60.0)).leaked), (2, 0));
}

#[test]
fn zone_slows_bloons_in_radius() {
    // Leads (Pin kann sie nicht beschädigen) laufen durch die Zone
    let pin = |zone: Option<Zone>| {
        let mut d = with_round(vec![group(2, 1, 0.0, 0.0)]);
        d.towers[PIN as usize].variants[0].zones = zone.into_iter().collect();
        run(d, place(PIN, 0.0, 15.0))
    };
    let z = |exclude: Vec<String>| Zone {
        radius: 30.0,
        speed_scale: 0.5,
        camo: false,
        filter: btd_data::v1::HitFilter { exclude_tags: exclude, ..Default::default() },
    };
    let base = pin(None);
    assert!(pin(Some(z(vec![]))).seconds > base.seconds + 0.5);
    assert_eq!(pin(Some(z(vec!["Lead".into()]))).seconds, base.seconds, "Filter");
}

#[test]
fn shards_explode_when_their_pierce_is_used_up() {
    // Cluster: Splitter ohne Schaden, die beim Verbrauch explodieren
    let frag = |exhaust: bool| {
        let mut d = with_round(vec![group(0, 12, 0.0, 1.0)]);
        if let Delivery::Projectile { on_contact, .. } = &mut atk(&mut d, FRAG).delivery {
            if let Contact::Shards { damage, on_exhaust, .. } = &mut on_contact[0] {
                *damage = 0.0;
                if exhaust {
                    on_exhaust.push(Contact::Explosion {
                        radius: 8.0,
                        pierce: 10,
                        damage: 1.0,
                        cannot_damage: vec![],
                        camo: false,
                        extras: Default::default(),
                    });
                }
            }
        }
        single_shot(d, FRAG, 0.0, 18.0)
    };
    assert_eq!(frag(false).pops, 0);
    assert!(frag(true).pops >= 2, "{:?}", frag(true));
}

fn slow(level: u8, seconds: f64) -> Slow {
    Slow {
        id: String::new(),
        multiplier: 0.5,
        seconds,
        level,
        layers: 3,
        multiplier_for: vec![],
        seconds_for: vec![],
    }
}

#[test]
fn permafrost_and_glue_use_separate_slots() {
    // Pin ohne Schaden mit Permafrost (Slow ohne Glue-Stufe), dazu Glue mit kurzer Dauer
    let build = |perma: bool, glue: bool| {
        let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
        let a = atk(&mut d, PIN);
        a.damage = 0.0;
        a.extras.slow = perma.then(|| slow(0, 1000.0));
        atk(&mut d, GLUE).extras.slow = Some(slow(1, 1.0));
        run(d, move |g: &mut Game| {
            g.apply(Action::Place { kind: PIN, x: -40.0, y: 15.0 }).unwrap();
            if glue {
                g.apply(Action::Place { kind: GLUE, x: -40.0, y: -15.0 }).unwrap();
            }
        })
    };
    let (perma, glue, both) = (build(true, false), build(false, true), build(true, true));
    assert!(both.seconds > perma.seconds + 0.2, "Glue wirkt zusätzlich: {both:?} vs {perma:?}");
    assert!(both.seconds > glue.seconds + 0.5, "Permafrost überlebt das Glue: {both:?} vs {glue:?}");
}

/// Ice friert nur ein (kein Schaden); Reichweite 25, damit sie vom Rand aus den Spawn-Punkt erreicht.
fn ice_only_freeze(d: &mut GameV1, layers: u32, scale: f64) {
    d.towers[ICE as usize].variants[0].range = 25.0;
    let a = atk(d, ICE);
    a.damage = 0.0;
    a.extras.freeze = Some(Freeze {
        seconds: 1.5,
        layers,
        scale: vec![TagFactor { tag: "Ceramic".into(), factor: scale }],
    });
}

#[test]
fn freeze_passes_to_children_by_layers() {
    // je ein Schuss: Ice friert ein (kein Schaden), direkt danach platzt der Pin den Blue; mit
    // 2 Schichten bleibt das Kind eingefroren stehen
    let build = |layers: u32| {
        let mut d = with_round(vec![group(1, 1, 0.0, 0.0)]);
        ice_only_freeze(&mut d, layers, 1.0);
        atk(&mut d, ICE).cooldown = 1000.0;
        atk(&mut d, PIN).cooldown = 1000.0;
        run(d, |g: &mut Game| {
            // beide haben den Spawn-Punkt in Reichweite und schießen im ersten Tick, Ice zuerst
            g.apply(Action::Place { kind: ICE, x: -44.0, y: 14.0 }).unwrap();
            g.apply(Action::Place { kind: PIN, x: -44.0, y: -15.0 }).unwrap();
        })
    };
    let (one, two) = (build(1), build(2));
    assert_eq!((one.pops, one.leaked, two.pops, two.leaked), (1, 1, 1, 1));
    assert!(two.seconds > one.seconds + 1.0, "{two:?} vs {one:?}");
}

#[test]
fn freeze_duration_scales_per_tag() {
    let build = |scale: f64| {
        let mut d = with_round(vec![group(3, 1, 0.0, 0.0)]);
        ice_only_freeze(&mut d, 2, scale);
        single_shot(d, ICE, -40.0, 14.0)
    };
    let (full, half) = (build(1.0), build(0.5));
    assert!(full.seconds > half.seconds + 0.5, "{full:?} vs {half:?}");
}

#[test]
fn frozen_bloons_resist_attacks_that_cannot_hit_frozen() {
    // je ein Schuss, Ice zuerst: der Pin trifft einen eingefrorenen Red
    let build = |pin_cannot_frozen: bool| {
        let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
        ice_only_freeze(&mut d, 2, 1.0);
        atk(&mut d, ICE).cooldown = 1000.0;
        atk(&mut d, PIN).cooldown = 1000.0;
        if pin_cannot_frozen {
            atk(&mut d, PIN).cannot_damage = btd_data::v1::Props(btd_data::v1::Props::FROZEN);
        }
        run(d, |g: &mut Game| {
            // beide haben den Spawn-Punkt in Reichweite und schießen im ersten Tick, Ice zuerst
            g.apply(Action::Place { kind: ICE, x: -44.0, y: 14.0 }).unwrap();
            g.apply(Action::Place { kind: PIN, x: -44.0, y: -15.0 }).unwrap();
        })
    };
    assert_eq!(build(false).pops, 1);
    assert_eq!(build(true).pops, 0);
}

#[test]
fn ice_does_not_fire_at_bloons_it_skips() {
    // Lead vorn, Red 1 s dahinter, Ice mit langem Cooldown: wird der Lead übersprungen, bleibt
    // der Schuss für den Red übrig
    let build = |skip: bool| {
        let mut d = with_round(vec![group(2, 1, 0.0, 0.0), group(0, 1, 1.0, 1.0)]);
        let a = atk(&mut d, ICE);
        a.cooldown = 1000.0;
        if !skip {
            a.extras.filter.skip_props.clear();
        }
        run(d, place(ICE, -40.0, 14.0))
    };
    assert_eq!(build(true).pops, 1);
    assert_eq!(build(false).pops, 0, "Schuss auf den immunen Lead verbraucht den Cooldown");
}

#[test]
fn area_radius_can_differ_from_range() {
    let build = |radius: Option<f64>| {
        let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
        atk(&mut d, ICE).delivery = Delivery::Area { radius };
        run(d, place(ICE, -40.0, 14.0))
    };
    assert_eq!(build(None).pops, 1);
    assert_eq!(build(Some(5.0)).pops, 0, "Ziel in Reichweite, aber außerhalb des Radius");
}

#[test]
fn unknown_tags_are_rejected() {
    let mut d = data();
    atk(&mut d, GLUE).extras.filter.exclude_tags = vec!["Ceramics".into()];
    assert!(d.validate().is_err());
}

#[test]
fn slows_with_different_ids_coexist() {
    // Permafrost (dauerhaft 0.5) vom Pin, kurzer Stun (0.0) vom Glue: der Stun darf den
    // Permafrost nicht ersetzen und beim Ablaufen löschen
    let build = |stun: bool| {
        let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
        let a = atk(&mut d, PIN);
        a.damage = 0.0;
        a.extras.slow = Some(Slow { id: "perma".into(), ..slow(0, 1000.0) });
        atk(&mut d, GLUE).extras.slow = Some(Slow { id: "stun".into(), multiplier: 0.0, ..slow(0, 0.5) });
        run(d, move |g: &mut Game| {
            g.apply(Action::Place { kind: PIN, x: -40.0, y: 15.0 }).unwrap();
            if stun {
                g.apply(Action::Place { kind: GLUE, x: -40.0, y: -15.0 }).unwrap();
            }
        })
    };
    // gemessen: Permafrost allein 9,6 s, mit Stun 14,6 s; mit einem gemeinsamen Slot (Stun ersetzt
    // den Permafrost und löscht ihn beim Ablaufen) nur 10,4 s
    let (perma, both) = (build(false), build(true));
    assert!(both.seconds > perma.seconds + 2.0, "{both:?} vs {perma:?}");
}

#[test]
fn non_colliding_shards_explode_at_the_end_of_their_flight() {
    let frag = |exhaust: bool| {
        let mut d = with_round(vec![group(0, 12, 0.0, 1.0)]);
        if let Delivery::Projectile { on_contact, .. } = &mut atk(&mut d, FRAG).delivery {
            if let Contact::Shards { damage, collide, on_exhaust, .. } = &mut on_contact[0] {
                (*damage, *collide) = (1.0, false);
                if exhaust {
                    on_exhaust.push(Contact::Explosion {
                        radius: 10.0,
                        pierce: 10,
                        damage: 1.0,
                        cannot_damage: vec![],
                        camo: false,
                        extras: Default::default(),
                    });
                }
            }
        }
        single_shot(d, FRAG, 0.0, 18.0)
    };
    assert_eq!(frag(false).pops, 0, "Splitter ohne Kollision treffen nichts");
    assert!(frag(true).pops >= 1, "{:?}", frag(true));
}

#[test]
fn crit_frequency_matters() {
    // Pin ohne Grundschaden: nur kritische Schüsse platzen einen Red
    let pin = |every: u32| {
        let mut d = with_round(vec![group(0, 40, 0.0, 20.0)]);
        let a = atk(&mut d, PIN);
        a.damage = 0.0;
        a.crit = Some(Crit { damage: 1.0, every });
        run(d, place(PIN, 0.0, 15.0)).pops
    };
    let (two, four) = (pin(2), pin(4));
    assert!(four > 0 && two >= 2 * four - 1 && two <= 2 * four + 1, "{two} vs {four}");
}

#[test]
fn push_needs_damage() {
    // Leads sind für den Pin immun: kein Schaden, also kein Zurückstoßen
    let pin = |push: bool| {
        let mut d = with_round(vec![group(2, 1, 0.0, 0.0)]);
        atk(&mut d, PIN).extras.push = push.then(|| Push { amount: 5.0, factor_for: vec![] });
        run(d, place(PIN, 0.0, 15.0))
    };
    assert_eq!(pin(true).seconds, pin(false).seconds);
}

#[test]
fn zone_needs_camo_detection_for_camo_bloons() {
    let pin = |camo: bool| {
        let mut d = with_round(vec![btd_data::v1::SpawnGroup {
            flags: btd_data::v1::Flags(btd_data::v1::Flags::CAMO),
            ..group(2, 1, 0.0, 0.0)
        }]);
        d.towers[PIN as usize].variants[0].zones =
            vec![Zone { radius: 30.0, speed_scale: 0.5, camo, filter: Default::default() }];
        run(d, place(PIN, 0.0, 15.0))
    };
    let base = run(
        with_round(vec![btd_data::v1::SpawnGroup {
            flags: btd_data::v1::Flags(btd_data::v1::Flags::CAMO),
            ..group(2, 1, 0.0, 0.0)
        }]),
        |_| {},
    );
    assert_eq!(pin(false).seconds, base.seconds);
    assert!(pin(true).seconds > base.seconds + 0.5);
}

#[test]
fn damage_over_time_ticks_after_one_interval() {
    let glue = |seconds: f64| {
        let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
        atk(&mut d, GLUE).extras.dot = Some(Dot { interval: 1.0, seconds, ..corrosive(9) });
        run(d, place(GLUE, -40.0, 15.0)).pops
    };
    assert_eq!(glue(0.9), 0, "erster Tick erst nach einem Intervall");
    assert_eq!(glue(1.1), 1);
}

#[test]
fn slow_duration_per_tag() {
    let glue = |long_for_red: bool| {
        let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
        let mut s = slow(1, 0.5);
        if long_for_red {
            s.seconds_for = vec![TagFactor { tag: "Red".into(), factor: 100.0 }];
        }
        atk(&mut d, GLUE).extras.slow = Some(s);
        run(d, place(GLUE, -40.0, 15.0))
    };
    assert!(glue(true).seconds > glue(false).seconds + 1.0);
}

#[test]
fn validation_rejects_zero_pierce_projectiles_and_bad_crits() {
    let mut d = data();
    atk(&mut d, DART).pierce = 0;
    assert!(d.validate().is_err());
    let mut d = data();
    atk(&mut d, PIN).crit = Some(Crit { damage: f64::NAN, every: 2 });
    assert!(d.validate().is_err());
    let mut d = data();
    atk(&mut d, PIN).crit = Some(Crit { damage: 5.0, every: 0 });
    assert!(d.validate().is_err());
}

#[test]
fn crit_counter_restarts_each_round() {
    // jeder 2. Schuss kritisch, Pin ohne Grundschaden; Runde 1 mit genau einem Schuss: ohne Reset
    // wäre der erste Schuss in Runde 2 kritisch
    let mut d = with_round(vec![group(0, 1, 0.0, 0.0)]);
    d.rounds[1] = vec![group(0, 1, 0.0, 0.0)];
    let a = atk(&mut d, PIN);
    (a.damage, a.cooldown) = (0.0, 1000.0);
    a.crit = Some(Crit { damage: 1.0, every: 2 });
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind: PIN, x: 0.0, y: 15.0 }).unwrap();
    assert_eq!(g.play_round().pops, 0);
    assert_eq!(g.play_round().pops, 0);
}

#[test]
fn too_many_slow_ids_are_rejected() {
    let mut d = data();
    for (i, k) in [PIN, BOMB, DART, TACK, GLUE].into_iter().enumerate() {
        atk(&mut d, k).extras.slow = Some(Slow { id: format!("s{i}"), ..slow(0, 1.0) });
    }
    assert!(Game::new(Arc::new(d)).is_err());
}

#[test]
fn burst_fires_several_shots_per_cooldown() {
    // ein Angriff pro Runde (riesiger Cooldown), Pierce 1, Reds als Pulk (immer ein Ziel da):
    // jede Salve platzt so viele Reds wie Schüsse
    let pin = |burst: Option<Burst>| {
        let mut d = with_round(vec![group(0, 10, 0.0, 0.0)]);
        let a = atk(&mut d, PIN);
        (a.cooldown, a.pierce) = (1000.0, 1);
        a.burst = burst;
        run(d, place(PIN, 0.0, 15.0)).pops
    };
    assert_eq!(pin(None), 1);
    assert_eq!(pin(Some(Burst { count: 3, interval: 0.1 })), 3);
    assert_eq!(pin(Some(Burst { count: 1, interval: 0.1 })), 1, "Salve aus einem Schuss = normal");
    assert_eq!(pin(Some(Burst { count: 3, interval: 1000.0 })), 1, "zweiter Schuss erst nach Rundenende");
}

#[test]
fn burst_stops_without_target_and_keeps_the_cooldown() {
    // ein einzelner Red: der erste Schuss platzt ihn, die restliche Salve hat kein Ziel mehr; der
    // nächste Red kommt erst nach dem Cooldown dran
    let mut d = with_round(vec![group(0, 1, 0.0, 0.0), group(0, 1, 1.0, 1.0)]);
    let a = atk(&mut d, PIN);
    (a.cooldown, a.pierce) = (0.8, 1);
    a.burst = Some(Burst { count: 3, interval: 0.1 });
    let r = run(d, place(PIN, 0.0, 15.0));
    assert_eq!((r.pops, r.leaked), (2, 0), "{r:?}");
}

#[test]
fn validation_rejects_empty_bursts() {
    let mut d = data();
    atk(&mut d, PIN).burst = Some(Burst { count: 0, interval: 0.1 });
    assert!(d.validate().is_err());
    atk(&mut d, PIN).burst = Some(Burst { count: 2, interval: -1.0 });
    assert!(d.validate().is_err());
}

/// Tack über dem Track mit einer einzigen Nadel fest Richtung +x (fliegt ohne Zielsuche vorbei).
fn homing_tack(homing: Option<Homing>) -> btd_core::RoundReport {
    let mut d = data();
    d.rounds[0] = vec![group(0, 10, 0.0, 9.0)];
    if let Delivery::Projectile { emission, homing: h, .. } = &mut d.towers[3].variants[0].attacks[0].delivery
    {
        *emission = Emission::Arc { count: 1, angle: 360.0, offset: 0.0, fixed: true };
        *h = homing;
    }
    let mut g = Game::new(Arc::new(d)).unwrap();
    g.apply(Action::Place { kind: 3, x: 0.0, y: -13.0 }).unwrap();
    g.play_round()
}

#[test]
fn homing_projectiles_turn_towards_their_target() {
    let h = |turn_rate| Some(Homing { turn_rate, distance: 100.0, max_angle: 360.0, retarget: true });
    assert_eq!(homing_tack(None).pops, 0, "ohne Zielsuche vorbei");
    let fast = homing_tack(h(2000.0));
    assert!(fast.pops >= 8, "dreht schnell genug zum Track: {fast:?}");
    assert_eq!(homing_tack(h(30.0)).pops, 0, "30°/s reicht in 0,2 s Flug nicht für 90°");
}

#[test]
fn validation_rejects_bad_homing() {
    let mut d = data();
    let set = |d: &mut GameV1, homing| {
        if let Delivery::Projectile { homing: h, .. } = &mut d.towers[3].variants[0].attacks[0].delivery {
            *h = Some(homing);
        }
    };
    set(&mut d, Homing { turn_rate: 0.0, distance: 10.0, max_angle: 90.0, retarget: true });
    assert!(d.validate().is_err(), "Drehrate 0");
    set(&mut d, Homing { turn_rate: 90.0, distance: 10.0, max_angle: 400.0, retarget: true });
    assert!(d.validate().is_err(), "Kegel > 360°");
    set(&mut d, Homing { turn_rate: 90.0, distance: 10.0, max_angle: 90.0, retarget: true });
    assert!(d.validate().is_ok());
}
