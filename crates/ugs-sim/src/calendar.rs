//! In-game calendar. Proleptic Gregorian, hour resolution, no time zones —
//! game time is a single global clock. Deliberately no chrono/time
//! dependency: the sim's notion of time must be tiny and fully deterministic.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GameDate {
    pub year: i32,
    /// 1-12
    pub month: u8,
    /// 1-31
    pub day: u8,
    /// 0-23
    pub hour: u8,
}

impl GameDate {
    pub fn new(year: i32, month: u8, day: u8, hour: u8) -> Self {
        debug_assert!((1..=12).contains(&month));
        debug_assert!(day >= 1 && day <= days_in_month(year, month));
        debug_assert!(hour < 24);
        Self {
            year,
            month,
            day,
            hour,
        }
    }

    pub fn plus_hours(self, hours: u64) -> Self {
        let mut d = self;
        let total = d.hour as u64 + hours;
        d.hour = (total % 24) as u8;
        let mut days = total / 24;
        while days > 0 {
            let dim = days_in_month(d.year, d.month);
            let remaining_in_month = (dim - d.day) as u64;
            if days > remaining_in_month {
                days -= remaining_in_month + 1;
                d.day = 1;
                if d.month == 12 {
                    d.month = 1;
                    d.year += 1;
                } else {
                    d.month += 1;
                }
            } else {
                d.day += days as u8;
                days = 0;
            }
        }
        d
    }
}

impl std::fmt::Display for GameDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:00",
            self.year, self.month, self.day, self.hour
        )
    }
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => unreachable!("invalid month {month}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_over_month_and_year() {
        let d = GameDate::new(1950, 12, 31, 23).plus_hours(1);
        assert_eq!(d, GameDate::new(1951, 1, 1, 0));
    }

    #[test]
    fn handles_leap_february_1952() {
        let d = GameDate::new(1952, 2, 28, 0).plus_hours(24);
        assert_eq!(d, GameDate::new(1952, 2, 29, 0));
        let d = d.plus_hours(24);
        assert_eq!(d, GameDate::new(1952, 3, 1, 0));
    }

    #[test]
    fn long_jump_matches_incremental() {
        let start = GameDate::new(1950, 1, 1, 0);
        let mut step = start;
        for _ in 0..10_000 {
            step = step.plus_hours(1);
        }
        assert_eq!(step, start.plus_hours(10_000));
    }
}
