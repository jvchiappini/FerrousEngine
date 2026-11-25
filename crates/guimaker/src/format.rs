//! Small, deterministic format helpers shared by apps and recipes.
//!
//! The library never reads the wall clock, so every helper here takes the
//! instant the host already owns. That keeps a story, a golden and a real
//! window identical at any hour.

use std::sync::Arc;

/// Minutes in an hour.
const MINUTES_PER_HOUR: u64 = 60;
/// Hours in a day.
const HOURS_PER_DAY: u64 = 24;
/// Milliseconds in a minute.
const MILLIS_PER_MINUTE: f64 = 60_000.0;

/// Formats an injected millisecond instant as a 24-hour `HH:MM` label.
///
/// A finite, non-negative instant is used as-is; anything else (including
/// `NaN` and infinity) reads as midnight. Only the time of day is kept, so a
/// value that spans several days still formats like a clock.
///
/// ```
/// use guimaker::format::clock_hh_mm;
/// assert_eq!(&*clock_hh_mm(0.0), "00:00");
/// assert_eq!(&*clock_hh_mm(11.0 * 3_600_000.0 + 42.0 * 60_000.0), "11:42");
/// assert_eq!(&*clock_hh_mm(f64::NAN), "00:00");
/// ```
#[must_use]
pub fn clock_hh_mm(now_ms: f64) -> Arc<str> {
    let millis = if now_ms.is_finite() {
        now_ms.max(0.0)
    } else {
        0.0
    };
    // A finite, non-negative instant fits in `u64` after saturating casts; the
    // modulo keeps only the time of day.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // reason: `millis` is finite and non-negative; saturation is acceptable
    let minutes = (millis / MILLIS_PER_MINUTE) as u64 % (HOURS_PER_DAY * MINUTES_PER_HOUR);
    format!(
        "{:02}:{:02}",
        minutes / MINUTES_PER_HOUR,
        minutes % MINUTES_PER_HOUR
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::clock_hh_mm;

    #[test]
    fn formats_a_finite_instant_as_a_24_hour_clock() {
        assert_eq!(&*clock_hh_mm(0.0), "00:00");
        assert_eq!(&*clock_hh_mm(11.0 * 3_600_000.0 + 42.0 * 60_000.0), "11:42");
        assert_eq!(&*clock_hh_mm(23.0 * 3_600_000.0 + 59.0 * 60_000.0), "23:59");
    }

    #[test]
    fn a_non_finite_or_negative_instant_reads_as_midnight() {
        assert_eq!(&*clock_hh_mm(f64::NAN), "00:00");
        assert_eq!(&*clock_hh_mm(f64::INFINITY), "00:00");
        assert_eq!(&*clock_hh_mm(-1.0), "00:00");
    }

    #[test]
    fn keeps_only_the_time_of_day() {
        assert_eq!(&*clock_hh_mm(25.0 * 3_600_000.0 + 5.0 * 60_000.0), "01:05");
    }
}
