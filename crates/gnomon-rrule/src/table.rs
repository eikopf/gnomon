use crate::types::Frequency;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ByRule {
    ByMonth,
    ByWeekNo,
    ByYearDay,
    ByMonthDay,
    ByDay,
    ByHour,
    ByMinute,
    BySecond,
    BySetPos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Expand,
    Limit,
    NA,
}

/// Context needed for BYDAY conditional logic.
pub struct ByDayContext {
    pub has_by_month_day: bool,
    pub has_by_year_day: bool,
    pub has_by_week_no: bool,
    pub has_by_month: bool,
}

/// RFC 5545 §3.3.10 expand/limit table.
///
/// Returns the action (Expand, Limit, or NA) for a given BY* rule at a given frequency.
/// BYDAY has conditional behavior depending on which other BY* rules are present.
pub fn action(rule: ByRule, freq: Frequency, ctx: &ByDayContext) -> Action {
    use Action::*;
    use ByRule::*;
    use Frequency::*;

    match rule {
        // BYMONTH: Expand for YEARLY, Limit for everything else
        ByMonth => match freq {
            Yearly => Expand,
            _ => Limit,
        },

        // BYWEEKNO: Expand for YEARLY only, NA otherwise
        ByWeekNo => match freq {
            Yearly => Expand,
            _ => NA,
        },

        // BYYEARDAY: Expand for YEARLY, Limit for DAILY/HOURLY/MINUTELY/SECONDLY, NA otherwise
        ByYearDay => match freq {
            Yearly => Expand,
            Daily | Hourly | Minutely | Secondly => Limit,
            _ => NA,
        },

        // BYMONTHDAY: Expand for YEARLY/MONTHLY, Limit for DAILY/HOURLY/MINUTELY/SECONDLY, NA for WEEKLY
        ByMonthDay => match freq {
            Yearly | Monthly => Expand,
            Weekly => NA,
            Daily | Hourly | Minutely | Secondly => Limit,
        },

        // BYDAY: complex conditional
        ByDay => match freq {
            Weekly => Expand,
            Daily | Hourly | Minutely | Secondly => Limit,
            Monthly => {
                // N1: Limit if BYMONTHDAY present, else Expand
                if ctx.has_by_month_day { Limit } else { Expand }
            }
            Yearly => {
                // N2: Limit if BYYEARDAY or BYMONTHDAY present;
                // expand within BYWEEKNO if present;
                // expand within BYMONTH if present;
                // else expand within entire year.
                if ctx.has_by_year_day || ctx.has_by_month_day {
                    Limit
                } else {
                    Expand
                }
            }
        },

        // BYHOUR: Expand for DAILY+, Limit for sub-daily
        ByHour => match freq {
            Yearly | Monthly | Weekly | Daily => Expand,
            Hourly | Minutely | Secondly => Limit,
        },

        // BYMINUTE: Expand for HOURLY+, Limit for sub-minutely
        ByMinute => match freq {
            Yearly | Monthly | Weekly | Daily | Hourly => Expand,
            Minutely | Secondly => Limit,
        },

        // BYSECOND: Expand for MINUTELY+, Limit for SECONDLY
        BySecond => match freq {
            Yearly | Monthly | Weekly | Daily | Hourly | Minutely => Expand,
            Secondly => Limit,
        },

        // BYSETPOS: always acts as a Limit (filter) — applied after expansion
        BySetPos => Limit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;
    use Frequency::*;

    fn no_ctx() -> ByDayContext {
        ByDayContext {
            has_by_month_day: false,
            has_by_year_day: false,
            has_by_week_no: false,
            has_by_month: false,
        }
    }

    #[test]
    fn by_month_actions() {
        assert_eq!(action(ByRule::ByMonth, Yearly, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByMonth, Monthly, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByMonth, Weekly, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByMonth, Daily, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByMonth, Hourly, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByMonth, Minutely, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByMonth, Secondly, &no_ctx()), Limit);
    }

    #[test]
    fn by_week_no_actions() {
        assert_eq!(action(ByRule::ByWeekNo, Yearly, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByWeekNo, Monthly, &no_ctx()), NA);
        assert_eq!(action(ByRule::ByWeekNo, Weekly, &no_ctx()), NA);
        assert_eq!(action(ByRule::ByWeekNo, Daily, &no_ctx()), NA);
    }

    #[test]
    fn by_year_day_actions() {
        assert_eq!(action(ByRule::ByYearDay, Yearly, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByYearDay, Monthly, &no_ctx()), NA);
        assert_eq!(action(ByRule::ByYearDay, Weekly, &no_ctx()), NA);
        assert_eq!(action(ByRule::ByYearDay, Daily, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByYearDay, Secondly, &no_ctx()), Limit);
    }

    #[test]
    fn by_month_day_actions() {
        assert_eq!(action(ByRule::ByMonthDay, Yearly, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByMonthDay, Monthly, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByMonthDay, Weekly, &no_ctx()), NA);
        assert_eq!(action(ByRule::ByMonthDay, Daily, &no_ctx()), Limit);
    }

    #[test]
    fn by_day_weekly_expands() {
        assert_eq!(action(ByRule::ByDay, Weekly, &no_ctx()), Expand);
    }

    #[test]
    fn by_day_daily_limits() {
        assert_eq!(action(ByRule::ByDay, Daily, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByDay, Hourly, &no_ctx()), Limit);
    }

    #[test]
    fn by_day_monthly_conditional() {
        // Without BYMONTHDAY → Expand
        assert_eq!(action(ByRule::ByDay, Monthly, &no_ctx()), Expand);
        // With BYMONTHDAY → Limit
        let ctx = ByDayContext { has_by_month_day: true, ..no_ctx() };
        assert_eq!(action(ByRule::ByDay, Monthly, &ctx), Limit);
    }

    #[test]
    fn by_day_yearly_conditional() {
        // No other BY* → Expand (within entire year)
        assert_eq!(action(ByRule::ByDay, Yearly, &no_ctx()), Expand);

        // With BYMONTHDAY → Limit
        let ctx = ByDayContext { has_by_month_day: true, ..no_ctx() };
        assert_eq!(action(ByRule::ByDay, Yearly, &ctx), Limit);

        // With BYYEARDAY → Limit
        let ctx = ByDayContext { has_by_year_day: true, ..no_ctx() };
        assert_eq!(action(ByRule::ByDay, Yearly, &ctx), Limit);

        // With BYWEEKNO only → Expand (within weeks)
        let ctx = ByDayContext { has_by_week_no: true, ..no_ctx() };
        assert_eq!(action(ByRule::ByDay, Yearly, &ctx), Expand);

        // With BYMONTH only → Expand (within months)
        let ctx = ByDayContext { has_by_month: true, ..no_ctx() };
        assert_eq!(action(ByRule::ByDay, Yearly, &ctx), Expand);
    }

    #[test]
    fn by_hour_actions() {
        assert_eq!(action(ByRule::ByHour, Yearly, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByHour, Daily, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByHour, Hourly, &no_ctx()), Limit);
        assert_eq!(action(ByRule::ByHour, Secondly, &no_ctx()), Limit);
    }

    #[test]
    fn by_minute_actions() {
        assert_eq!(action(ByRule::ByMinute, Hourly, &no_ctx()), Expand);
        assert_eq!(action(ByRule::ByMinute, Minutely, &no_ctx()), Limit);
    }

    #[test]
    fn by_second_actions() {
        assert_eq!(action(ByRule::BySecond, Minutely, &no_ctx()), Expand);
        assert_eq!(action(ByRule::BySecond, Secondly, &no_ctx()), Limit);
    }

    #[test]
    fn by_set_pos_always_limits() {
        for freq in [Yearly, Monthly, Weekly, Daily, Hourly, Minutely, Secondly] {
            assert_eq!(action(ByRule::BySetPos, freq, &no_ctx()), Limit);
        }
    }
}
