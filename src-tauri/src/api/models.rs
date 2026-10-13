//! The two money rules that belong to no single model record.
//!
//! The price table itself lives on the model records in `provider::seed`, because
//! a rate belongs to an endpoint-and-model pair rather than to a model id: the
//! same id can cost different money in two places, and one table keyed by id
//! alone can only ever be right about one of them. What is left here is what no
//! record owns — the largest window the app will assume, when the clock makes an
//! endpoint dearer, and when a run has spent enough to stop.

use chrono::{DateTime, Datelike, Timelike, Utc};

/// The largest context window the app knows about, for a model that declares
/// none.
///
/// Guessing *small* is the dangerous direction: it would fold a chat that had room
/// left and spend a summariser call nobody needed.
pub const LARGEST_CONTEXT: u32 = 1_000_000;

/// Peak hours are 01:00-04:00 and 06:00-10:00 UTC, Monday-Friday, excluding
/// Chinese holidays, which we cannot know locally — treat them as peak, i.e.
/// over-estimate.
pub fn is_peak(now: DateTime<Utc>) -> bool {
    if matches!(now.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
        return false;
    }
    let h = now.hour();
    (1..4).contains(&h) || (6..10).contains(&h)
}

/// Whether a run has crossed the user's spend limit.
///
/// A limit of 0 means "no limit" rather than "stop at once", which is the
/// default and the only value nobody has to think about. Reaching the limit
/// exactly counts as crossing it: a limit you can sit on is not a limit.
pub fn over_budget(spent: f64, limit: f64) -> bool {
    limit > 0.0 && spent >= limit
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn peak_hours_are_weekday_windows_only() {
        // Tuesday 02:00 UTC -> peak
        let t = Utc.with_ymd_and_hms(2026, 10, 13, 2, 0, 0).unwrap();
        assert!(is_peak(t));
        // Tuesday 12:00 UTC -> off-peak
        let t = Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap();
        assert!(!is_peak(t));
        // Saturday 02:00 UTC -> off-peak
        let t = Utc.with_ymd_and_hms(2026, 10, 17, 2, 0, 0).unwrap();
        assert!(!is_peak(t));
    }

    #[test]
    fn a_zero_limit_is_no_limit() {
        assert!(!over_budget(5.0, 0.0));
        assert!(!over_budget(0.0, 1.0));
        assert!(over_budget(1.0, 1.0), "the limit itself counts as reached");
        assert!(over_budget(2.5, 1.0));
    }
}
