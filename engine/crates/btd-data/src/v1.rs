//! Datenformat `game_v1` (docs/m2-plan.md): eine Spielkonfiguration aus Karte, Bloons, Türmen,
//! Runden, Einkommen und Regeln. Distanzen in Spiel-Einheiten, Zeiten in Sekunden.
//!
//! Türme sind **Varianten pro Upgrade-Kombination** (`tiers = [oben, mitte, unten]`). Ein Upgrade
//! wechselt auf die nächste Variante; Crosspaths stecken damit schon in den Daten.
//!
//! sim_v0 (die M1-Referenz) ist ein Datensatz in diesem Format: Hitscan-Angriffe, 1 HP pro
//! Schicht, Raster-Koordinaten, 20 Ticks/s.

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::{check_unique, invalid, BloonKind, DataError, TowerKind};

pub const FORMAT: &str = "game_v1";
pub const MAX_TIER: usize = 5;
/// Anzahl Upgrade-Pfade (BTD6: oben, mitte, unten).
pub const PATHS: usize = 3;

// ---------------------------------------------------------------- Bloon-Eigenschaften

/// Bitmaske der Bloon-Eigenschaften, gegen die Projektile immun sein können
/// (wie `BloonProperties` im Spiel: 1 Lead, 2 Black, 4 White, 8 Purple, 16 Frozen).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Props(pub u8);

impl Props {
    pub const LEAD: u8 = 1;
    pub const BLACK: u8 = 2;
    pub const WHITE: u8 = 4;
    pub const PURPLE: u8 = 8;
    pub const FROZEN: u8 = 16;

    pub fn intersects(self, other: Props) -> bool {
        self.0 & other.0 != 0
    }

    pub fn parse(names: &[String], what: &str) -> Result<Props, DataError> {
        let mut m = 0;
        for n in names {
            m |= match n.as_str() {
                "lead" => Self::LEAD,
                "black" => Self::BLACK,
                "white" => Self::WHITE,
                "purple" => Self::PURPLE,
                "frozen" => Self::FROZEN,
                other => return Err(invalid(format!("{what}: unbekannte Eigenschaft {other:?}"))),
            };
        }
        Ok(Props(m))
    }
}

/// Klassen für Bonus-Schaden und Targeting (MOAB-Klasse, Ceramic, Boss).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Classes(pub u8);

impl Classes {
    pub const MOAB: u8 = 1;
    pub const CERAMIC: u8 = 2;
    pub const BOSS: u8 = 4;

    fn parse(names: &[String], what: &str) -> Result<Classes, DataError> {
        let mut m = 0;
        for n in names {
            m |= match n.as_str() {
                "moab" => Self::MOAB,
                "ceramic" => Self::CERAMIC,
                "boss" => Self::BOSS,
                other => return Err(invalid(format!("{what}: unbekannte Klasse {other:?}"))),
            };
        }
        Ok(Classes(m))
    }
}

/// Instanz-Flags eines Bloons (keine eigenen Typen, sonst explodiert die Typanzahl).
/// Stand M2a: Camo wirkt (nur Türme mit Camo-Erkennung zielen auf den Bloon); Regrow und
/// Fortified werden geladen und vererbt, haben aber noch keine Wirkung (kommt in M2b).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(pub u8);

impl Flags {
    pub const CAMO: u8 = 1;
    pub const REGROW: u8 = 2;
    pub const FORTIFIED: u8 = 4;

    pub fn has(self, f: u8) -> bool {
        self.0 & f != 0
    }

    fn parse(names: &[String], what: &str) -> Result<Flags, DataError> {
        let mut m = 0;
        for n in names {
            m |= match n.as_str() {
                "camo" => Self::CAMO,
                "regrow" => Self::REGROW,
                "fortified" => Self::FORTIFIED,
                other => return Err(invalid(format!("{what}: unbekanntes Flag {other:?}"))),
            };
        }
        Ok(Flags(m))
    }
}

// ---------------------------------------------------------------- Typen

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesV1 {
    /// Simulationsschritte pro Sekunde (Spiel: 60, sim_v0: 20).
    pub tick_rate: f64,
    pub cd_eps: f64,
    pub max_round_seconds: f64,
    /// Abstand, mit dem Kinder hinter dem Parent auf dem Pfad erscheinen.
    pub child_spacing: f64,
    pub start_cash: i64,
    pub start_lives: i32,
    /// Erste gespielte Runde (1-basiert; CHIMPS: 6).
    pub start_round: u32,
    /// Letzte Runde, nach der das Spiel gewonnen ist.
    pub end_round: u32,
    pub selling: bool,
    pub sell_ratio: f64,
    /// Kosten = Medium-Kosten × `cost_multiplier`, gerundet auf ein Vielfaches von `cost_rounding`.
    pub cost_multiplier: f64,
    pub cost_rounding: i64,
    /// Turmtypen, die in diesem Modus nie gebaut werden dürfen (Spiel: LockTowerModModel, z.B. die
    /// Banana Farm in CHIMPS). Sie bleiben in den Daten, damit das Layout für alle Presets gleich ist.
    #[serde(default)]
    pub locked_towers: Vec<String>,
}

/// Raster für die Env: Aktionsraum (Zelle -> Weltposition) und Observation (Pfad, bebaubar).
#[derive(Clone, Debug)]
pub struct GridV1 {
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub w: usize,
    pub h: usize,
    /// Zeilenweise, Index = y * w + x.
    pub path: Vec<bool>,
    pub buildable: Vec<bool>,
}

/// Was beim Platzieren entscheidet, ob eine Position bebaubar ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// Die Rasterzelle unter der Position (sim_v0).
    Grid,
    /// Die Flächen-Polygone (echte Karten).
    Areas,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaKind {
    Land,
    Water,
    /// Weder bebaubar noch Pfad (Felsen, Bäume, Rand).
    Blocked,
    Track,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaV1 {
    pub kind: AreaKind,
    pub polygon: Vec<[f64; 2]>,
}

#[derive(Clone, Debug)]
pub struct MapV1 {
    pub name: String,
    /// Spielfeld [xmin, ymin, xmax, ymax]; Türme müssen mit ihrem Footprint darin liegen.
    pub bounds: [f64; 4],
    pub points: Vec<[f64; 2]>,
    pub cum: Vec<f64>,
    pub grid: GridV1,
    pub placement: Placement,
    /// Flächen; spätere überdecken frühere (Spiel: erste Fläche = Spielfeld).
    /// Bei `placement = grid` werden sie ignoriert.
    pub areas: Vec<AreaV1>,
}

#[derive(Clone, Debug)]
pub struct BloonDefV1 {
    pub name: String,
    pub hp: f64,
    /// Spiel-Einheiten pro Sekunde.
    pub speed: f64,
    /// Lebensverlust beim Durchkommen.
    pub leak: i32,
    pub children: Vec<(BloonKind, u16)>,
    pub props: Props,
    pub classes: Classes,
    /// Typ ist Camo (im Spiel eine eigene Variante wie `GreenCamo`); zusätzlich zum Instanz-Flag.
    pub camo: bool,
    /// Kollisionsradius für Projektile (Spiel: Red 8). 0 = Punkt.
    pub radius: f64,
    /// Tags des Spiels (z.B. "Ceramic", "Moabs", "Fortified") für Bonus-Schaden und Filter.
    pub tags: Vec<String>,
    /// Typ wächst nach (Regrow): alle `grow_rate` Sekunden eine Stufe zurück zum Vorfahren
    /// (docs/design-bloons-chimps.md). `grow_rate` 0 = wächst nicht (ältere Daten ohne das Feld).
    pub regrow: bool,
    pub grow_rate: f64,
    /// Derselbe Bloon ohne Camo (im Spiel ein eigener Typ, z.B. `RedCamo` -> `Red`); Ziel von
    /// `HitExtras.cleanse` (Signal Flare). Nur bei Camo-Typen.
    pub decamo: Option<BloonKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Targeting {
    First,
    /// Fester Punkt je Karte: der Streckenpunkt mit der meisten Streckenlänge im Explosionsradius
    /// (Mortar, docs/design-towers-step3.md §3). Nur mit `Delivery::Lob` mit Explosion; gefeuert wird,
    /// sobald ein anvisierbarer Bloon in Reichweite ist.
    TrackBest,
    /// Befreundeter Turm in Reichweite (Alchemist-Tränke, docs/design-towers-step3.md §2.4): mit Angriff,
    /// nicht ausgeschlossen, ohne gleich starken Effekt derselben Gruppe und nicht gesperrt; der mit dem
    /// höchsten `spent`, bei Gleichstand die kleinste Turm-ID. Nur mit `Delivery::Potion`.
    Friendly,
    /// Punkt auf der Strecke in Reichweite, über die Streckenlänge in Reichweite verteilt (Spike Factory,
    /// docs/design-towers-step3.md §2.2): ohne Zufall der Anteil frac(k·φ) für den k-ten Wurf der Runde,
    /// mit Zufall gleichverteilt. Nur mit `Delivery::Lob`; gefeuert wird, solange ein anvisierbarer Bloon
    /// auf der Karte ist.
    TrackSpread,
}

/// Wie ein Angriff seine Ziele trifft.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Delivery {
    /// Sofortiger Treffer auf bis zu `pierce` Bloons in Reichweite, weiteste zuerst (sim_v0).
    Hitscan {},
    /// Sofort am vordersten Ziel explodieren; trifft bis zu `pierce` Bloons im `radius`
    /// um das Ziel, weiteste zuerst (sim_v0-Bomb).
    HitscanSplash { radius: f64 },
    /// Sofortiger Flächentreffer um den Turm (Ice): bis zu `pierce` Bloons in Reichweite,
    /// weiteste zuerst. Feuert nur, wenn ein Ziel in Reichweite ist.
    Area {
        /// Radius des Flächentreffers; ohne Angabe die Reichweite des Angriffs.
        #[serde(default)]
        radius: Option<f64>,
        /// Im Spiel an die Reichweite gekoppelt (LinkProjectileRadiusToTowerRange): Radius =
        /// Reichweite + Versatz. Mit Reichweiten-Auren wächst `radius` um denselben Betrag wie die
        /// Reichweite (docs/design-towers-step2.md §2.1); ungebufft gilt `radius`.
        #[serde(default)]
        radius_offset: Option<f64>,
    },
    /// Fliegendes Projektil. Trifft jeden Bloon höchstens einmal, bis `pierce` verbraucht ist
    /// oder `lifespan` abläuft. `radius` + Bloon-Radius = Kollisionsabstand. Mit `path` fliegt es
    /// eine Kurve (Boomerang): Punkte relativ zum Turm, lokale +y-Achse = Richtung zum Ziel;
    /// am Ende der Kurve ist es weg.
    Projectile {
        speed: f64,
        lifespan: f64,
        radius: f64,
        emission: Emission,
        #[serde(default)]
        path: Option<Vec<[f64; 2]>>,
        /// Treffer-Set alle n Sekunden leeren, damit dasselbe Projektil erneut treffen kann.
        #[serde(default)]
        clear_hits: Option<f64>,
        /// Was passiert, sobald der Pierce verbraucht ist oder die Lebensdauer abläuft (Spiel:
        /// CreateProjectileOnExhaustFraction mit fraction 1).
        #[serde(default)]
        on_exhaust: Vec<Contact>,
        /// Was bei jedem Kontakt mit einem Bloon zusätzlich passiert (Bomb: Explosion,
        /// Frag Bombs: Explosion + Splitter).
        #[serde(default)]
        on_contact: Vec<Contact>,
        /// Zielsuche (Wizard, Ninja): das Projektil dreht sich im Flug auf sein Ziel.
        #[serde(default)]
        homing: Option<Homing>,
    },
    /// Wurf (docs/design-towers-step3.md §2.1): fliegt geradlinig zum Zielpunkt (Position des Ziels beim
    /// Abwurf bzw. Punkt aus `targeting`), trifft unterwegs nichts und löst bei Ankunft `on_arrive` aus.
    /// Flugzeit fest (`time`, Mortar) oder Abstand / `speed` (Alchemist); genau eins von beiden.
    Lob {
        #[serde(default)]
        time: Option<f64>,
        #[serde(default)]
        speed: Option<f64>,
        /// Streuung um den Zielpunkt (Radius, Spiel: RandomTargetSpreadModel); ohne Zufall der Punkt selbst.
        #[serde(default)]
        spread: f64,
        #[serde(default)]
        on_arrive: Vec<Contact>,
        /// Bleibt nach der Ankunft liegen (Spike-Stapel, docs/design-towers-step3.md §2.3).
        #[serde(default)]
        ground: Option<Box<Ground>>,
    },
    /// Trank auf einen befreundeten Turm (`Targeting::Friendly`): setzt dort einen Zeit-Effekt. Die
    /// Flugzeit (unter 0,2 s) wird ignoriert [KAL].
    Potion { effect: TimedEffect },
}

/// Bodenobjekt nach einem Wurf: liegt `lifespan` Sekunden (ab der Landung) am Zielpunkt, trifft Bloons wie ein
/// Projektil (je Bloon einmal, mit `clear_hits` erneut), aber nur alle `collide_every` Sekunden, bis `pierce`
/// verbraucht ist; dann bzw. beim Ablauf `on_exhaust` (nur Explosionen). Hält die Runde nicht offen und
/// verschwindet am Rundenende.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    pub lifespan: f64,
    pub radius: f64,
    pub pierce: u32,
    pub damage: f64,
    #[serde(default)]
    pub cannot_damage: Vec<String>,
    #[serde(default)]
    pub camo: bool,
    #[serde(default)]
    pub extras: HitExtras,
    #[serde(default)]
    pub clear_hits: Option<f64>,
    #[serde(default)]
    pub collide_every: f64,
    #[serde(default)]
    pub on_exhaust: Vec<Contact>,
}

impl Ground {
    /// `cannot_damage` als Bitmaske (beim Laden validiert).
    pub fn cannot_damage_props(&self) -> Props {
        Props::parse(&self.cannot_damage, "Boden").unwrap_or_default()
    }
}

/// Zeit-Effekt auf einem Turm (docs/design-towers-step3.md §2.4): wirkt `seconds` lang (ohne Angabe
/// unbegrenzt in der Runde) oder für `shots` Schüsse, je nachdem was zuerst endet; danach `block`
/// Sekunden keine neue Gabe derselben Gruppe. Je `group` wirkt höchstens ein Effekt; ein höherer `rank`
/// ersetzt einen niedrigeren, ein gleicher oder niedrigerer wird nicht vergeben. Zu Rundenbeginn und
/// -ende gelöscht.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedEffect {
    pub group: String,
    #[serde(default)]
    pub rank: u32,
    #[serde(default)]
    pub seconds: Option<f64>,
    /// Ohne Angabe unbegrenzt.
    #[serde(default)]
    pub shots: Option<u32>,
    #[serde(default)]
    pub block: f64,
    /// Zuschlag auf den Schaden aller Angriffe und Kontakte mit Schaden > 0.
    #[serde(default)]
    pub damage: f64,
    /// Zuschlag auf den Pierce fliegender Projektile (wie die Pierce-Aura).
    #[serde(default)]
    pub pierce: u32,
    /// Tempo: Cooldown / (1 + `rate_frac`) [KAL].
    #[serde(default)]
    pub rate_frac: f64,
    /// Reichweite × (1 + Auren-Anteil + `range_frac`).
    #[serde(default)]
    pub range_frac: f64,
    /// Bonus-Schaden für alle Angriffe und Kontakte mit Schaden > 0 (Acidic Mixture Dip).
    #[serde(default)]
    pub damage_vs: Vec<Bonus>,
    /// Türme (Namen), die diesen Effekt nicht bekommen.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Zielsuchendes Projektil (Spiel: TrackTargetModel / TrackTargetWithinTimeModel). Startziel ist
/// das Ziel des Turms; ist es weg, sucht es mit `retarget` den nächsten Bloon im Umkreis `distance`
/// und im Kegel `max_angle` (voller Öffnungswinkel, Grad) um die Flugrichtung [KAL].
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Homing {
    /// Grad pro Sekunde.
    pub turn_rate: f64,
    pub distance: f64,
    pub max_angle: f64,
    pub retarget: bool,
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contact {
    /// Sofortiger Flächentreffer an der Kontaktstelle.
    Explosion {
        radius: f64,
        pierce: u32,
        damage: f64,
        #[serde(default)]
        cannot_damage: Vec<String>,
        #[serde(default)]
        camo: bool,
        #[serde(default)]
        extras: HitExtras,
    },
    /// Neue Projektile ab der Kontaktstelle (Richtungen relativ zur Flugrichtung).
    Shards {
        emission: Emission,
        speed: f64,
        lifespan: f64,
        radius: f64,
        pierce: u32,
        damage: f64,
        #[serde(default)]
        cannot_damage: Vec<String>,
        #[serde(default)]
        camo: bool,
        #[serde(default)]
        extras: HitExtras,
        /// Splitter übernehmen das Treffer-Set des Projektils (Spiel: passOnCollidedWith),
        /// treffen also z.B. den Kontakt-Bloon nicht noch einmal.
        #[serde(default)]
        inherit_hit: bool,
        /// Effekte, sobald ein Splitter verbraucht oder abgelaufen ist (Cluster Bombs:
        /// Explosion). Nur Explosionen, keine weiteren Splitter.
        #[serde(default)]
        on_exhaust: Vec<Contact>,
        /// Splitter treffen Bloons; `false` = fliegen nur und lösen am Ende `on_exhaust` aus
        /// (Spiel: dontUseCollisionChecker).
        #[serde(default = "yes")]
        collide: bool,
    },
}

/// Wie viele Projektile pro Schuss in welche Richtungen fliegen.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Emission {
    /// Ein Projektil Richtung Ziel.
    Single {},
    /// `count` Projektile, über `angle` Grad verteilt (360 = rundum). `fixed`: Richtung hängt
    /// nicht vom Ziel ab (Turm dreht sich nicht, z.B. Tack), sonst um die Zielrichtung zentriert.
    Arc {
        count: u32,
        angle: f64,
        #[serde(default)]
        offset: f64,
        #[serde(default)]
        fixed: bool,
    },
    /// `count` Projektile nebeneinander in dieselbe Richtung, `spread` Abstand quer dazu.
    Parallel { count: u32, spread: f64 },
}

impl Emission {
    fn valid(&self) -> bool {
        match self {
            Emission::Single {} => true,
            Emission::Arc { count, angle, offset, .. } => {
                (1..=1000).contains(count)
                    && angle.is_finite()
                    && (0.0..=360.0).contains(angle)
                    && offset.is_finite()
            }
            Emission::Parallel { count, spread } => {
                (1..=100).contains(count) && spread.is_finite() && *spread >= 0.0
            }
        }
    }
}

impl HitFilter {
    fn valid(&self, tags: &HashSet<&str>) -> bool {
        self.exclude_tags.iter().all(|t| tags.contains(t.as_str()))
            && Props::parse(&self.skip_props, "").is_ok()
    }
}

impl HitExtras {
    /// Werte im gültigen Bereich und nur Tags, die ein Bloon der Daten hat (Tippfehler würden
    /// sonst still zur leeren Maske).
    fn valid(&self, tags: &HashSet<&str>) -> bool {
        let ok = |x: f64| x.is_finite() && x >= 0.0;
        let known = |list: &[TagFactor]| list.iter().all(|t| tags.contains(t.tag.as_str()));
        self.filter.valid(tags)
            && self
                .damage_vs
                .iter()
                .all(|b| tags.contains(b.tag.as_str()) && b.all.iter().all(|t| tags.contains(t.as_str())))
            && self.freeze.as_ref().map_or(true, |f| known(&f.scale))
            && self.slow.as_ref().map_or(true, |s| known(&s.multiplier_for) && known(&s.seconds_for))
            && self.dot.as_ref().map_or(true, |d| {
                known(&d.factor_for)
                    && d.exclude_tags.iter().all(|t| tags.contains(t.as_str()))
                    && Props::parse(&d.cannot_damage, "").is_ok()
            })
            && self
                .cleanse
                .as_ref()
                .map_or(true, |c| c.exclude_tags.iter().all(|t| tags.contains(t.as_str())))
            && self.pop_slow.as_ref().map_or(true, |s| {
                known(&s.multiplier_for)
                    && known(&s.seconds_for)
                    && s.level == 0
                    && ok(s.seconds)
                    && (0.0..=1.0).contains(&s.multiplier)
            })
            && self.push.as_ref().map_or(true, |p| known(&p.factor_for))
            && self.damage_vs.iter().all(|b| b.add.is_finite())
            && self.freeze.as_ref().map_or(true, |f| ok(f.seconds) && f.scale.iter().all(|t| ok(t.factor)))
            && self.dot.as_ref().map_or(true, |d| {
                ok(d.damage)
                    && ok(d.interval)
                    && d.interval > 0.0
                    && ok(d.seconds)
                    && d.factor_for.iter().all(|t| ok(t.factor))
            })
            && self.push.as_ref().map_or(true, |p| ok(p.amount) && p.factor_for.iter().all(|t| ok(t.factor)))
            && self.slow.as_ref().map_or(true, |s| {
                ok(s.seconds)
                    && (0.0..=1.0).contains(&s.multiplier)
                    && s.multiplier_for.iter().all(|t| (0.0..=1.0).contains(&t.factor))
                    && s.seconds_for.iter().all(|t| ok(t.factor))
            })
    }
}

fn yes() -> bool {
    true
}

impl GameV1 {
    /// Alle Treffer-Extras der Türme (Angriffe, Kontakt- und Verbrauchs-Effekte, verschachtelt).
    pub fn all_extras(&self) -> Vec<&HitExtras> {
        fn contacts<'a>(list: &'a [Contact], out: &mut Vec<&'a HitExtras>) {
            for c in list {
                out.push(c.extras());
                if let Contact::Shards { on_exhaust, .. } = c {
                    contacts(on_exhaust, out);
                }
            }
        }
        let mut out = Vec::new();
        for a in self.towers.iter().flat_map(|t| &t.variants).flat_map(|v| &v.attacks) {
            out.push(&a.extras);
            if let Delivery::Projectile { on_contact, on_exhaust, .. } = &a.delivery {
                contacts(on_contact, &mut out);
                contacts(on_exhaust, &mut out);
            }
            if let Delivery::Lob { on_arrive, ground, .. } = &a.delivery {
                contacts(on_arrive, &mut out);
                if let Some(g) = ground {
                    out.push(&g.extras);
                    contacts(&g.on_exhaust, &mut out);
                }
            }
        }
        out
    }
}

impl GameV1 {
    /// Wertebereiche eines Zeit-Effekts, bekannte Tags und Turmnamen.
    fn effect_valid(&self, e: &TimedEffect, tags: &HashSet<&str>) -> bool {
        let non_neg = |x: f64| x.is_finite() && x >= 0.0;
        !e.group.is_empty()
            && e.shots != Some(0)
            && e.seconds.map_or(true, |s| s.is_finite() && s > 0.0)
            && non_neg(e.block)
            && non_neg(e.damage)
            && non_neg(e.rate_frac)
            && non_neg(e.range_frac)
            && e.damage_vs.iter().all(|b| {
                b.add.is_finite()
                    && tags.contains(b.tag.as_str())
                    && b.all.iter().all(|t| tags.contains(t.as_str()))
            })
            && e.exclude.iter().all(|n| self.towers.iter().any(|t| &t.name == n))
    }
}

impl Delivery {
    /// Radius für `Targeting::TrackBest`: die größte Explosion eines Wurfs, die Schaden macht (sonst die
    /// größte überhaupt; Signal Flare hat eine große Explosion ohne Schaden neben der eigentlichen).
    pub fn track_best_radius(&self) -> Option<f64> {
        let Delivery::Lob { on_arrive, .. } = self else { return None };
        let radius = |with_damage: bool| {
            on_arrive
                .iter()
                .filter_map(|c| match c {
                    Contact::Explosion { radius, damage, .. } if !with_damage || *damage > 0.0 => {
                        Some(*radius)
                    }
                    _ => None,
                })
                .reduce(f64::max)
        };
        radius(true).or_else(|| radius(false))
    }
}

impl Contact {
    /// Werte, Eigenschaften und Extras gültig; Splitter (nur mit `shards_ok`) dürfen beim
    /// Verbrauch höchstens 8 Explosionen auslösen.
    fn valid(&self, shards_ok: bool, tags: &HashSet<&str>) -> bool {
        let pos = |x: f64| x.is_finite() && x > 0.0;
        let non_neg = |x: f64| x.is_finite() && x >= 0.0;
        let (Contact::Explosion { cannot_damage, .. } | Contact::Shards { cannot_damage, .. }) = self;
        Props::parse(cannot_damage, "").is_ok()
            && self.extras().valid(tags)
            && match self {
                Contact::Explosion { radius, damage, .. } => pos(*radius) && non_neg(*damage),
                Contact::Shards { emission, speed, lifespan, radius, damage, on_exhaust, pierce, .. } => {
                    shards_ok
                        && *pierce >= 1
                        && pos(*speed)
                        && pos(*lifespan)
                        && non_neg(*radius)
                        && non_neg(*damage)
                        && emission.valid()
                        && on_exhaust.len() <= 8
                        && on_exhaust.iter().all(|c| c.valid(false, tags))
                }
            }
    }

    pub fn extras(&self) -> &HitExtras {
        let (Contact::Explosion { extras, .. } | Contact::Shards { extras, .. }) = self;
        extras
    }

    /// `cannot_damage` als Bitmaske (beim Laden validiert).
    pub fn cannot_damage_props(&self) -> Props {
        let (Contact::Explosion { cannot_damage, .. } | Contact::Shards { cannot_damage, .. }) = self;
        Props::parse(cannot_damage, "Kontakt").unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttackV1 {
    pub tag: String,
    /// Eigene Reichweite, sonst die des Turms.
    pub range: Option<f64>,
    pub targeting: Targeting,
    pub cooldown: f64,
    pub damage: f64,
    /// Maximale Anzahl Treffer pro Schuss (0 = der Angriff verbraucht nur seinen Cooldown).
    pub pierce: u32,
    /// Bloon-Eigenschaften, die dieser Angriff nicht beschädigen kann.
    pub cannot_damage: Props,
    /// Camo-Erkennung. Gilt nur fürs Zielen: ohne sie wird kein Camo-Bloon anvisiert, eine
    /// Explosion um ein sichtbares Ziel trifft aber auch Camo-Bloons in der Nähe (wie im Spiel).
    /// Ein Splash-Angriff ohne Camo-Erkennung schießt deshalb nie auf eine reine Camo-Gruppe.
    pub camo: bool,
    pub delivery: Delivery,
    pub extras: HitExtras,
    pub crit: Option<Crit>,
    /// Salve: nach dem Cooldown `count` Schüsse im Abstand `interval` (Spiel: BurstWeaponBehaviorModel).
    pub burst: Option<Burst>,
}

/// Zusätzliche Treffer-Regeln (aus den Filter-, Bonus- und Status-Models des Spiels).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HitExtras {
    pub filter: HitFilter,
    /// Bonus-Schaden gegen Bloons mit einem Tag (Spiel: DamageModifierForTagModel).
    pub damage_vs: Vec<Bonus>,
    /// Einfrieren (Ice): Bloon bleibt stehen, ist so lange Frozen (spitze Angriffe wirkungslos).
    pub freeze: Option<Freeze>,
    /// Verlangsamen (Glue).
    pub slow: Option<Slow>,
    /// Schaden über Zeit (Corrosive Glue, Tack-Feuer).
    pub dot: Option<Dot>,
    /// Zurückstoßen entlang des Pfads (Bomb).
    pub push: Option<Push>,
    /// Entfernt Eigenschaften des getroffenen Bloons (Spiel: RemoveBloonModifiersModel, z.B. Signal Flare).
    pub cleanse: Option<Cleanse>,
    /// Verlangsamt die Kinder, wenn der Treffer einen Bloon platzt (Spiel: SlowOnPopModel, Engineer Pin).
    pub pop_slow: Option<Slow>,
}

/// Camo entfernen: der Bloon wechselt auf seinen Typ ohne Camo (`BloonDefV1::decamo`), wahlweise nur bei
/// Schaden, nicht bei Bloons mit Tag. Regrow, Lead und Fortified gibt es noch nicht.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cleanse {
    pub camo: bool,
    pub only_if_damaged: bool,
    pub exclude_tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dot {
    pub damage: f64,
    /// Sekunden zwischen zwei Schadens-Ticks; der erste kommt nach einem Intervall.
    pub interval: f64,
    pub seconds: f64,
    pub layers: u32,
    /// Glue-Stufe, die der Effekt setzt (0 = keine).
    #[serde(default)]
    pub level: u8,
    /// Schadens-Faktor für Bloons mit Tag (Spiel: MOABs 0.375).
    #[serde(default)]
    pub factor_for: Vec<TagFactor>,
    /// Eigenschaften, die der Schaden über Zeit nicht trifft (Feuer: Purple).
    #[serde(default)]
    pub cannot_damage: Vec<String>,
    /// Nur setzen, wenn der Treffer Schaden gemacht hat (Burny Stuff).
    #[serde(default)]
    pub only_if_damaged: bool,
    /// Nicht auf Bloons mit diesen Tags (Burny Stuff: DDT, Black, Zebra); unter MIB (Village) aufgehoben.
    #[serde(default)]
    pub exclude_tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Push {
    /// Strecke zurück entlang des Pfads.
    pub amount: f64,
    /// Faktor für Bloons mit Tag (Spiel: MOAB-Klassen 0).
    #[serde(default)]
    pub factor_for: Vec<TagFactor>,
}

/// Kritischer Treffer: jeder `every`-te Schuss macht `damage` statt des normalen Schadens. Das
/// Spiel würfelt den Abstand je Crit aus `every_range`; die Sim würfelt inklusive der Obergrenze
/// [KAL: ob das Spiel sie einschließt]. Ohne Zufall gilt `every`.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crit {
    pub damage: f64,
    pub every: u32,
    #[serde(default)]
    pub every_range: Option<[u32; 2]>,
}

/// Salve: `count` Schüsse je Angriff, `interval` Sekunden auseinander (Desperado).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Burst {
    pub count: u32,
    pub interval: f64,
}

/// Verlangsamungs-Zone um den Turm (Arctic Wind).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub radius: f64,
    pub speed_scale: f64,
    #[serde(default)]
    pub camo: bool,
    #[serde(default)]
    pub filter: HitFilter,
}

/// Turmgruppe im Spiel (`towerSet`), z.B. als Filter für Buffs, die nur Primary-Türme treffen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TowerSet {
    #[default]
    Primary,
    Military,
    Magic,
    Support,
}

/// Statische Aura eines Support-Turms auf andere Türme in seiner (ungebufften) Reichweite
/// (docs/design-towers-step2.md §2.1). Je `group` (Spiel: mutatorId) wirkt nur die stärkste Aura;
/// `sets` leer = alle Türme. Wirkung: `btd_core::Game::refresh_buffs`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Support {
    /// Reichweite × (1 + `add_frac`) + `add`.
    Range {
        group: String,
        #[serde(default)]
        sets: Vec<TowerSet>,
        #[serde(default)]
        add_frac: f64,
        #[serde(default)]
        add: f64,
    },
    /// Cooldown × `factor` (0 < factor <= 1).
    Rate {
        group: String,
        #[serde(default)]
        sets: Vec<TowerSet>,
        factor: f64,
    },
    /// Camo-Erkennung.
    Camo {
        group: String,
        #[serde(default)]
        sets: Vec<TowerSet>,
    },
    /// Pierce + `add`.
    Pierce {
        group: String,
        #[serde(default)]
        sets: Vec<TowerSet>,
        add: u32,
    },
    /// Projektiltempo × (1 + `add_frac`).
    ProjectileSpeed {
        group: String,
        #[serde(default)]
        sets: Vec<TowerSet>,
        add_frac: f64,
    },
    /// Trifft alle Bloon-Arten (`cannot_damage` leer; MIB).
    DamageAll {
        group: String,
        #[serde(default)]
        sets: Vec<TowerSet>,
    },
}

impl Support {
    pub fn group(&self) -> &str {
        match self {
            Support::Range { group, .. }
            | Support::Rate { group, .. }
            | Support::Camo { group, .. }
            | Support::Pierce { group, .. }
            | Support::ProjectileSpeed { group, .. }
            | Support::DamageAll { group, .. } => group,
        }
    }

    pub fn sets(&self) -> &[TowerSet] {
        match self {
            Support::Range { sets, .. }
            | Support::Rate { sets, .. }
            | Support::Camo { sets, .. }
            | Support::Pierce { sets, .. }
            | Support::ProjectileSpeed { sets, .. }
            | Support::DamageAll { sets, .. } => sets,
        }
    }

    fn valid(&self) -> bool {
        let fin = |x: f64| x.is_finite();
        let sets = self.sets();
        !self.group().is_empty()
            && sets.iter().enumerate().all(|(i, s)| !sets[..i].contains(s))
            && match *self {
                // nicht negativ: sonst konvergiert der Reichweiten-Fixpunkt der Engine nicht sicher
                Support::Range { add_frac, add, .. } => {
                    fin(add_frac) && add_frac >= 0.0 && fin(add) && add >= 0.0
                }
                Support::Rate { factor, .. } => fin(factor) && factor > 0.0 && factor <= 1.0,
                Support::ProjectileSpeed { add_frac, .. } => fin(add_frac) && add_frac > -1.0,
                Support::Pierce { add, .. } => add <= MAX_PIERCE_ADD,
                Support::Camo { .. } | Support::DamageAll { .. } => true,
            }
    }
}

/// Rabatt-Zone (Spiel: DiscountZoneModel) auf Käufe in der Reichweite des Turms: Platzieren und
/// Upgrades auf Tier `tier_min..=tier_cap`. Je `stack` zählen höchstens `stack_limit` Zonen.
/// Wirkung: `btd_core::Game::discount_at` (docs/design-towers-step2.md §2.2).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discount {
    pub fraction: f64,
    pub tier_min: u8,
    pub tier_cap: u8,
    pub stack: String,
    pub stack_limit: u32,
    /// Ob der Turm seine eigenen Käufe rabattiert.
    #[serde(default)]
    pub affect_self: bool,
}

/// Einkommen nach jeder geschafften Runde (Banana Farm; Bananen gelten als eingesammelt).
/// Noch ohne Wirkung in der Engine (docs/design-towers-step2.md §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Income {
    pub per_round: f64,
    /// Zusätzliche Leben je Runde.
    #[serde(default)]
    pub lives: i32,
}

/// Welche Bloons ein Angriff gar nicht erst anvisiert oder trifft.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HitFilter {
    /// Bloons mit einem dieser Tags auslassen (z.B. "Moabs").
    pub exclude_tags: Vec<String>,
    /// Bloons mit einer dieser Eigenschaften auslassen (Spiel: FilterBloonIfDamageType mit
    /// `ifCantHitBloonProperties`; kann von `cannot_damage` abweichen).
    pub skip_props: Vec<String>,
    pub skip_frozen: bool,
    /// Bloons auslassen, die schon mindestens diese Glue-Stufe haben (0 = kein Filter).
    pub skip_glue_level: u8,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bonus {
    pub tag: String,
    pub add: f64,
    /// Weitere Tags, die der Bloon zusätzlich alle haben muss (Spiel: mustIncludeAllTags, z.B.
    /// "Lead,Fortified" beim Acidic Mixture Dip).
    #[serde(default)]
    pub all: Vec<String>,
}

/// `layers`: über wie viele Schichten (Kinder beim Platzen) der Effekt erhalten bleibt.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Freeze {
    pub seconds: f64,
    pub layers: u32,
    /// Dauer-Faktor für Bloons mit Tag (Spiel: Ceramics nur halb so lange).
    #[serde(default)]
    pub scale: Vec<TagFactor>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagFactor {
    pub tag: String,
    pub factor: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slow {
    /// Effekt-ID (Spiel: mutationId). Verlangsamungen ohne Glue-Stufe mit verschiedener ID
    /// (Permafrost, Stun) wirken nebeneinander, gleiche ID ersetzt sich.
    #[serde(default)]
    pub id: String,
    /// Geschwindigkeitsfaktor (0.5 = halbe Geschwindigkeit).
    pub multiplier: f64,
    pub seconds: f64,
    pub level: u8,
    pub layers: u32,
    /// Abweichender Geschwindigkeitsfaktor für Bloons mit Tag (Spiel: MOABs 1.0 = nicht verlangsamt).
    #[serde(default)]
    pub multiplier_for: Vec<TagFactor>,
    /// Abweichende Dauer in Sekunden für Bloons mit Tag (`factor` = Sekunden; Spiel: lifespanOverride).
    #[serde(default)]
    pub seconds_for: Vec<TagFactor>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantV1 {
    pub tiers: [u8; PATHS],
    pub range: f64,
    pub footprint: f64,
    pub attacks: Vec<AttackV1>,
    pub zones: Vec<Zone>,
    /// Auren auf andere Türme (Village).
    pub supports: Vec<Support>,
    pub discount: Vec<Discount>,
    pub income: Option<Income>,
    /// Zuschlag auf `rules.sell_ratio` beim Verkaufen (Farm).
    pub sell_bonus: f64,
    /// Zeit-Effekt auf dem eigenen Turm zu Beginn jeder Runde (Spiel: StartOfRoundRateBuffModel).
    pub round_start: Option<TimedEffect>,
}

#[derive(Clone, Debug)]
pub struct TowerDefV1 {
    pub name: String,
    /// ID des Turms im Spiel (z.B. `DartMonkey`), für die Mod (Replay-Player); fehlt bei sim_v0.
    pub game_id: Option<String>,
    pub set: TowerSet,
    /// Medium-Kosten.
    pub cost: i64,
    /// `upgrade_costs[pfad][tier-1]`, Medium.
    pub upgrade_costs: [Vec<i64>; PATHS],
    pub variants: Vec<VariantV1>,
    /// Index in `variants` je Kombination (`t0*36 + t1*6 + t2`), `u16::MAX` = ungültig.
    variant_index: Vec<u16>,
}

impl TowerDefV1 {
    pub fn variant(&self, tiers: [u8; PATHS]) -> Option<&VariantV1> {
        let i = tiers_index(tiers)?;
        self.variant_index.get(i).filter(|&&v| v != u16::MAX).map(|&v| &self.variants[v as usize])
    }

    pub fn variant_id(&self, tiers: [u8; PATHS]) -> Option<u16> {
        let i = tiers_index(tiers)?;
        self.variant_index.get(i).copied().filter(|&v| v != u16::MAX)
    }
}

fn tiers_index(t: [u8; PATHS]) -> Option<usize> {
    if t.iter().any(|&x| x as usize > MAX_TIER) {
        return None;
    }
    Some(t[0] as usize * 36 + t[1] as usize * 6 + t[2] as usize)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnGroup {
    pub kind: BloonKind,
    pub flags: Flags,
    pub count: u32,
    /// Sekunden ab Rundenbeginn; `count` Bloons gleichmäßig auf [start, end] verteilt.
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomeV1 {
    /// Cash pro geplatzter Schicht.
    pub cash_per_pop: f64,
    /// Bonus nach geschaffter Runde.
    pub end_bonus: f64,
}

#[derive(Clone, Debug)]
pub struct GameV1 {
    pub name: String,
    /// Herkunft der Daten (aus dem Spiel-Dump); fehlt bei sim_v0.
    pub meta: Option<MetaV1>,
    pub rules: RulesV1,
    pub map: MapV1,
    pub bloons: Vec<BloonDefV1>,
    pub towers: Vec<TowerDefV1>,
    /// `rounds[r-1]` = Runde r.
    pub rounds: Vec<Vec<SpawnGroup>>,
    /// `income[r-1]` gilt während/nach Runde r.
    pub income: Vec<IncomeV1>,
}

/// Herkunft einer Konfiguration aus dem Spiel: interner Kartenname (`Tutorial` = Monkey Meadow),
/// Modus und Schwierigkeit wie in den Partie-Regeln (`Standard`, `Clicks` = CHIMPS; `Medium`, `Hard`)
/// und Spielversion des Dumps.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaV1 {
    pub map: String,
    pub mode: String,
    pub difficulty: String,
    pub game_version: String,
}

// ---------------------------------------------------------------- Rohformat

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    format: String,
    name: String,
    #[serde(default)]
    meta: Option<MetaV1>,
    rules: RulesV1,
    map: RawMap,
    bloons: Vec<RawBloon>,
    towers: Vec<RawTower>,
    rounds: Vec<Vec<RawGroup>>,
    income: Vec<IncomeV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMap {
    name: String,
    bounds: [f64; 4],
    points: Vec<[f64; 2]>,
    cum: Vec<f64>,
    grid: RawGrid,
    placement: Placement,
    #[serde(default)]
    areas: Vec<AreaV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGrid {
    x0: f64,
    y0: f64,
    cell: f64,
    w: usize,
    h: usize,
    path: Vec<String>,
    buildable: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBloon {
    name: String,
    hp: f64,
    speed: f64,
    leak: i32,
    #[serde(default)]
    children: Vec<(String, u16)>,
    #[serde(default)]
    props: Vec<String>,
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    camo: bool,
    #[serde(default)]
    regrow: bool,
    #[serde(default)]
    grow_rate: f64,
    #[serde(default)]
    radius: f64,
    #[serde(default)]
    decamo: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTower {
    name: String,
    #[serde(default)]
    game_id: Option<String>,
    /// Ohne Angabe `primary`; Pflicht, sobald ein Support nach Turmgruppen filtert (sonst würden
    /// ältere Daten ohne das Feld alle Türme als Primary buffen).
    #[serde(default)]
    set: Option<TowerSet>,
    cost: i64,
    upgrade_costs: Vec<Vec<i64>>,
    variants: Vec<RawVariant>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVariant {
    tiers: [u8; PATHS],
    range: f64,
    footprint: f64,
    attacks: Vec<RawAttack>,
    #[serde(default)]
    zones: Vec<Zone>,
    #[serde(default)]
    supports: Vec<Support>,
    #[serde(default)]
    discount: Vec<Discount>,
    #[serde(default)]
    income: Option<Income>,
    #[serde(default)]
    sell_bonus: f64,
    #[serde(default)]
    round_start: Option<TimedEffect>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAttack {
    tag: String,
    #[serde(default)]
    range: Option<f64>,
    #[serde(default = "first")]
    targeting: Targeting,
    cooldown: f64,
    damage: f64,
    pierce: u32,
    #[serde(default)]
    cannot_damage: Vec<String>,
    #[serde(default)]
    camo: bool,
    delivery: Delivery,
    #[serde(default)]
    extras: HitExtras,
    #[serde(default)]
    crit: Option<Crit>,
    #[serde(default)]
    burst: Option<Burst>,
}

fn first() -> Targeting {
    Targeting::First
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGroup {
    bloon: String,
    #[serde(default)]
    flags: Vec<String>,
    count: u32,
    start: f64,
    end: f64,
}

fn parse_mask(rows: &[String], w: usize, h: usize, what: &str) -> Result<Vec<bool>, DataError> {
    if rows.len() != h || rows.iter().any(|r| r.chars().count() != w) {
        return Err(invalid(format!("{what}: erwartet {h} Zeilen à {w} Zeichen")));
    }
    rows.iter()
        .flat_map(|r| r.chars())
        .map(|c| match c {
            '#' => Ok(true),
            '.' => Ok(false),
            o => Err(invalid(format!("{what}: unbekanntes Zeichen {o:?}"))),
        })
        .collect()
}

impl GameV1 {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DataError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|source| DataError::Io { path: path.display().to_string(), source })?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> Result<Self, DataError> {
        let raw: Raw = serde_json::from_str(text)?;
        if raw.format != FORMAT {
            return Err(invalid(format!("Format {:?} nicht unterstützt (erwartet {FORMAT})", raw.format)));
        }
        if raw.bloons.len() > BloonKind::MAX as usize || raw.towers.len() > TowerKind::MAX as usize {
            return Err(invalid("zu viele Bloon- oder Turmtypen"));
        }
        check_unique(raw.bloons.iter().map(|b| b.name.as_str()), "Bloon")?;
        check_unique(raw.towers.iter().map(|t| t.name.as_str()), "Turm")?;
        let bloon_id = |name: &str| -> Result<BloonKind, DataError> {
            raw.bloons
                .iter()
                .position(|b| b.name == name)
                .map(|i| i as BloonKind)
                .ok_or_else(|| invalid(format!("unbekannter Bloon {name:?}")))
        };

        let bloons = raw
            .bloons
            .iter()
            .map(|b| {
                let what = format!("Bloon {}", b.name);
                if !(b.grow_rate.is_finite() && b.grow_rate >= 0.0) || (b.grow_rate > 0.0 && !b.regrow) {
                    return Err(invalid(format!(
                        "{what}: grow_rate {} (nur >= 0 und nur mit regrow)",
                        b.grow_rate
                    )));
                }
                Ok(BloonDefV1 {
                    name: b.name.clone(),
                    hp: b.hp,
                    speed: b.speed,
                    leak: b.leak,
                    children: b
                        .children
                        .iter()
                        .map(|(n, c)| Ok((bloon_id(n)?, *c)))
                        .collect::<Result<_, DataError>>()?,
                    props: Props::parse(&b.props, &what)?,
                    classes: Classes::parse(&b.classes, &what)?,
                    camo: b.camo,
                    regrow: b.regrow,
                    grow_rate: b.grow_rate,
                    radius: b.radius,
                    tags: b.tags.clone(),
                    decamo: b.decamo.as_deref().map(bloon_id).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, DataError>>()?;

        let towers = raw.towers.iter().map(resolve_tower).collect::<Result<Vec<_>, _>>()?;
        let filters_by_set =
            towers.iter().flat_map(|t| &t.variants).flat_map(|v| &v.supports).any(|s| !s.sets().is_empty());
        if let Some(t) = raw.towers.iter().find(|t| filters_by_set && t.set.is_none()) {
            return Err(invalid(format!("Turm {}: set fehlt (Supports filtern nach Turmgruppen)", t.name)));
        }

        let rounds = raw
            .rounds
            .iter()
            .enumerate()
            .map(|(i, groups)| {
                groups
                    .iter()
                    .map(|g| {
                        Ok(SpawnGroup {
                            kind: bloon_id(&g.bloon)?,
                            flags: Flags::parse(&g.flags, &format!("Runde {}", i + 1))?,
                            count: g.count,
                            start: g.start,
                            end: g.end,
                        })
                    })
                    .collect::<Result<Vec<_>, DataError>>()
            })
            .collect::<Result<Vec<_>, DataError>>()?;

        let m = &raw.map;
        let g = &m.grid;
        let map = MapV1 {
            name: m.name.clone(),
            bounds: m.bounds,
            points: m.points.clone(),
            cum: m.cum.clone(),
            grid: GridV1 {
                x0: g.x0,
                y0: g.y0,
                cell: g.cell,
                w: g.w,
                h: g.h,
                path: parse_mask(&g.path, g.w, g.h, "map.grid.path")?,
                buildable: parse_mask(&g.buildable, g.w, g.h, "map.grid.buildable")?,
            },
            placement: m.placement,
            areas: m.areas.clone(),
        };

        let data = GameV1 {
            name: raw.name,
            meta: raw.meta,
            rules: raw.rules,
            map,
            bloons,
            towers,
            rounds,
            income: raw.income,
        };
        data.validate()?;
        Ok(data)
    }

    /// Prüft alles, worauf sich der Core verlässt (auch für von Hand gebaute Daten).
    pub fn validate(&self) -> Result<(), DataError> {
        let r = &self.rules;
        let pos = |x: f64| x.is_finite() && x > 0.0;
        let non_neg = |x: f64| x.is_finite() && x >= 0.0;
        if !pos(r.tick_rate) || !pos(r.max_round_seconds) || r.max_round_seconds * r.tick_rate > 1e7 {
            return Err(invalid("rules: tick_rate/max_round_seconds ungültig"));
        }
        if !non_neg(r.cd_eps) || !non_neg(r.child_spacing) || !(0.0..=1.0).contains(&r.sell_ratio) {
            return Err(invalid("rules: cd_eps/child_spacing >= 0, sell_ratio in [0, 1] erwartet"));
        }
        if r.start_lives <= 0 || !(0..=crate::MAX_DOLLARS).contains(&r.start_cash) {
            return Err(invalid("rules: start_lives > 0, start_cash in [0, 1e12] erwartet"));
        }
        if !pos(r.cost_multiplier) || r.cost_multiplier > 100.0 || !(1..=1_000_000).contains(&r.cost_rounding)
        {
            return Err(invalid("rules: cost_multiplier in (0, 100] und cost_rounding in [1, 1e6] erwartet"));
        }
        if r.start_round < 1 || r.end_round < r.start_round || r.end_round as usize > self.rounds.len() {
            return Err(invalid("rules: 1 <= start_round <= end_round <= Anzahl Runden erwartet"));
        }
        check_unique(r.locked_towers.iter().map(String::as_str), "rules.locked_towers: Turm")?;
        if let Some(n) = r.locked_towers.iter().find(|n| !self.towers.iter().any(|t| &t.name == *n)) {
            return Err(invalid(format!("rules.locked_towers: unbekannter Turm {n:?}")));
        }
        if self.income.len() < r.end_round as usize
            || self.income.iter().any(|i| !non_neg(i.cash_per_pop) || !non_neg(i.end_bonus))
        {
            return Err(invalid("income: ein Eintrag >= 0 pro Runde bis end_round erwartet"));
        }

        let m = &self.map;
        if m.points.len() < 2 || m.points.len() > u16::MAX as usize || m.cum.len() != m.points.len() {
            return Err(invalid("map: 2 bis 65535 Punkte, cum passend zu points"));
        }
        if m.cum[0] != 0.0
            || m.cum.iter().any(|c| !c.is_finite())
            || m.cum.windows(2).any(|w| w[1] < w[0])
            || !pos(*m.cum.last().unwrap())
            || m.points.iter().flatten().any(|v| !v.is_finite())
        {
            return Err(invalid("map: cum muss bei 0 beginnen und monoton steigen, points endlich"));
        }
        for (i, w) in m.points.windows(2).enumerate() {
            let len = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            if (m.cum[i + 1] - m.cum[i] - len).abs() > 1e-9 * len.max(1.0) {
                return Err(invalid(format!("map: cum passt bei Segment {i} nicht zu points")));
            }
        }
        let [x0, y0, x1, y1] = m.bounds;
        if !(x0 < x1 && y0 < y1) || m.bounds.iter().any(|v| !v.is_finite()) {
            return Err(invalid("map: bounds ungültig"));
        }
        let gr = &m.grid;
        let cells = gr.w.checked_mul(gr.h).filter(|&c| c > 0 && c <= u16::MAX as usize);
        if cells.is_none() || !pos(gr.cell) || !gr.x0.is_finite() || !gr.y0.is_finite() {
            return Err(invalid("map.grid: 1 bis 65535 Zellen, cell > 0"));
        }
        let (gx1, gy1) = (gr.x0 + gr.w as f64 * gr.cell, gr.y0 + gr.h as f64 * gr.cell);
        if gr.x0 >= x1 || gr.y0 >= y1 || gx1 <= x0 || gy1 <= y0 {
            return Err(invalid("map.grid: Raster liegt komplett außerhalb der bounds"));
        }
        if m.placement == Placement::Areas && m.areas.is_empty() {
            return Err(invalid("map: placement = areas braucht Flächen"));
        }
        if m.areas.iter().any(|a| a.polygon.len() < 3 || a.polygon.iter().flatten().any(|v| !v.is_finite())) {
            return Err(invalid("map.areas: Polygone mit >= 3 endlichen Punkten erwartet"));
        }

        for b in &self.bloons {
            if b.decamo.is_some_and(|k| !b.camo || self.bloons[k as usize].camo) {
                return Err(invalid(format!(
                    "Bloon {}: decamo nur von einem Camo- auf einen Nicht-Camo-Typ",
                    b.name
                )));
            }
            if !non_neg(b.radius) {
                return Err(invalid(format!("Bloon {}: radius >= 0 erwartet", b.name)));
            }
            if !pos(b.hp)
                || !pos(b.speed)
                || (b.speed as f32) * (r.dt() as f32) <= 0.0
                || !(1..=MAX_LEAK).contains(&b.leak)
            {
                return Err(invalid(format!("Bloon {}: hp, speed > 0, leak in [1, 1e6] erwartet", b.name)));
            }
        }
        self.check_child_cycles()?;
        // Splitten ist rekursiv: zu viele Nachkommen würden einen Treffer beliebig teuer machen
        let mut desc: Vec<Option<f64>> = vec![None; self.bloons.len()];
        for i in 0..self.bloons.len() {
            if descendants(&self.bloons, i, &mut desc) > MAX_DESCENDANTS {
                return Err(invalid(format!("Bloon {}: mehr als 1e5 Nachkommen", self.bloons[i].name)));
            }
        }
        // Kollision Projektil/Bloon braucht einen Radius > 0 auf mindestens einer Seite
        let min_bloon_radius = self.bloons.iter().map(|b| b.radius).fold(f64::INFINITY, f64::min);
        let tags: HashSet<&str> =
            self.bloons.iter().flat_map(|b| b.tags.iter().map(String::as_str)).collect();

        for t in &self.towers {
            if !(1..=crate::MAX_DOLLARS).contains(&t.cost)
                || t.upgrade_costs.iter().flatten().any(|c| !(1..=crate::MAX_DOLLARS).contains(c))
            {
                return Err(invalid(format!("Turm {}: Kosten in [1, 1e12] erwartet", t.name)));
            }
            if t.variant([0, 0, 0]).is_none() {
                return Err(invalid(format!("Turm {}: Basisvariante 0-0-0 fehlt", t.name)));
            }
            for v in &t.variants {
                let what = format!("Turm {} {:?}", t.name, v.tiers);
                for (p, &tier) in v.tiers.iter().enumerate() {
                    if tier as usize > t.upgrade_costs[p].len() {
                        return Err(invalid(format!(
                            "{what}: keine Upgrade-Kosten für Pfad {p} Tier {tier}"
                        )));
                    }
                }
                if !pos(v.range) || !non_neg(v.footprint) {
                    return Err(invalid(format!("{what}: range > 0, footprint >= 0 erwartet")));
                }
                for z in &v.zones {
                    if !pos(z.radius) || !(0.0..=1.0).contains(&z.speed_scale) || !z.filter.valid(&tags) {
                        return Err(invalid(format!("{what}: ungültige Zone")));
                    }
                }
                // Platzhalter ohne Wirkung nur als Basisvariante gesperrter Türme, deren Mechanik noch fehlt
                // (docs/design-towers-step3.md §4): die Turmliste und damit das Layout bleiben stabil
                if v.attacks.is_empty()
                    && v.zones.is_empty()
                    && v.supports.is_empty()
                    && v.discount.is_empty()
                    && v.income.is_none()
                    && !(v.tiers == [0; PATHS] && r.locked_towers.contains(&t.name))
                {
                    return Err(invalid(format!(
                        "{what}: weder Angriff, Zone, Support, Rabatt noch Einkommen (nur als Basis gesperrter Türme erlaubt)"
                    )));
                }
                if !v.supports.iter().all(Support::valid) {
                    return Err(invalid(format!("{what}: ungültiger Support")));
                }
                if !v.discount.iter().all(|d| {
                    d.fraction.is_finite()
                        && d.fraction > 0.0
                        && d.fraction < 1.0
                        && d.tier_min <= d.tier_cap
                        && d.tier_cap as usize <= MAX_TIER
                        && !d.stack.is_empty()
                        && d.stack_limit >= 1
                }) {
                    return Err(invalid(format!("{what}: ungültiger Rabatt")));
                }
                if v.income.is_some_and(|i| {
                    !non_neg(i.per_round) || i.per_round > 1e9 || !(0..=MAX_INCOME_LIVES).contains(&i.lives)
                }) {
                    return Err(invalid(format!("{what}: ungültiges Einkommen")));
                }
                if v.round_start.as_ref().is_some_and(|e| !self.effect_valid(e, &tags)) {
                    return Err(invalid(format!("{what}: ungültiger Effekt zu Rundenbeginn")));
                }
                if !v.sell_bonus.is_finite() || !(0.0..=1.0).contains(&(r.sell_ratio + v.sell_bonus)) {
                    return Err(invalid(format!("{what}: sell_ratio + sell_bonus in [0, 1] erwartet")));
                }
                for a in &v.attacks {
                    let ok = pos(a.cooldown)
                        && non_neg(a.damage)
                        && a.crit.map_or(true, |c| {
                            non_neg(c.damage)
                                && c.every >= 1
                                && c.every_range.map_or(true, |[lo, hi]| lo >= 1 && lo <= hi)
                        })
                        && a.burst.map_or(true, |b| b.count >= 1 && non_neg(b.interval))
                        && a.range.map_or(true, pos)
                        && match &a.delivery {
                            Delivery::Hitscan {} => true,
                            Delivery::Area { radius, radius_offset } => {
                                let range = a.range.unwrap_or(v.range);
                                radius.map_or(true, pos) && radius_offset.map_or(true, |o| pos(range + o))
                            }
                            Delivery::HitscanSplash { radius } => pos(*radius),
                            Delivery::Potion { effect: e } => {
                                a.targeting == Targeting::Friendly && self.effect_valid(e, &tags)
                            }
                            Delivery::Lob { time, speed, spread, on_arrive, ground } => {
                                time.is_some() != speed.is_some()
                                    && time.map_or(true, pos)
                                    && speed.map_or(true, pos)
                                    && non_neg(*spread)
                                    && (!on_arrive.is_empty() || ground.is_some())
                                    && on_arrive.len() <= 8
                                    && on_arrive.iter().all(|c| c.valid(false, &tags))
                                    && ground.as_ref().map_or(true, |g| {
                                        pos(g.lifespan)
                                            && pos(g.radius)
                                            && g.pierce >= 1
                                            && non_neg(g.damage)
                                            && non_neg(g.collide_every)
                                            && g.clear_hits.map_or(true, pos)
                                            && Props::parse(&g.cannot_damage, "").is_ok()
                                            && g.extras.valid(&tags)
                                            && g.on_exhaust.len() <= 8
                                            && g.on_exhaust.iter().all(|c| c.valid(false, &tags))
                                    })
                            }
                            Delivery::Projectile {
                                speed,
                                lifespan,
                                radius,
                                emission,
                                on_contact,
                                path,
                                clear_hits,
                                on_exhaust,
                                homing,
                            } => {
                                a.pierce >= 1 // ohne Pierce wäre das Projektil sofort verbraucht
                                    && homing.map_or(true, |h| {
                                        pos(h.turn_rate)
                                            && pos(h.distance)
                                            && (0.0..=360.0).contains(&h.max_angle)
                                            && path.is_none()
                                    })
                                    && clear_hits.map_or(true, |c| c > 0.0)
                                    && on_exhaust.len() <= 8
                                    && on_contact.len() <= 8
                                    && (*radius > 0.0 || min_bloon_radius > 0.0)
                                    && path.as_ref().map_or(true, |p| {
                                        p.len() >= 2
                                            && p.len() <= 10_000
                                            && p.iter().flatten().all(|v| v.is_finite())
                                    })
                                    && pos(*speed)
                                    && pos(*lifespan)
                                    && non_neg(*radius)
                                    && emission.valid()
                                    && on_contact
                                        .iter()
                                        .chain(on_exhaust.iter())
                                        .all(|c| c.valid(true, &tags))
                            }
                        };
                    if !ok {
                        return Err(invalid(format!("{what} Angriff {}: ungültige Werte", a.tag)));
                    }
                    // Schaden und Crit stecken beim Wurf in den Explosionen, nicht im Angriff; ein Trank hat keinen
                    let potion = matches!(a.delivery, Delivery::Potion { .. });
                    if matches!(a.delivery, Delivery::Lob { .. } | Delivery::Potion { .. })
                        && (a.damage != 0.0 || a.crit.is_some())
                        || potion && (a.burst.is_some() || a.camo || a.extras != HitExtras::default())
                    {
                        return Err(invalid(format!(
                            "{what} Angriff {}: Wurf/Trank ohne damage/crit (Trank auch ohne burst/camo/extras)",
                            a.tag
                        )));
                    }
                    if a.targeting == Targeting::TrackSpread && !matches!(a.delivery, Delivery::Lob { .. }) {
                        return Err(invalid(format!("{what} Angriff {}: track_spread nur mit Wurf", a.tag)));
                    }
                    if a.targeting == Targeting::Friendly && !matches!(a.delivery, Delivery::Potion { .. }) {
                        return Err(invalid(format!("{what} Angriff {}: friendly nur mit Trank", a.tag)));
                    }
                    if a.targeting == Targeting::TrackBest && a.delivery.track_best_radius().is_none() {
                        return Err(invalid(format!(
                            "{what} Angriff {}: track_best nur mit Wurf und Explosion",
                            a.tag
                        )));
                    }
                    if !a.extras.valid(&tags) {
                        return Err(invalid(format!("{what} Angriff {}: ungültige extras", a.tag)));
                    }
                }
            }
        }

        for (i, round) in self.rounds.iter().enumerate() {
            for g in round {
                if !non_neg(g.start)
                    || !g.end.is_finite()
                    || g.end < g.start
                    || !(1..=MAX_GROUP).contains(&g.count)
                {
                    return Err(invalid(format!("Runde {}: ungültige Gruppe", i + 1)));
                }
            }
        }
        Ok(())
    }

    fn check_child_cycles(&self) -> Result<(), DataError> {
        fn visit(defs: &[BloonDefV1], i: usize, state: &mut [u8]) -> bool {
            match state[i] {
                1 => return false,
                2 => return true,
                _ => {}
            }
            state[i] = 1;
            let ok = defs[i].children.iter().all(|&(c, _)| visit(defs, c as usize, state));
            state[i] = 2;
            ok
        }
        let mut state = vec![0u8; self.bloons.len()];
        for i in 0..self.bloons.len() {
            if !visit(&self.bloons, i, &mut state) {
                return Err(invalid(format!("Bloon {}: Zyklus in den Kindern", self.bloons[i].name)));
            }
        }
        Ok(())
    }

    /// Kosten nach Regeln (Medium × Faktor, auf `cost_rounding` gerundet; mindestens `cost_rounding`).
    pub fn cost(&self, medium: i64) -> i64 {
        self.cost_discounted(medium, 0.0)
    }

    /// Kosten mit Rabatt (Anteil, Village-Rabatt-Zonen): Medium × (1 − Rabatt) × Faktor, gerundet
    /// wie `cost` [KAL: Reihenfolge Rabatt/Schwierigkeit/Rundung]. Ohne Rabatt bitgleich zu `cost`.
    pub fn cost_discounted(&self, medium: i64, discount: f64) -> i64 {
        let r = &self.rules;
        let scaled = medium as f64 * (1.0 - discount) * r.cost_multiplier / r.cost_rounding as f64;
        // half-up auf das Vielfache (Rundungsart ist ein Kalibrierpunkt, docs/engine-design.md §9)
        ((scaled + 0.5).floor() as i64).max(1) * r.cost_rounding
    }
}

impl RulesV1 {
    pub fn dt(&self) -> f64 {
        1.0 / self.tick_rate
    }
}

const MAX_LEAK: i32 = 1_000_000;
/// Obergrenzen für Support-Werte (gegen Überläufe beim Aufaddieren in der Engine).
const MAX_PIERCE_ADD: u32 = 1_000_000;
const MAX_INCOME_LIVES: i32 = 1_000_000;
const MAX_GROUP: u32 = 100_000;
const MAX_DESCENDANTS: f64 = 100_000.0;

/// Anzahl Bloons, die beim kompletten Platzen von `i` insgesamt entstehen (inkl. `i`).
/// Zyklen sind vorher ausgeschlossen.
fn descendants(defs: &[BloonDefV1], i: usize, memo: &mut [Option<f64>]) -> f64 {
    if let Some(n) = memo[i] {
        return n;
    }
    let n = 1.0
        + defs[i].children.iter().map(|&(c, k)| k as f64 * descendants(defs, c as usize, memo)).sum::<f64>();
    memo[i] = Some(n);
    n
}

fn resolve_tower(t: &RawTower) -> Result<TowerDefV1, DataError> {
    let what = format!("Turm {}", t.name);
    if t.upgrade_costs.len() != PATHS || t.upgrade_costs.iter().any(|p| p.len() > MAX_TIER) {
        return Err(invalid(format!(
            "{what}: {PATHS} Upgrade-Pfade mit höchstens {MAX_TIER} Stufen erwartet"
        )));
    }
    if t.variants.len() > u16::MAX as usize - 1 {
        return Err(invalid(format!("{what}: zu viele Varianten")));
    }
    let mut variant_index = vec![u16::MAX; (MAX_TIER + 1).pow(PATHS as u32)];
    let mut variants = Vec::with_capacity(t.variants.len());
    for (i, v) in t.variants.iter().enumerate() {
        let idx = tiers_index(v.tiers).ok_or_else(|| invalid(format!("{what}: Tier > {MAX_TIER}")))?;
        if variant_index[idx] != u16::MAX {
            return Err(invalid(format!("{what}: Variante {:?} doppelt", v.tiers)));
        }
        variant_index[idx] = i as u16;
        variants.push(VariantV1 {
            tiers: v.tiers,
            range: v.range,
            footprint: v.footprint,
            zones: v.zones.clone(),
            supports: v.supports.clone(),
            discount: v.discount.clone(),
            income: v.income,
            sell_bonus: v.sell_bonus,
            round_start: v.round_start.clone(),
            attacks: v
                .attacks
                .iter()
                .map(|a| {
                    Ok(AttackV1 {
                        tag: a.tag.clone(),
                        range: a.range,
                        targeting: a.targeting,
                        cooldown: a.cooldown,
                        damage: a.damage,
                        pierce: a.pierce,
                        cannot_damage: Props::parse(&a.cannot_damage, &what)?,
                        camo: a.camo,
                        delivery: a.delivery.clone(),
                        extras: a.extras.clone(),
                        crit: a.crit,
                        burst: a.burst,
                    })
                })
                .collect::<Result<_, DataError>>()?,
        });
    }
    let mut costs: [Vec<i64>; PATHS] = Default::default();
    for (p, c) in t.upgrade_costs.iter().enumerate() {
        costs[p] = c.clone();
    }
    Ok(TowerDefV1 {
        name: t.name.clone(),
        game_id: t.game_id.clone(),
        set: t.set.unwrap_or_default(),
        cost: t.cost,
        upgrade_costs: costs,
        variants,
        variant_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_sim_v0_export() {
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.game_v1.json");
        let g = GameV1::load(path).unwrap();
        assert_eq!((g.rounds.len(), g.towers.len(), g.bloons.len()), (40, 4, 9));
        let dart = &g.towers[0];
        assert_eq!(dart.variants.len(), 15, "3x3 Tiers ohne 3/3 auf zwei Pfaden");
        assert!(dart.variant([3, 2, 0]).is_some() && dart.variant([3, 3, 0]).is_none());
        assert_eq!(g.cost(dart.cost), 200);
        let black = g.bloons.iter().find(|b| b.name == "black").unwrap();
        assert_eq!(black.children.len(), 1);
        assert_eq!(black.children[0].1, 2);
        assert!(black.props.intersects(Props(Props::BLACK)));
        assert_eq!(g.income[0].end_bonus, 101.0);
    }

    #[test]
    fn meta_and_game_ids_are_optional() {
        let parse = |v: serde_json::Value| GameV1::from_json(&v.to_string());
        assert_eq!(parse(mini()).unwrap().meta, None, "Fixture ohne meta");
        let mut v = mini();
        v["meta"] = serde_json::json!({"map": "Tutorial", "mode": "Clicks", "difficulty": "Hard", "game_version": "57.4"});
        v["towers"][0]["game_id"] = serde_json::json!("DartMonkey");
        let d = parse(v.clone()).unwrap();
        assert_eq!(d.meta.as_ref().unwrap().mode, "Clicks");
        assert_eq!(d.towers[0].game_id.as_deref(), Some("DartMonkey"));
        assert_eq!(d.towers[1].game_id, None);
        v["meta"]["extra"] = serde_json::json!(1);
        assert!(parse(v).is_err(), "unbekannte Felder in meta werden abgelehnt");
    }

    fn mini() -> serde_json::Value {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../data/fixtures/game_v1_mini.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn rejects(f: impl FnOnce(&mut serde_json::Value), needle: &str) {
        let mut v = mini();
        f(&mut v);
        match GameV1::from_json(&v.to_string()) {
            Err(e) => assert!(e.to_string().contains(needle), "{e} enthält nicht {needle:?}"),
            Ok(_) => panic!("sollte abgelehnt werden: {needle}"),
        }
    }

    #[test]
    fn loads_mini_fixture() {
        let g = GameV1::from_json(&mini().to_string()).unwrap();
        assert_eq!(g.towers[0].variants.len(), 10);
        assert!(g.towers[0].variant([1, 1, 1]).is_none());
        assert_eq!(g.map.placement, Placement::Areas);
        assert_eq!(g.rounds[1][1].flags, Flags(Flags::CAMO));
        assert_eq!(g.bloons[3].classes, Classes(Classes::CERAMIC));
    }

    #[test]
    fn floats_roundtrip_exactly() {
        let mut v = mini();
        v["rounds"][0][0]["end"] = serde_json::json!(0.9843489761028231);
        let g = GameV1::from_json(&v.to_string()).unwrap();
        assert_eq!(g.rounds[0][0].end.to_bits(), 0.9843489761028231f64.to_bits());
    }

    #[test]
    fn rejects_invalid_data() {
        use serde_json::json;
        rejects(|v| v["format"] = json!("sim_v0"), "Format");
        rejects(|v| v["extra"] = json!(1), "unknown field");
        rejects(|v| v["bloons"][1]["children"] = json!([["green", 1]]), "unbekannter Bloon");
        rejects(|v| v["bloons"][0]["children"] = json!([["blue", 1]]), "Zyklus");
        rejects(|v| v["bloons"][1]["name"] = json!("red"), "doppelt");
        rejects(|v| v["bloons"][0]["props"] = json!(["glass"]), "Eigenschaft");
        rejects(|v| v["bloons"][0]["speed"] = json!(1e-300), "speed");
        rejects(|v| v["rounds"][0][0]["flags"] = json!(["shiny"]), "Flag");
        rejects(|v| v["rounds"][0][0]["count"] = json!(0), "Gruppe");
        rejects(|v| v["map"]["grid"]["path"] = json!(["#"]), "map.grid.path");
        rejects(|v| v["map"]["cum"] = json!([0.0, 100.0]), "Segment 0");
        rejects(|v| v["map"]["areas"] = json!([]), "placement = areas");
        rejects(|v| v["map"]["bounds"] = json!([1.0, 0.0, 0.0, 1.0]), "bounds");
        rejects(|v| v["rules"]["end_round"] = json!(4), "end_round");
        rejects(|v| v["rules"]["start_round"] = json!(0), "start_round");
        rejects(|v| v["rules"]["cost_rounding"] = json!(0), "cost_rounding");
        rejects(|v| v["income"] = json!([]), "income");
        rejects(|v| v["towers"][0]["variants"][1]["tiers"] = json!([0, 0, 0]), "doppelt");
        rejects(|v| v["towers"][0]["variants"][0]["tiers"] = json!([0, 0, 2]), "0-0-0 fehlt");
        rejects(|v| v["towers"][0]["upgrade_costs"] = json!([[50], [60], [40]]), "Upgrade-Kosten");
        rejects(|v| v["towers"][0]["upgrade_costs"] = json!([[50], [60]]), "Upgrade-Pfade");
        rejects(|v| v["towers"][0]["variants"][0]["attacks"][0]["cooldown"] = json!(0.0), "ungültige Werte");
        rejects(
            |v| {
                v["towers"][1]["variants"][0]["attacks"][0]["delivery"] =
                    json!({"kind": "hitscan_splash", "radius": 0.0})
            },
            "ungültige Werte",
        );
        rejects(
            |v| {
                v["towers"][0]["variants"][0]["attacks"][0]["delivery"] =
                    json!({"kind": "hitscan", "radius": 8.0})
            },
            "unknown field",
        );
        rejects(|v| v["rules"]["difficulty"] = json!("hard"), "unknown field");
        rejects(|v| v["income"][0]["bonus"] = json!(1), "unknown field");
        rejects(|v| v["rules"]["cost_multiplier"] = json!(1e300), "cost_multiplier");
        rejects(|v| v["bloons"][3]["children"] = json!([["blue", 60000], ["lead", 2]]), "Nachkommen");
        rejects(|v| v["bloons"][0]["leak"] = json!(10_000_000), "leak");
        rejects(|v| v["map"]["grid"]["x0"] = json!(500.0), "außerhalb");
    }

    /// Mini-Daten mit einem Support-Turm (Village-artig: keine Angriffe, Auren + Rabatt) und einem
    /// Einkommens-Turm (Farm-artig), die Farm gesperrt.
    fn with_support_towers() -> serde_json::Value {
        use serde_json::json;
        let mut v = mini();
        let towers = v["towers"].as_array_mut().unwrap();
        for t in towers.iter_mut() {
            t["set"] = json!("primary"); // Pflicht, weil Supports nach Turmgruppen filtern
        }
        towers.push(json!({
            "name": "village", "set": "support", "cost": 1000, "upgrade_costs": [[], [], [100]],
            "variants": [
                {"tiers": [0, 0, 0], "range": 40.0, "footprint": 11.0, "attacks": [], "supports": [
                    {"kind": "range", "group": "Range:Support", "add_frac": 0.1},
                    {"kind": "rate", "group": "Village:Rate", "factor": 0.8},
                    {"kind": "camo", "group": "Village:Visibility"},
                    {"kind": "pierce", "group": "P", "sets": ["primary"], "add": 1},
                    {"kind": "projectile_speed", "group": "S", "sets": ["primary"], "add_frac": 0.2},
                    {"kind": "damage_all", "group": "Village:DamageType"}]},
                {"tiers": [0, 0, 1], "range": 40.0, "footprint": 11.0, "attacks": [], "discount": [
                    {"fraction": 0.1, "tier_min": 0, "tier_cap": 3, "stack": "major", "stack_limit": 1}]}
            ]
        }));
        towers.push(json!({
            "name": "farm", "set": "support", "cost": 1000, "upgrade_costs": [[], [], []],
            "variants": [{"tiers": [0, 0, 0], "range": 40.0, "footprint": 21.0, "attacks": [],
                          "income": {"per_round": 80.0}, "sell_bonus": 0.1}]
        }));
        v["rules"]["locked_towers"] = json!(["farm"]);
        v
    }

    #[test]
    fn loads_support_and_income_towers() {
        let g = GameV1::from_json(&with_support_towers().to_string()).unwrap();
        let n = g.towers.len();
        let (village, farm) = (&g.towers[n - 2], &g.towers[n - 1]);
        assert_eq!((village.set, g.towers[0].set), (TowerSet::Support, TowerSet::Primary));
        let base = village.variant([0, 0, 0]).unwrap();
        assert!(base.attacks.is_empty() && base.supports.len() == 6);
        assert_eq!(
            base.supports[1],
            Support::Rate { group: "Village:Rate".into(), sets: vec![], factor: 0.8 }
        );
        assert_eq!(village.variant([0, 0, 1]).unwrap().discount[0].tier_cap, 3);
        let f = farm.variant([0, 0, 0]).unwrap();
        assert_eq!((f.income, f.sell_bonus), (Some(Income { per_round: 80.0, lives: 0 }), 0.1));
        assert_eq!(g.rules.locked_towers, vec!["farm".to_string()]);
        // Platzhalter ohne Wirkung: nur für gesperrte Türme
        let mut v = with_support_towers();
        let t = v["towers"].as_array().unwrap().len() - 1;
        v["towers"][t]["variants"][0].as_object_mut().unwrap().remove("income");
        assert!(GameV1::from_json(&v.to_string()).unwrap().towers[t].variants[0].attacks.is_empty());
        let mut v = with_support_towers();
        v["towers"][0]["variants"][0]["attacks"][0]["delivery"] =
            serde_json::json!({"kind": "area", "radius": 30.0, "radius_offset": 2.0});
        let g = GameV1::from_json(&v.to_string()).unwrap();
        assert_eq!(
            g.towers[0].variants[0].attacks[0].delivery,
            Delivery::Area { radius: Some(30.0), radius_offset: Some(2.0) }
        );
        // ohne die neuen Felder: Standardwerte (sim_v0 und ältere Daten laden weiter)
        let v0 = &GameV1::from_json(&mini().to_string()).unwrap().towers[0];
        let b = v0.variant([0, 0, 0]).unwrap();
        assert!(b.supports.is_empty() && b.discount.is_empty() && b.income.is_none() && b.sell_bonus == 0.0);
    }

    #[test]
    fn rejects_invalid_support_data() {
        use serde_json::json;
        let rejects = |f: &dyn Fn(&mut serde_json::Value), needle: &str| {
            let mut v = with_support_towers();
            f(&mut v);
            match GameV1::from_json(&v.to_string()) {
                Err(e) => assert!(e.to_string().contains(needle), "{e} enthält nicht {needle:?}"),
                Ok(_) => panic!("sollte abgelehnt werden: {needle}"),
            }
        };
        let village = |v: &mut serde_json::Value| -> usize { v["towers"].as_array().unwrap().len() - 2 };
        let sup = |v: &mut serde_json::Value, i: usize, val: serde_json::Value| {
            let t = village(v);
            v["towers"][t]["variants"][0]["supports"][i] = val;
        };
        rejects(&|v| v["rules"]["locked_towers"] = json!(["castle"]), "unbekannter Turm");
        rejects(
            &|v| {
                let t = v["towers"].as_array().unwrap().len() - 1;
                v["towers"][t]["variants"][0].as_object_mut().unwrap().remove("income");
                v["rules"]["locked_towers"] = json!([]);
            },
            "nur als Basis gesperrter Türme",
        );
        rejects(
            &|v| {
                let t = v["towers"].as_array().unwrap().len() - 1;
                v["towers"][t]["upgrade_costs"] = json!([[], [], [100]]);
                v["towers"][t]["variants"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"tiers": [0, 0, 1], "range": 40.0, "footprint": 21.0, "attacks": []}));
            },
            "nur als Basis gesperrter Türme",
        );
        rejects(&|v| v["rules"]["locked_towers"] = json!(["farm", "farm"]), "doppelt");
        rejects(&|v| sup(v, 1, json!({"kind": "rate", "group": "R", "factor": 1.5})), "ungültiger Support");
        rejects(&|v| sup(v, 1, json!({"kind": "rate", "group": "R", "factor": 0.0})), "ungültiger Support");
        rejects(&|v| sup(v, 0, json!({"kind": "range", "group": "", "add_frac": 0.1})), "ungültiger Support");
        rejects(
            &|v| sup(v, 0, json!({"kind": "range", "group": "R", "add_frac": -0.1})),
            "ungültiger Support",
        );
        rejects(&|v| sup(v, 0, json!({"kind": "range", "group": "R", "mul": 0.1})), "unknown field");
        rejects(&|v| sup(v, 0, json!({"kind": "slow", "group": "R"})), "unknown variant");
        rejects(
            &|v| sup(v, 3, json!({"kind": "pierce", "group": "P", "sets": ["hero"], "add": 1})),
            "unknown variant",
        );
        rejects(
            &|v| sup(v, 3, json!({"kind": "pierce", "group": "P", "sets": ["magic", "magic"], "add": 1})),
            "ungültiger Support",
        );
        rejects(
            &|v| sup(v, 3, json!({"kind": "pierce", "group": "P", "add": 2_000_000})),
            "ungültiger Support",
        );
        rejects(
            &|v| {
                let t = village(v);
                v["towers"][t].as_object_mut().unwrap().remove("set");
            },
            "set fehlt",
        );
        let discount = |field: &'static str, val: serde_json::Value| {
            move |v: &mut serde_json::Value| {
                let t = v["towers"].as_array().unwrap().len() - 2;
                v["towers"][t]["variants"][1]["discount"][0][field] = val.clone();
            }
        };
        rejects(&discount("tier_min", json!(4)), "ungültiger Rabatt");
        rejects(&discount("stack_limit", json!(0)), "ungültiger Rabatt");
        rejects(&discount("stack", json!("")), "ungültiger Rabatt");
        rejects(&discount("percent", json!(10)), "unknown field");
        let income = |field: &'static str, val: serde_json::Value| {
            move |v: &mut serde_json::Value| {
                let t = v["towers"].as_array().unwrap().len() - 1;
                v["towers"][t]["variants"][0]["income"][field] = val.clone();
            }
        };
        rejects(&income("lives", json!(-1)), "ungültiges Einkommen");
        rejects(&income("bananas", json!(4)), "unknown field");
        rejects(
            &|v| {
                v["towers"][0]["variants"][0]["attacks"][0]["delivery"] =
                    json!({"kind": "area", "radius": 30.0, "radius_offset": -1e9})
            },
            "ungültige Werte",
        );
        rejects(
            &|v| {
                let t = village(v);
                v["towers"][t]["variants"][0]["supports"] = json!([]);
            },
            "weder Angriff",
        );
        rejects(
            &|v| {
                let t = village(v);
                v["towers"][t]["variants"][1]["discount"][0]["fraction"] = json!(1.0);
            },
            "ungültiger Rabatt",
        );
        rejects(
            &|v| {
                let t = village(v);
                v["towers"][t]["variants"][1]["discount"][0]["tier_cap"] = json!(6);
            },
            "ungültiger Rabatt",
        );
        rejects(
            &|v| {
                let t = village(v) + 1;
                v["towers"][t]["variants"][0]["income"]["per_round"] = json!(-1.0);
            },
            "ungültiges Einkommen",
        );
        rejects(
            &|v| {
                let t = village(v) + 1;
                v["towers"][t]["variants"][0]["sell_bonus"] = json!(0.5);
            },
            "sell_bonus",
        );
    }

    #[test]
    fn cost_rounding() {
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../data/sim_v0.game_v1.json");
        let mut g = GameV1::load(path).unwrap();
        g.rules.cost_multiplier = 1.08; // Hard
        g.rules.cost_rounding = 5;
        assert_eq!(g.cost(200), 215); // 216 -> 215
        assert_eq!(g.cost(260), 280); // 280.8 -> 280
        assert_eq!(g.cost(375), 405);
    }
}
