//! When a scheduled task next runs.
//!
//! Arithmetic over local time and nothing else. The whole need is "every day at
//! 09:00", "every Monday at 17:00", "the first Wednesday of the month at 08:00"
//! — and, since 8.5, "every hour" and "every weekday" — so a cron crate would be
//! a dependency and a grammar for five shapes. `next_run_at` is pure, so the
//! calendar rules are pinned by tests rather than by waiting a week to see
//! whether it fires.

use chrono::{
    DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, NaiveTime, TimeZone, Timelike,
    Weekday,
};

/// How often a task repeats. `weekday` and `nth` are read only by the shape that
/// uses them, so a daily task carries values that are simply ignored — and so
/// does an hourly one, which reads only the *minute* of `time`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Hourly,
    Daily,
    /// Every Monday to Friday; the weekend is skipped.
    Weekdays,
    Weekly,
    Monthly,
}

impl Freq {
    /// An unknown value is daily: the most frequent repeat is the one a typo is
    /// most likely to notice, rather than a task that silently never runs.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "hourly" => Self::Hourly,
            "weekdays" => Self::Weekdays,
            "weekly" => Self::Weekly,
            "monthly" => Self::Monthly,
            _ => Self::Daily,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekdays => "weekdays",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub freq: Freq,
    /// Local wall-clock time of day.
    pub time: NaiveTime,
    /// 0 = Monday, matching [`Datelike::num_days_from_monday`]. Used by weekly
    /// and monthly.
    pub weekday: u32,
    /// Which `weekday` of the month: 1 = first … 4 = fourth. Used by monthly.
    pub nth: u32,
}

impl Schedule {
    /// From the stored columns. A value that will not parse falls back to a sane
    /// one rather than being refused: a task that runs at 09:00 is better than a
    /// task that never runs because a row was hand-edited.
    pub fn parse(freq: &str, time: &str, weekday: i64, nth: i64) -> Self {
        Self {
            freq: Freq::parse(freq),
            time: NaiveTime::parse_from_str(time.trim(), "%H:%M")
                .unwrap_or_else(|_| NaiveTime::from_hms_opt(9, 0, 0).expect("09:00 is a time")),
            weekday: weekday.rem_euclid(7) as u32,
            nth: nth.clamp(1, 4) as u32,
        }
    }
}

/// The timestamp of a wall-clock date and time, in local time.
///
/// A DST transition makes one local hour a year either missing or doubled.
/// `Local` reports both, and this takes the first real instant after a forward
/// jump rather than failing — the difference is an hour, and a task that fires
/// an hour late once a year is not worth a migration.
fn local_at(date: NaiveDate, time: NaiveTime) -> i64 {
    let naive = date.and_time(time);
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt.timestamp(),
        LocalResult::Ambiguous(dt, _) => dt.timestamp(),
        LocalResult::None => (naive + Duration::hours(1)).and_utc().timestamp(),
    }
}

/// The nth `weekday` of a month. `nth` is 1-based and clamped to 4, so the day
/// is never later than the 28th and the date is always real.
fn nth_weekday(year: i32, month: u32, weekday: u32, nth: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("a real month");
    let offset = (weekday + 7 - first.weekday().num_days_from_monday()) % 7;
    let day = 1 + offset + (nth.clamp(1, 4) - 1) * 7;
    NaiveDate::from_ymd_opt(year, month, day).expect("day 1 + at most 6 + 21 is real")
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

/// The next timestamp strictly after `now` at which the schedule fires.
///
/// Strictly after, so a task whose minute has just arrived is not also due the
/// moment it is advanced — advancing before firing is what keeps a long run from
/// being started twice.
pub fn next_run_at(now: DateTime<Local>, s: &Schedule) -> i64 {
    let today = now.date_naive();
    let after = now.timestamp();

    match s.freq {
        Freq::Hourly => {
            // "Every hour at :MM" — the minute is read, the hour is not. Worked
            // out on the timestamp so 23:30 rolls straight to 00:30 rather than
            // wrapping a date; strictly after, so a task whose minute is now does
            // not fire again — the same rule daily follows.
            let mut candidate = now.with_minute(s.time.minute()).unwrap_or(now);
            candidate = candidate.with_second(0).unwrap_or(candidate);
            candidate = candidate.with_nanosecond(0).unwrap_or(candidate);
            if candidate.timestamp() > after {
                candidate.timestamp()
            } else {
                (candidate + Duration::hours(1)).timestamp()
            }
        }
        Freq::Daily => {
            let today_at = local_at(today, s.time);
            if today_at > after {
                today_at
            } else {
                local_at(today + Duration::days(1), s.time)
            }
        }
        Freq::Weekdays => {
            // Monday to Friday at the same time; Saturday and Sunday are skipped.
            // Eight days always hold a weekday, so one of them is ahead of `now`.
            let mut date = today;
            for _ in 0..8 {
                let weekend = matches!(date.weekday(), Weekday::Sat | Weekday::Sun);
                if !weekend {
                    let ts = local_at(date, s.time);
                    if ts > after {
                        return ts;
                    }
                }
                date = date + Duration::days(1);
            }
            // Unreachable: a weekday always turns up well inside eight days.
            local_at(today + Duration::days(7), s.time)
        }
        Freq::Weekly => {
            // Eight days covers every weekday, so one of them matches.
            let mut date = today;
            for _ in 0..8 {
                if date.weekday().num_days_from_monday() == s.weekday {
                    let ts = local_at(date, s.time);
                    if ts > after {
                        return ts;
                    }
                }
                date = date + Duration::days(1);
            }
            local_at(today + Duration::days(7), s.time)
        }
        Freq::Monthly => {
            let (mut year, mut month) = (today.year(), today.month());
            for _ in 0..3 {
                let ts = local_at(nth_weekday(year, month, s.weekday, s.nth), s.time);
                if ts > after {
                    return ts;
                }
                (year, month) = next_month(year, month);
            }
            // Unreachable: three consecutive months always hold an occurrence.
            local_at(nth_weekday(year, month, s.weekday, s.nth), s.time)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-12 is a Monday, which is the anchor every case below counts from.
    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("a real local time")
    }

    fn stamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        at(year, month, day, hour, minute).timestamp()
    }

    fn sched(freq: Freq, time: &str, weekday: u32, nth: u32) -> Schedule {
        Schedule::parse(freq.id(), time, weekday as i64, nth as i64)
    }

    #[test]
    fn daily_uses_today_while_the_hour_is_still_ahead() {
        let s = sched(Freq::Daily, "09:00", 0, 1);
        assert_eq!(next_run_at(at(2026, 10, 12, 8, 0), &s), stamp(2026, 10, 12, 9, 0));
    }

    #[test]
    fn daily_rolls_to_tomorrow_once_the_hour_has_passed() {
        let s = sched(Freq::Daily, "09:00", 0, 1);
        assert_eq!(next_run_at(at(2026, 10, 12, 10, 0), &s), stamp(2026, 10, 13, 9, 0));
    }

    #[test]
    fn a_time_exactly_now_is_already_gone() {
        // Strictly after: otherwise advancing and firing would re-fire forever.
        let s = sched(Freq::Daily, "09:00", 0, 1);
        assert_eq!(next_run_at(at(2026, 10, 12, 9, 0), &s), stamp(2026, 10, 13, 9, 0));
    }

    #[test]
    fn weekly_can_still_be_today() {
        let s = sched(Freq::Weekly, "17:00", 0, 1); // Monday
        assert_eq!(next_run_at(at(2026, 10, 12, 10, 0), &s), stamp(2026, 10, 12, 17, 0));
    }

    #[test]
    fn weekly_moves_to_the_next_matching_day() {
        let s = sched(Freq::Weekly, "08:00", 2, 1); // Wednesday
        assert_eq!(next_run_at(at(2026, 10, 12, 10, 0), &s), stamp(2026, 10, 14, 8, 0));
    }

    #[test]
    fn weekly_that_has_passed_waits_a_week() {
        let s = sched(Freq::Weekly, "09:00", 0, 1); // Monday, already 10:00
        assert_eq!(next_run_at(at(2026, 10, 12, 10, 0), &s), stamp(2026, 10, 19, 9, 0));
    }

    #[test]
    fn monthly_first_wednesday_can_be_this_month() {
        let s = sched(Freq::Monthly, "08:00", 2, 1);
        // 2026-10-07 is the first Wednesday of October 2026.
        assert_eq!(next_run_at(at(2026, 10, 1, 6, 0), &s), stamp(2026, 10, 7, 8, 0));
    }

    #[test]
    fn monthly_first_wednesday_rolls_into_the_next_month() {
        let s = sched(Freq::Monthly, "08:00", 2, 1);
        // 2026-10-12 is past October's first Wednesday, so it is November's.
        assert_eq!(next_run_at(at(2026, 10, 12, 10, 0), &s), stamp(2026, 11, 4, 8, 0));
    }

    #[test]
    fn a_third_friday_lands_on_the_third_friday() {
        let s = sched(Freq::Monthly, "12:00", 4, 3);
        let ts = next_run_at(at(2026, 10, 1, 0, 0), &s);
        let dt = Local.timestamp_opt(ts, 0).unwrap();
        assert_eq!(dt.weekday().num_days_from_monday(), 4);
        assert!((15..=21).contains(&dt.day()), "third Friday: {}", dt.day());
    }

    #[test]
    fn every_frequency_lands_strictly_in_the_future() {
        let now = at(2026, 10, 12, 10, 0);
        for freq in [Freq::Hourly, Freq::Daily, Freq::Weekdays, Freq::Weekly, Freq::Monthly] {
            let s = sched(freq, "10:00", 2, 1);
            let ts = next_run_at(now, &s);
            assert!(ts > now.timestamp(), "{freq:?} did not move forward");
        }
    }

    #[test]
    fn hourly_reads_the_minute_and_crosses_midnight() {
        let s = sched(Freq::Hourly, "09:30", 0, 1);
        // Later the same hour, then the next hour, then over a day boundary.
        assert_eq!(next_run_at(at(2026, 10, 12, 9, 10), &s), stamp(2026, 10, 12, 9, 30));
        assert_eq!(next_run_at(at(2026, 10, 12, 9, 40), &s), stamp(2026, 10, 12, 10, 30));
        assert_eq!(next_run_at(at(2026, 10, 12, 23, 30), &s), stamp(2026, 10, 13, 0, 30));
        // Exactly on the minute is already gone: strictly after, like every other
        // shape, or advancing and firing would re-fire forever.
        assert_eq!(next_run_at(at(2026, 10, 12, 9, 30), &s), stamp(2026, 10, 12, 10, 30));
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        let s = sched(Freq::Weekdays, "09:00", 0, 1);
        // Monday, later that day.
        assert_eq!(next_run_at(at(2026, 10, 12, 8, 0), &s), stamp(2026, 10, 12, 9, 0));
        // Friday after the time has gone: the next weekday is Monday, not Saturday.
        assert_eq!(next_run_at(at(2026, 10, 16, 18, 0), &s), stamp(2026, 10, 19, 9, 0));
        // Saturday and Sunday both wait for Monday.
        assert_eq!(next_run_at(at(2026, 10, 17, 8, 0), &s), stamp(2026, 10, 19, 9, 0));
        assert_eq!(next_run_at(at(2026, 10, 18, 8, 0), &s), stamp(2026, 10, 19, 9, 0));
    }

    #[test]
    fn an_unparseable_row_still_schedules_something() {
        let s = Schedule::parse("fortnightly", "nonsense", -3, 99);
        assert_eq!(s.freq, Freq::Daily);
        assert_eq!(s.time, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(s.weekday, 4);
        assert_eq!(s.nth, 4);
    }

    #[test]
    fn the_two_new_shapes_round_trip_through_their_ids() {
        for (id, freq) in [("hourly", Freq::Hourly), ("weekdays", Freq::Weekdays)] {
            assert_eq!(Freq::parse(id), freq);
            assert_eq!(freq.id(), id);
        }
        // `weekdays` and `weekly` are different shapes, not a prefix of each other.
        assert_ne!(Freq::parse("weekdays"), Freq::parse("weekly"));
    }
}
