use jiff::Span;

use crate::table::{self, Action, ByDayContext, ByRule};
use crate::types::*;
use crate::util;

/// Expand a single period into candidate datetimes.
///
/// This is the core expansion pipeline: seed from dtstart, then apply BY* rules
/// in canonical order, filter invalids, sort, dedup, and apply BYSETPOS.
pub fn expand_period(rule: &RecurrenceRule, dtstart: DateTime, period_index: u64) -> Vec<DateTime> {
    let seed = match util::advance_period(dtstart, rule.frequency, rule.interval, period_index) {
        Some(dt) => dt,
        None => return Vec::new(),
    };

    let ctx = ByDayContext {
        has_by_month_day: !rule.by_month_day.is_empty(),
        has_by_year_day: !rule.by_year_day.is_empty(),
        has_by_week_no: !rule.by_week_no.is_empty(),
        has_by_month: !rule.by_month.is_empty(),
    };

    let mut candidates = vec![seed];

    // Apply BY* rules in canonical order
    candidates = apply_by_month(&candidates, &rule.by_month, rule.frequency, rule.skip, &ctx);
    candidates = apply_by_week_no(
        &candidates,
        &rule.by_week_no,
        rule.frequency,
        rule.week_start,
        &ctx,
    );
    candidates = apply_by_year_day(&candidates, &rule.by_year_day, rule.frequency, &ctx);
    candidates = apply_by_month_day(
        &candidates,
        &rule.by_month_day,
        rule.frequency,
        rule.skip,
        &ctx,
    );
    candidates = apply_by_day(&candidates, &rule.by_day, rule.frequency, &ctx);
    candidates = apply_by_hour(&candidates, &rule.by_hour, rule.frequency, &ctx);
    candidates = apply_by_minute(&candidates, &rule.by_minute, rule.frequency, &ctx);
    candidates = apply_by_second(&candidates, &rule.by_second, rule.frequency, &ctx);

    // Sort and dedup
    candidates.sort();
    candidates.dedup();

    // Apply BYSETPOS last
    candidates = apply_by_set_pos(&candidates, &rule.by_set_position);

    candidates
}

fn apply_by_month(
    candidates: &[DateTime],
    by_month: &[ByMonth],
    freq: Frequency,
    skip: Skip,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_month.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::ByMonth, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                for bm in by_month {
                    let dim = match util::days_in_month(dt.year(), bm.month as i8) {
                        Some(d) => d,
                        None => continue,
                    };
                    let day = dt.day().min(dim);
                    if let Some(d) = util::apply_skip(dt.year(), bm.month as i8, day, skip) {
                        result.push(d.to_datetime(dt.time()));
                    }
                }
            }
            result
        }
        Action::Limit => candidates
            .iter()
            .filter(|dt| by_month.iter().any(|bm| dt.month() == bm.month as i8))
            .copied()
            .collect(),
        Action::NA => candidates.to_vec(),
    }
}

// r[impl record.rrule.eval.by-week-no]
fn apply_by_week_no(
    candidates: &[DateTime],
    by_week_no: &[i8],
    freq: Frequency,
    week_start: Weekday,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_week_no.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::ByWeekNo, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                for &wn in by_week_no {
                    let dates = util::dates_in_iso_week(dt.year(), wn, week_start);
                    for d in dates {
                        result.push(d.to_datetime(dt.time()));
                    }
                }
            }
            result
        }
        Action::Limit | Action::NA => candidates.to_vec(),
    }
}

fn apply_by_year_day(
    candidates: &[DateTime],
    by_year_day: &[i16],
    freq: Frequency,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_year_day.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::ByYearDay, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                let diy = util::days_in_year(dt.year());
                for &yd in by_year_day {
                    if let Some(resolved) = util::resolve_year_day(yd, diy)
                        && let Ok(jan1) = jiff::civil::Date::new(dt.year(), 1, 1)
                        && let Ok(d) = jan1.checked_add(Span::new().days(i64::from(resolved) - 1))
                    {
                        result.push(d.to_datetime(dt.time()));
                    }
                }
            }
            result
        }
        Action::Limit => candidates
            .iter()
            .filter(|dt| {
                let diy = util::days_in_year(dt.year());
                let day_of_year = dt.date().day_of_year();
                by_year_day.iter().any(|&yd| {
                    util::resolve_year_day(yd, diy).is_some_and(|resolved| resolved == day_of_year)
                })
            })
            .copied()
            .collect(),
        Action::NA => candidates.to_vec(),
    }
}

fn apply_by_month_day(
    candidates: &[DateTime],
    by_month_day: &[i8],
    freq: Frequency,
    skip: Skip,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_month_day.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::ByMonthDay, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                let dim = match util::days_in_month(dt.year(), dt.month()) {
                    Some(d) => d,
                    None => continue,
                };
                for &md in by_month_day {
                    if let Some(resolved) = util::resolve_month_day(md, dim)
                        && let Some(d) = util::apply_skip(dt.year(), dt.month(), resolved, skip)
                    {
                        result.push(d.to_datetime(dt.time()));
                    }
                }
            }
            result
        }
        Action::Limit => candidates
            .iter()
            .filter(|dt| {
                let dim = util::days_in_month(dt.year(), dt.month()).unwrap_or(28);
                by_month_day
                    .iter()
                    .any(|&md| util::resolve_month_day(md, dim).is_some_and(|r| r == dt.day()))
            })
            .copied()
            .collect(),
        Action::NA => candidates.to_vec(),
    }
}

// r[impl record.rrule.eval.by-day.monthly-expand]
// r[impl record.rrule.eval.by-day.yearly-expand]
fn apply_by_day(
    candidates: &[DateTime],
    by_day: &[NDay],
    freq: Frequency,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_day.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::ByDay, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                for nday in by_day {
                    match nday.nth {
                        Some(nth) => match freq {
                            Frequency::Monthly => {
                                if let Some(d) =
                                    util::nth_weekday_in_month(dt.year(), dt.month(), nth, nday.day)
                                {
                                    result.push(d.to_datetime(dt.time()));
                                }
                            }
                            Frequency::Yearly => {
                                if ctx.has_by_month {
                                    if let Some(d) = util::nth_weekday_in_month(
                                        dt.year(),
                                        dt.month(),
                                        nth,
                                        nday.day,
                                    ) {
                                        result.push(d.to_datetime(dt.time()));
                                    }
                                } else if ctx.has_by_week_no {
                                    if Weekday::from_jiff(dt.date().weekday()) == nday.day {
                                        result.push(dt);
                                    }
                                } else if let Some(d) =
                                    util::nth_weekday_in_year(dt.year(), nth, nday.day)
                                {
                                    result.push(d.to_datetime(dt.time()));
                                }
                            }
                            _ => {
                                if Weekday::from_jiff(dt.date().weekday()) == nday.day {
                                    result.push(dt);
                                }
                            }
                        },
                        None => match freq {
                            Frequency::Monthly => {
                                for d in util::all_weekday_in_month(dt.year(), dt.month(), nday.day)
                                {
                                    result.push(d.to_datetime(dt.time()));
                                }
                            }
                            Frequency::Yearly => {
                                if ctx.has_by_month {
                                    for d in
                                        util::all_weekday_in_month(dt.year(), dt.month(), nday.day)
                                    {
                                        result.push(d.to_datetime(dt.time()));
                                    }
                                } else if ctx.has_by_week_no {
                                    if Weekday::from_jiff(dt.date().weekday()) == nday.day {
                                        result.push(dt);
                                    }
                                } else {
                                    for d in util::all_weekday_in_year(dt.year(), nday.day) {
                                        result.push(d.to_datetime(dt.time()));
                                    }
                                }
                            }
                            Frequency::Weekly => {
                                let current_wd = dt.date().weekday();
                                let target_wd = nday.day.to_jiff();
                                let diff = i64::from(target_wd.to_monday_zero_offset())
                                    - i64::from(current_wd.to_monday_zero_offset());
                                if let Ok(d) = dt.date().checked_add(Span::new().days(diff)) {
                                    result.push(d.to_datetime(dt.time()));
                                }
                            }
                            _ => {
                                if Weekday::from_jiff(dt.date().weekday()) == nday.day {
                                    result.push(dt);
                                }
                            }
                        },
                    }
                }
            }
            result
        }
        Action::Limit => candidates
            .iter()
            .filter(|dt| {
                let wd = Weekday::from_jiff(dt.date().weekday());
                by_day.iter().any(|nday| nday.day == wd)
            })
            .copied()
            .collect(),
        Action::NA => candidates.to_vec(),
    }
}

fn apply_by_hour(
    candidates: &[DateTime],
    by_hour: &[u8],
    freq: Frequency,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_hour.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::ByHour, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                for &h in by_hour {
                    if let Ok(t) = jiff::civil::Time::new(h as i8, dt.minute(), dt.second(), 0) {
                        result.push(dt.date().to_datetime(t));
                    }
                }
            }
            result
        }
        Action::Limit => candidates
            .iter()
            .filter(|dt| by_hour.contains(&(dt.hour() as u8)))
            .copied()
            .collect(),
        Action::NA => candidates.to_vec(),
    }
}

fn apply_by_minute(
    candidates: &[DateTime],
    by_minute: &[u8],
    freq: Frequency,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_minute.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::ByMinute, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                for &m in by_minute {
                    if let Ok(t) = jiff::civil::Time::new(dt.hour(), m as i8, dt.second(), 0) {
                        result.push(dt.date().to_datetime(t));
                    }
                }
            }
            result
        }
        Action::Limit => candidates
            .iter()
            .filter(|dt| by_minute.contains(&(dt.minute() as u8)))
            .copied()
            .collect(),
        Action::NA => candidates.to_vec(),
    }
}

fn apply_by_second(
    candidates: &[DateTime],
    by_second: &[u8],
    freq: Frequency,
    ctx: &ByDayContext,
) -> Vec<DateTime> {
    if by_second.is_empty() {
        return candidates.to_vec();
    }
    match table::action(ByRule::BySecond, freq, ctx) {
        Action::Expand => {
            let mut result = Vec::new();
            for &dt in candidates {
                for &s in by_second {
                    if let Ok(t) = jiff::civil::Time::new(dt.hour(), dt.minute(), s as i8, 0) {
                        result.push(dt.date().to_datetime(t));
                    }
                }
            }
            result
        }
        Action::Limit => candidates
            .iter()
            .filter(|dt| by_second.contains(&(dt.second() as u8)))
            .copied()
            .collect(),
        Action::NA => candidates.to_vec(),
    }
}

// r[impl record.rrule.eval.by-set-pos]
fn apply_by_set_pos(candidates: &[DateTime], by_set_pos: &[i32]) -> Vec<DateTime> {
    if by_set_pos.is_empty() {
        return candidates.to_vec();
    }
    let len = candidates.len();
    let mut result = Vec::new();
    for &pos in by_set_pos {
        let idx = if pos > 0 {
            (pos as usize).checked_sub(1)
        } else if pos < 0 {
            len.checked_sub((-pos) as usize)
        } else {
            continue;
        };
        if let Some(idx) = idx
            && idx < len
        {
            result.push(candidates[idx]);
        }
    }
    result.sort();
    result.dedup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i16, mo: i8, d: i8, h: i8, mi: i8, s: i8) -> DateTime {
        DateTime::new(y, mo, d, h, mi, s, 0).unwrap()
    }

    fn rule(freq: Frequency) -> RecurrenceRule {
        RecurrenceRule {
            frequency: freq,
            ..Default::default()
        }
    }

    #[test]
    fn expand_daily_no_by() {
        let r = rule(Frequency::Daily);
        let start = dt(2024, 1, 1, 9, 0, 0);
        assert_eq!(expand_period(&r, start, 0), vec![start]);
        assert_eq!(expand_period(&r, start, 1), vec![dt(2024, 1, 2, 9, 0, 0)]);
    }

    #[test]
    fn expand_weekly_by_day() {
        let r = RecurrenceRule {
            frequency: Frequency::Weekly,
            by_day: vec![
                NDay {
                    day: Weekday::Monday,
                    nth: None,
                },
                NDay {
                    day: Weekday::Wednesday,
                    nth: None,
                },
                NDay {
                    day: Weekday::Friday,
                    nth: None,
                },
            ],
            ..Default::default()
        };
        // 2024-01-01 is Monday
        let start = dt(2024, 1, 1, 9, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(
            c,
            vec![
                dt(2024, 1, 1, 9, 0, 0),
                dt(2024, 1, 3, 9, 0, 0),
                dt(2024, 1, 5, 9, 0, 0),
            ]
        );
    }

    #[test]
    fn expand_monthly_by_month_day() {
        let r = RecurrenceRule {
            frequency: Frequency::Monthly,
            by_month_day: vec![1, 15],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 10, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(c, vec![dt(2024, 1, 1, 10, 0, 0), dt(2024, 1, 15, 10, 0, 0)]);
    }

    // r[verify record.rrule.eval.by-day.monthly-expand]
    #[test]
    fn expand_monthly_by_day_expand() {
        let r = RecurrenceRule {
            frequency: Frequency::Monthly,
            by_day: vec![NDay {
                day: Weekday::Friday,
                nth: None,
            }],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 10, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(
            c,
            vec![
                dt(2024, 1, 5, 10, 0, 0),
                dt(2024, 1, 12, 10, 0, 0),
                dt(2024, 1, 19, 10, 0, 0),
                dt(2024, 1, 26, 10, 0, 0),
            ]
        );
    }

    #[test]
    fn expand_monthly_by_day_with_nth() {
        let r = RecurrenceRule {
            frequency: Frequency::Monthly,
            by_day: vec![NDay {
                day: Weekday::Friday,
                nth: Some(2),
            }],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 10, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(c, vec![dt(2024, 1, 12, 10, 0, 0)]);
    }

    // r[verify record.rrule.eval.negative.month-day]
    #[test]
    fn expand_monthly_last_day() {
        let r = RecurrenceRule {
            frequency: Frequency::Monthly,
            by_month_day: vec![-1],
            ..Default::default()
        };
        let start = dt(2024, 1, 31, 10, 0, 0);
        assert_eq!(expand_period(&r, start, 0), vec![dt(2024, 1, 31, 10, 0, 0)]);
        assert_eq!(expand_period(&r, start, 1), vec![dt(2024, 2, 29, 10, 0, 0)]);
    }

    // r[verify record.rrule.eval.by-day.yearly-expand]
    #[test]
    fn expand_yearly_by_month_by_day() {
        let r = RecurrenceRule {
            frequency: Frequency::Yearly,
            by_month: vec![ByMonth {
                month: 11,
                leap: false,
            }],
            by_day: vec![NDay {
                day: Weekday::Thursday,
                nth: Some(4),
            }],
            ..Default::default()
        };
        let start = dt(2024, 11, 1, 0, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(c, vec![dt(2024, 11, 28, 0, 0, 0)]);
    }

    #[test]
    fn expand_yearly_by_month() {
        let r = RecurrenceRule {
            frequency: Frequency::Yearly,
            by_month: vec![
                ByMonth {
                    month: 3,
                    leap: false,
                },
                ByMonth {
                    month: 6,
                    leap: false,
                },
            ],
            ..Default::default()
        };
        let start = dt(2024, 1, 15, 10, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(
            c,
            vec![dt(2024, 3, 15, 10, 0, 0), dt(2024, 6, 15, 10, 0, 0)]
        );
    }

    // r[verify record.rrule.eval.by-set-pos]
    #[test]
    fn by_set_pos_first_and_last() {
        let r = RecurrenceRule {
            frequency: Frequency::Monthly,
            by_day: vec![NDay {
                day: Weekday::Friday,
                nth: None,
            }],
            by_set_position: vec![1, -1],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 10, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(c, vec![dt(2024, 1, 5, 10, 0, 0), dt(2024, 1, 26, 10, 0, 0)]);
    }

    #[test]
    fn expand_daily_by_hour() {
        let r = RecurrenceRule {
            frequency: Frequency::Daily,
            by_hour: vec![9, 17],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 9, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(c, vec![dt(2024, 1, 1, 9, 0, 0), dt(2024, 1, 1, 17, 0, 0)]);
    }

    #[test]
    fn expand_hourly_by_minute() {
        let r = RecurrenceRule {
            frequency: Frequency::Hourly,
            by_minute: vec![0, 30],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 9, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(c, vec![dt(2024, 1, 1, 9, 0, 0), dt(2024, 1, 1, 9, 30, 0)]);
    }

    // r[verify record.rrule.eval.negative.year-day]
    #[test]
    fn expand_yearly_by_year_day() {
        let r = RecurrenceRule {
            frequency: Frequency::Yearly,
            by_year_day: vec![1, -1],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 0, 0, 0);
        let c = expand_period(&r, start, 0);
        assert_eq!(c, vec![dt(2024, 1, 1, 0, 0, 0), dt(2024, 12, 31, 0, 0, 0)]);
    }

    #[test]
    fn daily_by_day_limit() {
        let r = RecurrenceRule {
            frequency: Frequency::Daily,
            by_day: vec![
                NDay {
                    day: Weekday::Monday,
                    nth: None,
                },
                NDay {
                    day: Weekday::Wednesday,
                    nth: None,
                },
            ],
            ..Default::default()
        };
        let start = dt(2024, 1, 1, 9, 0, 0);
        // Monday passes
        assert_eq!(expand_period(&r, start, 0), vec![dt(2024, 1, 1, 9, 0, 0)]);
        // Tuesday filtered
        assert!(expand_period(&r, start, 1).is_empty());
        // Wednesday passes
        assert_eq!(expand_period(&r, start, 2), vec![dt(2024, 1, 3, 9, 0, 0)]);
    }
}
