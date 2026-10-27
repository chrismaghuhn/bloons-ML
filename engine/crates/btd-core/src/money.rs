//! Cash als Fixed-Point in 1/1000 $ (docs/engine-design.md §3).

use std::ops::{Add, AddAssign, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money(pub i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub const fn from_dollars(d: i64) -> Self {
        Money(d * 1000)
    }

    /// Ganze Dollar, abgerundet (wie die Anzeige im Spiel).
    pub const fn dollars(self) -> i64 {
        self.0.div_euclid(1000)
    }

    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 1000.0
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, o: Money) -> Money {
        Money(self.0 + o.0)
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, o: Money) -> Money {
        Money(self.0 - o.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, o: Money) {
        self.0 += o.0;
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, o: Money) {
        self.0 -= o.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollars_round_down() {
        assert_eq!(Money(1999).dollars(), 1);
        assert_eq!(Money(-1).dollars(), -1);
        assert_eq!(Money::from_dollars(650) - Money::from_dollars(200), Money::from_dollars(450));
    }
}
