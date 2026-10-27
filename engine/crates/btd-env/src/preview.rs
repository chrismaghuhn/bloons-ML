//! Runden-Vorschau für die Observation: was in den nächsten Runden kommt und wie viele Runden
//! es noch bis zum nächsten Spezial-Bloon sind (Camo, Lead, Ceramic, MOAB, ...).
//!
//! Merkmale je Runde werden einmal aus den Daten vorberechnet. Ein Bloon zählt für eine
//! Kategorie, wenn er selbst oder einer seiner Nachkommen sie hat (Zebra -> Black + White,
//! MOAB -> Ceramic); Camo/Regrow/Fortified-Flags der Gruppe gelten für alle Nachkommen.
//! Fortified zählt außerdem über den Bloon-Tag "Fortified" (so kommt es in den Spieldaten vor).

use btd_data::v1::{Classes, Flags, GameV1, Props};

/// Kategorien in der Reihenfolge der Observation.
pub const CATEGORIES: [&str; 9] =
    ["camo", "regrow", "fortified", "lead", "black", "white", "purple", "ceramic", "moab"];
const NC: usize = CATEGORIES.len();
const CAMO: u16 = 1;
const REGROW: u16 = 2;
const FORTIFIED: u16 = 4;
const LEAD: u16 = 8;
const BLACK: u16 = 16;
const WHITE: u16 = 32;
const PURPLE: u16 = 64;
const CERAMIC: u16 = 128;
const MOAB: u16 = 256;

/// Horizont für "Runden bis zur nächsten Kategorie" (weiter weg = 1.0).
const HORIZON: usize = 10;

#[derive(Clone, Debug, Default)]
struct RoundFeatures {
    /// Bloons je Kategorie (oberste Ebene, Nachkommen nur als Kategorie-Merkmal).
    counts: [f32; NC],
    /// Red Bloon Equivalent der Runde (Summe der Leak-Werte).
    rbe: f32,
    /// Spawn-Dauer in Sekunden.
    seconds: f32,
}

#[derive(Clone, Debug)]
pub struct RoundPreview {
    rounds: Vec<RoundFeatures>,
    lookahead: usize,
}

impl RoundPreview {
    pub fn new(data: &GameV1, lookahead: usize) -> Self {
        let n = data.bloons.len();
        // Kategorie-Maske je Bloon-Typ inkl. Nachkommen (Kinder haben laut Validierung keine Zyklen)
        let mut masks: Vec<Option<u16>> = vec![None; n];
        fn mask_of(data: &GameV1, k: usize, memo: &mut Vec<Option<u16>>) -> u16 {
            if let Some(m) = memo[k] {
                return m;
            }
            let b = &data.bloons[k];
            let p = b.props.0;
            let mut m = 0;
            for (bit, flag) in
                [(Props::LEAD, LEAD), (Props::BLACK, BLACK), (Props::WHITE, WHITE), (Props::PURPLE, PURPLE)]
            {
                if p & bit != 0 {
                    m |= flag;
                }
            }
            if b.classes.0 & Classes::CERAMIC != 0 {
                m |= CERAMIC;
            }
            if b.classes.0 & Classes::MOAB != 0 {
                m |= MOAB;
            }
            if b.camo {
                m |= CAMO;
            }
            if b.regrow {
                m |= REGROW;
            }
            // Spieldaten: Fortified ist eine eigene Bloon-Variante mit Tag (z.B. "BfbFortified")
            if b.tags.iter().any(|t| t == "Fortified") {
                m |= FORTIFIED;
            }
            for &(c, _) in &b.children {
                m |= mask_of(data, c as usize, memo);
            }
            memo[k] = Some(m);
            m
        }
        let rounds = data
            .rounds
            .iter()
            .map(|groups| {
                let mut f = RoundFeatures::default();
                for g in groups {
                    let mut m = mask_of(data, g.kind as usize, &mut masks);
                    for (bit, flag) in
                        [(Flags::CAMO, CAMO), (Flags::REGROW, REGROW), (Flags::FORTIFIED, FORTIFIED)]
                    {
                        if g.flags.has(bit) {
                            m |= flag;
                        }
                    }
                    for (i, c) in f.counts.iter_mut().enumerate() {
                        if m & (1 << i) != 0 {
                            *c += g.count as f32;
                        }
                    }
                    f.rbe += (data.bloons[g.kind as usize].leak as f64 * g.count as f64) as f32;
                    f.seconds = f.seconds.max(g.end as f32);
                }
                f
            })
            .collect();
        RoundPreview { rounds, lookahead }
    }

    /// Anzahl Werte: je Vorschau-Runde Kategorien + RBE + Dauer, dazu "Runden bis" je Kategorie.
    pub fn len(lookahead: usize) -> usize {
        lookahead * (NC + 2) + NC
    }

    /// Schreibt die Vorschau ab Runden-Index `next` (= nächste zu spielende Runde) nach `out`,
    /// alle Werte in [0, 1]. Runden hinter dem Ende zählen als leer.
    pub fn write(&self, next: usize, out: &mut [f32]) {
        let norm = |x: f32, max: f32| (libm::log1pf(x) / libm::log1pf(max)).min(1.0);
        let mut o = 0;
        for d in 0..self.lookahead {
            if let Some(f) = self.rounds.get(next + d) {
                for (i, &c) in f.counts.iter().enumerate() {
                    out[o + i] = norm(c, 200.0);
                }
                out[o + NC] = norm(f.rbe, 20_000.0);
                out[o + NC + 1] = (f.seconds / 60.0).min(1.0);
            } else {
                out[o..o + NC + 2].fill(0.0);
            }
            o += NC + 2;
        }
        for i in 0..NC {
            let until = (0..HORIZON).find(|&d| self.rounds.get(next + d).is_some_and(|f| f.counts[i] > 0.0));
            out[o + i] = until.map_or(1.0, |d| d as f32 / HORIZON as f32);
        }
    }
}
