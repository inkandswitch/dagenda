//! When things are expected: `est` and `target` take a date at whatever
//! precision the writer knows, from a day to a year.

use core::fmt;
use thiserror::Error;

/// A date as a roadmap writes one: a day (`2026-11-19`), a month
/// (`2026-11`), a quarter (`2026-Q4`), a year (`2026`), or the end of a year
/// (`EOY26`). Each covers a span of days, from [`Date::first`] to
/// [`Date::last`], and is written back the way it was written.
///
/// ```
/// use dagenda_core::date::Date;
///
/// let quarter = Date::new("2026-Q4")?;
/// assert_eq!(quarter.first().to_string(), "2026-10-01");
/// assert_eq!(quarter.last().to_string(), "2026-12-31");
/// assert_eq!(quarter.to_string(), "2026-Q4");
/// assert!(Date::new("2w").is_err(), "a duration doesn't say when");
/// # Ok::<(), dagenda_core::date::InvalidDate>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Date(Form);

impl Date {
    /// Parse a date as written.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDate`] for anything but the forms above, for a
    /// duration (`2w`), or for a month, day, or quarter that doesn't exist.
    pub fn new(written: &str) -> Result<Self, InvalidDate> {
        if let Some(year) = written.strip_prefix("EOY") {
            let short: u16 = digits(year, 2).ok_or(InvalidDate::Form)?;
            return Ok(Self(Form::EndOfYear(2000 + short)));
        }
        if duration(written) {
            return Err(InvalidDate::Duration);
        }

        let mut parts = written.splitn(3, '-');
        let year = parts
            .next()
            .and_then(|year| digits(year, 4))
            .ok_or(InvalidDate::Form)?;
        let form = match (parts.next(), parts.next()) {
            (None, _) => Form::Year(year),
            (Some(quarter), None) if quarter.starts_with('Q') => {
                let quarter = quarter
                    .strip_prefix('Q')
                    .and_then(|q| digits(q, 1))
                    .ok_or(InvalidDate::Form)?;
                if !(1..=4).contains(&quarter) {
                    return Err(InvalidDate::Quarter);
                }
                Form::Quarter(year, quarter)
            }
            (Some(month), day) => {
                let month = digits(month, 2).ok_or(InvalidDate::Form)?;
                if !(1..=12).contains(&month) {
                    return Err(InvalidDate::Month(month));
                }
                match day {
                    None => Form::Month(year, month),
                    Some(day) => {
                        let day = digits(day, 2).ok_or(InvalidDate::Form)?;
                        if day == 0 || day > days_in(year, month) {
                            return Err(InvalidDate::Day { year, month, day });
                        }
                        Form::Day(Day {
                            year,
                            month,
                            date: day,
                        })
                    }
                }
            }
        };
        Ok(Self(form))
    }

    /// The first day it covers.
    #[must_use]
    pub const fn first(self) -> Day {
        match self.0 {
            Form::Day(day) => day,
            Form::Month(year, month) => Day {
                year,
                month,
                date: 1,
            },
            Form::Quarter(year, quarter) => Day {
                year,
                month: quarter * 3 - 2,
                date: 1,
            },
            Form::Year(year) | Form::EndOfYear(year) => Day {
                year,
                month: 1,
                date: 1,
            },
        }
    }

    /// The last day it covers.
    #[must_use]
    pub const fn last(self) -> Day {
        let (year, month) = match self.0 {
            Form::Day(day) => return day,
            Form::Month(year, month) => (year, month),
            Form::Quarter(year, quarter) => (year, quarter * 3),
            Form::Year(year) | Form::EndOfYear(year) => (year, 12),
        };
        Day {
            year,
            month,
            date: days_in(year, month),
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Form::Day(day) => day.fmt(f),
            Form::Month(year, month) => write!(f, "{year:04}-{month:02}"),
            Form::Quarter(year, quarter) => write!(f, "{year:04}-Q{quarter}"),
            Form::Year(year) => write!(f, "{year:04}"),
            Form::EndOfYear(year) => write!(f, "EOY{:02}", year - 2000),
        }
    }
}

/// A day on the calendar. Days order by when they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day {
    year: u16,
    month: u8,
    date: u8,
}

impl Day {
    /// Its year.
    #[must_use]
    pub const fn year(self) -> u16 {
        self.year
    }

    /// Its month, from 1.
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// Its day of the month, from 1.
    #[must_use]
    pub const fn day(self) -> u8 {
        self.date
    }
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.date)
    }
}

/// Which form a date was written in, and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Form {
    Day(Day),
    Month(u16, u8),
    Quarter(u16, u8),
    Year(u16),
    EndOfYear(u16),
}

/// Why a string isn't a valid [`Date`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidDate {
    /// None of the forms.
    #[error("expected a date like `2026-11-19`, `2026-11`, `2026-Q4`, `2026`, or `EOY26`")]
    Form,

    /// A duration, which doesn't say when.
    #[error("expected a date like `2026-11-19` or `2026-Q4`; a duration doesn't say when it lands")]
    Duration,

    /// A month outside 1 to 12.
    #[error("there's no month {0:02}; months run from 01 to 12")]
    Month(u8),

    /// A day the month doesn't have.
    #[error("{year:04}-{month:02} has no day {day:02}")]
    Day {
        /// The year written.
        year: u16,

        /// The month written.
        month: u8,

        /// The day written.
        day: u8,
    },

    /// A quarter outside Q1 to Q4.
    #[error("quarters run from Q1 to Q4")]
    Quarter,
}

/// Exactly `count` ASCII digits, as a number.
fn digits<T: From<u8> + core::ops::Mul<Output = T> + core::ops::Add<Output = T>>(
    text: &str,
    count: usize,
) -> Option<T> {
    if text.len() != count || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(
        text.bytes()
            .fold(T::from(0), |n, b| n * T::from(10) + T::from(b - b'0')),
    )
}

/// Whether `text` looks like a duration: a number, then a unit.
fn duration(text: &str) -> bool {
    let unit = text.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
    unit.len() < text.len()
        && matches!(
            unit,
            "h" | "d" | "w" | "m" | "y" | "mo" | "day" | "days" | "week" | "weeks"
        )
}

/// How many days `month` has in `year`.
const fn days_in(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
