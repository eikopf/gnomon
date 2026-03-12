use jiff::Span;

use crate::types::{Date, DateTime, Frequency, Skip, Weekday};

/// Resolve a negative year-day to positive (1-based).
/// `days_in_year` is 365 or 366.
// r[impl record.rrule.eval.negative.year-day]
pub fn resolve_year_day(day: i16, days_in_year: i16) -> Option<i16> {
    if day > 0 && day <= days_in_year {
        Some(day)
    } else if day < 0 {
        let resolved = days_in_year + 1 + day;
        if resolved >= 1 { Some(resolved) } else { None }
    } else {
        None
    }
}

/// Resolve a negative month-day to positive (1-based).
/// `days_in_month` is 28..=31.
// r[impl record.rrule.eval.negative.month-day]
pub fn resolve_month_day(day: i8, days_in_month: i8) -> Option<i8> {
    if day > 0 && day <= days_in_month {
        Some(day)
    } else if day < 0 {
        let resolved = days_in_month + 1 + day;
        if resolved >= 1 { Some(resolved) } else { None }
    } else {
        None
    }
}

/// Advance from dtstart by `interval * period_index` periods at the given frequency.
/// Uses absolute computation to avoid month-clamping drift.
// r[impl record.rrule.eval.advance.yearly]
// r[impl record.rrule.eval.advance.monthly]
// r[impl record.rrule.eval.advance.weekly]
// r[impl record.rrule.eval.advance.daily]
// r[impl record.rrule.eval.advance.sub-daily]
pub fn advance_period(
    dtstart: DateTime,
    freq: Frequency,
    interval: u32,
    period_index: u64,
) -> Option<DateTime> {
    let n = i64::try_from(period_index).ok()? * i64::from(interval);
    match freq {
        Frequency::Yearly => {
            let year = i16::try_from(i64::from(dtstart.year()) + n).ok()?;
            let month = dtstart.month();
            let day = dtstart.day().min(days_in_month(year, month)?);
            Date::new(year, month, day)
                .ok()?
                .to_datetime(dtstart.time())
                .into()
        }
        Frequency::Monthly => {
            let total_months = i64::from(dtstart.year()) * 12 + i64::from(dtstart.month()) - 1 + n;
            let year = i16::try_from(total_months.div_euclid(12)).ok()?;
            let month = i8::try_from(total_months.rem_euclid(12) + 1).ok()?;
            let day = dtstart.day().min(days_in_month(year, month)?);
            Date::new(year, month, day)
                .ok()?
                .to_datetime(dtstart.time())
                .into()
        }
        Frequency::Weekly => {
            let days = n.checked_mul(7)?;
            dtstart
                .date()
                .checked_add(Span::new().days(days))
                .ok()?
                .to_datetime(dtstart.time())
                .into()
        }
        Frequency::Daily => dtstart
            .date()
            .checked_add(Span::new().days(n))
            .ok()?
            .to_datetime(dtstart.time())
            .into(),
        Frequency::Hourly => {
            let total_seconds = n.checked_mul(3600)?;
            add_seconds(dtstart, total_seconds)
        }
        Frequency::Minutely => {
            let total_seconds = n.checked_mul(60)?;
            add_seconds(dtstart, total_seconds)
        }
        Frequency::Secondly => add_seconds(dtstart, n),
    }
}

fn add_seconds(dt: DateTime, seconds: i64) -> Option<DateTime> {
    dt.checked_add(Span::new().seconds(seconds)).ok()
}

/// Apply skip strategy for an invalid date (e.g., Jan 31 → Feb).
/// Returns None for Skip::Omit when the date is invalid.
// r[impl record.rrule.eval.skip.omit]
// r[impl record.rrule.eval.skip.forward]
// r[impl record.rrule.eval.skip.backward]
pub fn apply_skip(year: i16, month: i8, day: i8, skip: Skip) -> Option<Date> {
    if let Ok(d) = Date::new(year, month, day) {
        return Some(d);
    }
    match skip {
        Skip::Omit => None,
        Skip::Forward => {
            // Move to the 1st of the next month
            if month == 12 {
                Date::new(year + 1, 1, 1).ok()
            } else {
                Date::new(year, month + 1, 1).ok()
            }
        }
        Skip::Backward => {
            // Clamp to last day of month
            let dim = days_in_month(year, month)?;
            Date::new(year, month, dim).ok()
        }
    }
}

/// Number of days in the given month.
pub fn days_in_month(year: i16, month: i8) -> Option<i8> {
    let d = Date::new(year, month, 1).ok()?;
    Some(d.days_in_month())
}

/// Number of days in the given year (365 or 366).
pub fn days_in_year(year: i16) -> i16 {
    if jiff::civil::Date::new(year, 2, 29).is_ok() {
        366
    } else {
        365
    }
}

/// Find the nth occurrence of a weekday in a given month.
/// `nth` is 1-based positive or negative. Positive counts from start, negative from end.
// r[impl record.rrule.eval.negative.weekday]
pub fn nth_weekday_in_month(year: i16, month: i8, nth: i8, weekday: Weekday) -> Option<Date> {
    let jwd = weekday.to_jiff();
    if nth > 0 {
        Date::new(year, month, 1)
            .ok()?
            .nth_weekday_of_month(nth, jwd)
            .ok()
    } else if nth < 0 {
        let dim = days_in_month(year, month)?;
        Date::new(year, month, dim)
            .ok()?
            .nth_weekday_of_month(nth, jwd)
            .ok()
    } else {
        None
    }
}

/// Find all occurrences of a weekday in a month.
pub fn all_weekday_in_month(year: i16, month: i8, weekday: Weekday) -> Vec<Date> {
    let jwd = weekday.to_jiff();
    let first = match Date::new(year, month, 1)
        .ok()
        .and_then(|d| d.nth_weekday_of_month(1, jwd).ok())
    {
        Some(d) => d,
        None => return Vec::new(),
    };
    let mut dates = Vec::new();
    let mut d = first;
    loop {
        if d.month() != month {
            break;
        }
        dates.push(d);
        match d.checked_add(Span::new().days(7)) {
            Ok(next) if next.month() == month => d = next,
            _ => break,
        }
    }
    dates
}

/// Find all occurrences of a weekday in a year.
pub fn all_weekday_in_year(year: i16, weekday: Weekday) -> Vec<Date> {
    let jwd = weekday.to_jiff();
    let jan1 = Date::new(year, 1, 1).unwrap();
    let jan1_wd = jan1.weekday();
    let diff = (jwd.to_monday_zero_offset() as i64) - (jan1_wd.to_monday_zero_offset() as i64);
    let diff = if diff < 0 { diff + 7 } else { diff };
    let mut d = match jan1.checked_add(Span::new().days(diff)) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut dates = Vec::new();
    while d.year() == year {
        dates.push(d);
        match d.checked_add(Span::new().days(7)) {
            Ok(next) if next.year() == year => d = next,
            _ => break,
        }
    }
    dates
}

/// Find the nth occurrence of a weekday in a year.
/// `nth` is 1-based positive or negative.
pub fn nth_weekday_in_year(year: i16, nth: i8, weekday: Weekday) -> Option<Date> {
    let all = all_weekday_in_year(year, weekday);
    if nth > 0 {
        all.get((nth as usize).checked_sub(1)?).copied()
    } else if nth < 0 {
        let idx = all.len().checked_sub((-nth) as usize)?;
        all.get(idx).copied()
    } else {
        None
    }
}

/// Return all dates in a given ISO week number for a year.
/// `week_no` is 1-based (or negative from end). `week_start` determines which day starts the week.
// r[impl record.rrule.eval.by-week-no]
// r[impl record.rrule.eval.iso-week]
pub fn dates_in_iso_week(year: i16, week_no: i8, week_start: Weekday) -> Vec<Date> {
    let total_weeks = iso_weeks_in_year(year, week_start);
    let resolved = if week_no > 0 {
        week_no
    } else {
        let r = total_weeks as i8 + 1 + week_no;
        if r < 1 {
            return Vec::new();
        }
        r
    };
    if resolved < 1 || resolved > total_weeks as i8 {
        return Vec::new();
    }

    // Find the first day of week 1
    let jan1 = Date::new(year, 1, 1).unwrap();
    let jan1_offset = day_offset_from(jan1.weekday(), week_start);

    // Week 1 is the first week containing at least 4 days of the new year
    let week1_start = if jan1_offset <= 3 {
        jan1.checked_add(Span::new().days(-(jan1_offset as i64)))
            .unwrap()
    } else {
        jan1.checked_add(Span::new().days((7 - jan1_offset) as i64))
            .unwrap()
    };

    let target_start = week1_start
        .checked_add(Span::new().days((resolved as i64 - 1) * 7))
        .unwrap();

    let mut dates = Vec::with_capacity(7);
    for i in 0..7i64 {
        if let Ok(d) = target_start.checked_add(Span::new().days(i)) {
            dates.push(d);
        }
    }
    dates
}

/// How many ISO weeks in a year with given week start.
fn iso_weeks_in_year(year: i16, week_start: Weekday) -> u8 {
    let jan1 = Date::new(year, 1, 1).unwrap();
    let dec31 = Date::new(year, 12, 31).unwrap();
    let jan1_off = day_offset_from(jan1.weekday(), week_start);
    let dec31_off = day_offset_from(dec31.weekday(), week_start);

    // A year has 53 weeks if Jan 1 or Dec 31 falls on the 4th day of the week
    // (offset == 3). For leap years, also check offset == 2.
    if jan1_off == 3
        || dec31_off == 3
        || (days_in_year(year) == 366 && (jan1_off == 2 || dec31_off == 2))
    {
        53
    } else {
        52
    }
}

/// Number of days from `week_start` to `day` (0..6).
fn day_offset_from(day: jiff::civil::Weekday, week_start: Weekday) -> i32 {
    let d = day.to_monday_zero_offset() as i32;
    let s = week_start.days_since_monday();
    (d - s).rem_euclid(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_year_day_positive() {
        assert_eq!(resolve_year_day(1, 365), Some(1));
        assert_eq!(resolve_year_day(365, 365), Some(365));
        assert_eq!(resolve_year_day(366, 365), None);
        assert_eq!(resolve_year_day(366, 366), Some(366));
    }

    // r[verify record.rrule.eval.negative.year-day]
    #[test]
    fn resolve_year_day_negative() {
        assert_eq!(resolve_year_day(-1, 365), Some(365));
        assert_eq!(resolve_year_day(-365, 365), Some(1));
        assert_eq!(resolve_year_day(-366, 365), None);
        assert_eq!(resolve_year_day(-366, 366), Some(1));
    }

    // r[verify record.rrule.eval.negative.month-day]
    #[test]
    fn resolve_month_day_tests() {
        assert_eq!(resolve_month_day(1, 31), Some(1));
        assert_eq!(resolve_month_day(31, 31), Some(31));
        assert_eq!(resolve_month_day(-1, 31), Some(31));
        assert_eq!(resolve_month_day(-1, 28), Some(28));
        assert_eq!(resolve_month_day(-28, 28), Some(1));
        assert_eq!(resolve_month_day(-29, 28), None);
    }

    // r[verify record.rrule.eval.advance.yearly]
    #[test]
    fn advance_period_yearly() {
        let dt = DateTime::new(2024, 1, 31, 10, 0, 0, 0).unwrap();
        let next = advance_period(dt, Frequency::Yearly, 1, 1).unwrap();
        assert_eq!(next, DateTime::new(2025, 1, 31, 10, 0, 0, 0).unwrap());
    }

    // r[verify record.rrule.eval.advance.monthly]
    #[test]
    fn advance_period_monthly_clamp() {
        let dt = DateTime::new(2024, 1, 31, 10, 0, 0, 0).unwrap();
        let next = advance_period(dt, Frequency::Monthly, 1, 1).unwrap();
        assert_eq!(next, DateTime::new(2024, 2, 29, 10, 0, 0, 0).unwrap());
    }

    // r[verify record.rrule.eval.advance.daily]
    #[test]
    fn advance_period_daily() {
        let dt = DateTime::new(2024, 12, 30, 0, 0, 0, 0).unwrap();
        let next = advance_period(dt, Frequency::Daily, 1, 3).unwrap();
        assert_eq!(next, DateTime::new(2025, 1, 2, 0, 0, 0, 0).unwrap());
    }

    // r[verify record.rrule.eval.advance.weekly]
    #[test]
    fn advance_period_weekly() {
        let dt = DateTime::new(2024, 1, 1, 0, 0, 0, 0).unwrap();
        let next = advance_period(dt, Frequency::Weekly, 2, 1).unwrap();
        assert_eq!(next, DateTime::new(2024, 1, 15, 0, 0, 0, 0).unwrap());
    }

    // r[verify record.rrule.eval.advance.sub-daily]
    #[test]
    fn advance_period_hourly() {
        let dt = DateTime::new(2024, 1, 1, 23, 0, 0, 0).unwrap();
        let next = advance_period(dt, Frequency::Hourly, 1, 2).unwrap();
        assert_eq!(next, DateTime::new(2024, 1, 2, 1, 0, 0, 0).unwrap());
    }

    #[test]
    fn apply_skip_valid_date() {
        let d = apply_skip(2024, 3, 15, Skip::Omit).unwrap();
        assert_eq!(d, Date::new(2024, 3, 15).unwrap());
    }

    // r[verify record.rrule.eval.skip.omit]
    #[test]
    fn apply_skip_omit_invalid() {
        assert_eq!(apply_skip(2024, 2, 30, Skip::Omit), None);
    }

    // r[verify record.rrule.eval.skip.forward]
    #[test]
    fn apply_skip_forward() {
        let d = apply_skip(2024, 2, 30, Skip::Forward).unwrap();
        assert_eq!(d, Date::new(2024, 3, 1).unwrap());
    }

    // r[verify record.rrule.eval.skip.backward]
    #[test]
    fn apply_skip_backward() {
        let d = apply_skip(2024, 2, 30, Skip::Backward).unwrap();
        assert_eq!(d, Date::new(2024, 2, 29).unwrap());
    }

    #[test]
    fn apply_skip_forward_december() {
        let d = apply_skip(2024, 12, 32, Skip::Forward).unwrap();
        assert_eq!(d, Date::new(2025, 1, 1).unwrap());
    }

    #[test]
    fn all_weekday_in_month_fridays_jan_2024() {
        let fridays = all_weekday_in_month(2024, 1, Weekday::Friday);
        assert_eq!(fridays.len(), 4);
        assert_eq!(fridays[0], Date::new(2024, 1, 5).unwrap());
        assert_eq!(fridays[3], Date::new(2024, 1, 26).unwrap());
    }

    #[test]
    fn all_weekday_in_month_five_fridays() {
        // March 2024 has 5 Fridays
        let fridays = all_weekday_in_month(2024, 3, Weekday::Friday);
        assert_eq!(fridays.len(), 5);
    }

    #[test]
    fn nth_weekday_in_month_positive() {
        let d = nth_weekday_in_month(2024, 1, 2, Weekday::Tuesday).unwrap();
        assert_eq!(d, Date::new(2024, 1, 9).unwrap());
    }

    // r[verify record.rrule.eval.negative.weekday]
    #[test]
    fn nth_weekday_in_month_negative() {
        let d = nth_weekday_in_month(2024, 1, -1, Weekday::Friday).unwrap();
        assert_eq!(d, Date::new(2024, 1, 26).unwrap());
    }

    #[test]
    fn nth_weekday_in_year_first_monday() {
        let d = nth_weekday_in_year(2024, 1, Weekday::Monday).unwrap();
        assert_eq!(d, Date::new(2024, 1, 1).unwrap());
    }

    #[test]
    fn nth_weekday_in_year_last_friday() {
        let d = nth_weekday_in_year(2024, -1, Weekday::Friday).unwrap();
        assert_eq!(d, Date::new(2024, 12, 27).unwrap());
    }

    #[test]
    fn all_weekday_in_year_count() {
        let mondays = all_weekday_in_year(2024, Weekday::Monday);
        assert_eq!(mondays.len(), 53); // 2024 starts on Monday
    }

    // r[verify record.rrule.eval.by-week-no]
    // r[verify record.rrule.eval.iso-week]
    #[test]
    fn dates_in_iso_week_basic() {
        // ISO week 1 of 2024 (Monday start): Jan 1 is Monday
        let dates = dates_in_iso_week(2024, 1, Weekday::Monday);
        assert_eq!(dates.len(), 7);
        assert_eq!(dates[0], Date::new(2024, 1, 1).unwrap());
        assert_eq!(dates[6], Date::new(2024, 1, 7).unwrap());
    }

    #[test]
    fn dates_in_iso_week_negative() {
        let dates = dates_in_iso_week(2024, -1, Weekday::Monday);
        assert!(!dates.is_empty());
    }

    #[test]
    fn days_in_month_leap() {
        assert_eq!(days_in_month(2024, 2), Some(29));
        assert_eq!(days_in_month(2023, 2), Some(28));
        assert_eq!(days_in_month(2024, 1), Some(31));
        assert_eq!(days_in_month(2024, 4), Some(30));
    }

    #[test]
    fn days_in_year_leap() {
        assert_eq!(days_in_year(2024), 366);
        assert_eq!(days_in_year(2023), 365);
    }
}
