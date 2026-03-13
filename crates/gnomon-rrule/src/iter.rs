use crate::expand;
use crate::types::*;

/// Borrowed iterator over occurrences.
pub struct OccurrenceIter<'a> {
    rule: &'a RecurrenceRule,
    dtstart: DateTime,
    period_index: u64,
    buffer: Vec<DateTime>,
    buffer_pos: usize,
    yielded: u64,
    done: bool,
    started: bool,
}

impl<'a> OccurrenceIter<'a> {
    pub fn new(rule: &'a RecurrenceRule, dtstart: DateTime) -> Self {
        Self {
            rule,
            dtstart,
            period_index: 0,
            buffer: Vec::new(),
            buffer_pos: 0,
            yielded: 0,
            done: false,
            started: false,
        }
    }
}

// r[impl record.rrule.eval.empty]
impl Iterator for OccurrenceIter<'_> {
    type Item = DateTime;

    fn next(&mut self) -> Option<DateTime> {
        if self.done {
            return None;
        }

        // r[impl record.rrule.eval.dtstart]
        // DTSTART is always the first occurrence (RFC 5545)
        if !self.started {
            self.started = true;
            if self.check_termination(self.dtstart) {
                self.yielded += 1;
                return Some(self.dtstart);
            } else {
                self.done = true;
                return None;
            }
        }

        loop {
            // Try to get next from buffer
            while self.buffer_pos < self.buffer.len() {
                let dt = self.buffer[self.buffer_pos];
                self.buffer_pos += 1;

                // Skip occurrences <= dtstart (already yielded as first)
                if dt <= self.dtstart {
                    continue;
                }

                if self.check_termination(dt) {
                    self.yielded += 1;
                    return Some(dt);
                } else {
                    self.done = true;
                    return None;
                }
            }

            // Buffer exhausted, expand next period
            self.buffer = expand::expand_period(self.rule, self.dtstart, self.period_index);
            self.buffer_pos = 0;
            self.period_index += 1;

            // r[impl record.rrule.eval.retry]
            // If expansion returned nothing, we've likely overflowed
            if self.buffer.is_empty() {
                // Try a few more periods before giving up (sparse rules may skip periods)
                let mut tries = 0;
                while self.buffer.is_empty() && tries < 1000 {
                    self.buffer = expand::expand_period(self.rule, self.dtstart, self.period_index);
                    self.buffer_pos = 0;
                    self.period_index += 1;
                    tries += 1;
                }
                if self.buffer.is_empty() {
                    self.done = true;
                    return None;
                }
            }
        }
    }
}

impl OccurrenceIter<'_> {
    fn check_termination(&self, dt: DateTime) -> bool {
        match &self.rule.termination {
            Termination::None => true,
            Termination::Count(n) => self.yielded < *n,
            Termination::Until(until) => dt <= *until,
        }
    }
}

/// Owned iterator over occurrences (for IntoIterator).
pub struct OwnedOccurrenceIter {
    rule: RecurrenceRule,
    dtstart: DateTime,
    period_index: u64,
    buffer: Vec<DateTime>,
    buffer_pos: usize,
    yielded: u64,
    done: bool,
    started: bool,
}

impl OwnedOccurrenceIter {
    pub fn new(rule: RecurrenceRule, dtstart: DateTime) -> Self {
        Self {
            rule,
            dtstart,
            period_index: 0,
            buffer: Vec::new(),
            buffer_pos: 0,
            yielded: 0,
            done: false,
            started: false,
        }
    }
}

impl Iterator for OwnedOccurrenceIter {
    type Item = DateTime;

    fn next(&mut self) -> Option<DateTime> {
        if self.done {
            return None;
        }

        if !self.started {
            self.started = true;
            if self.check_termination(self.dtstart) {
                self.yielded += 1;
                return Some(self.dtstart);
            } else {
                self.done = true;
                return None;
            }
        }

        loop {
            while self.buffer_pos < self.buffer.len() {
                let dt = self.buffer[self.buffer_pos];
                self.buffer_pos += 1;

                if dt <= self.dtstart {
                    continue;
                }

                if self.check_termination(dt) {
                    self.yielded += 1;
                    return Some(dt);
                } else {
                    self.done = true;
                    return None;
                }
            }

            self.buffer = expand::expand_period(&self.rule, self.dtstart, self.period_index);
            self.buffer_pos = 0;
            self.period_index += 1;

            if self.buffer.is_empty() {
                let mut tries = 0;
                while self.buffer.is_empty() && tries < 1000 {
                    self.buffer =
                        expand::expand_period(&self.rule, self.dtstart, self.period_index);
                    self.buffer_pos = 0;
                    self.period_index += 1;
                    tries += 1;
                }
                if self.buffer.is_empty() {
                    self.done = true;
                    return None;
                }
            }
        }
    }
}

impl OwnedOccurrenceIter {
    fn check_termination(&self, dt: DateTime) -> bool {
        match &self.rule.termination {
            Termination::None => true,
            Termination::Count(n) => self.yielded < *n,
            Termination::Until(until) => dt <= *until,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn dt(y: i16, mo: i8, d: i8, h: i8, mi: i8, s: i8) -> DateTime {
        DateTime::new(y, mo, d, h, mi, s, 0).unwrap()
    }

    // r[verify record.rrule.eval.empty]
    #[test]
    fn count_zero_yields_empty() {
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            termination: Termination::Count(0),
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 1, 9, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert!(dates.is_empty(), "COUNT=0 should produce no occurrences");
    }

    // r[verify record.rrule.eval.dtstart]
    #[test]
    fn daily_count_5() {
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            termination: Termination::Count(5),
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 1, 9, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 1, 9, 0, 0),
                dt(2024, 1, 2, 9, 0, 0),
                dt(2024, 1, 3, 9, 0, 0),
                dt(2024, 1, 4, 9, 0, 0),
                dt(2024, 1, 5, 9, 0, 0),
            ]
        );
    }

    #[test]
    fn daily_until() {
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            termination: Termination::Until(dt(2024, 1, 3, 23, 59, 59)),
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 1, 9, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 1, 9, 0, 0),
                dt(2024, 1, 2, 9, 0, 0),
                dt(2024, 1, 3, 9, 0, 0),
            ]
        );
    }

    #[test]
    fn weekly_mwf_count_6() {
        let rule = RecurrenceRule {
            frequency: Frequency::Weekly,
            termination: Termination::Count(6),
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
        let occ = Occurrences::new(rule, dt(2024, 1, 1, 9, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 1, 9, 0, 0),
                dt(2024, 1, 3, 9, 0, 0),
                dt(2024, 1, 5, 9, 0, 0),
                dt(2024, 1, 8, 9, 0, 0),
                dt(2024, 1, 10, 9, 0, 0),
                dt(2024, 1, 12, 9, 0, 0),
            ]
        );
    }

    #[test]
    fn monthly_on_15th_count_3() {
        let rule = RecurrenceRule {
            frequency: Frequency::Monthly,
            termination: Termination::Count(3),
            by_month_day: vec![15],
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 15, 10, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 15, 10, 0, 0),
                dt(2024, 2, 15, 10, 0, 0),
                dt(2024, 3, 15, 10, 0, 0),
            ]
        );
    }

    // r[verify record.rrule.eval.negative.weekday]
    #[test]
    fn monthly_last_friday() {
        let rule = RecurrenceRule {
            frequency: Frequency::Monthly,
            termination: Termination::Count(3),
            by_day: vec![NDay {
                day: Weekday::Friday,
                nth: Some(-1),
            }],
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 26, 10, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 26, 10, 0, 0),
                dt(2024, 2, 23, 10, 0, 0),
                dt(2024, 3, 29, 10, 0, 0),
            ]
        );
    }

    #[test]
    fn yearly_every_march_15() {
        let rule = RecurrenceRule {
            frequency: Frequency::Yearly,
            termination: Termination::Count(3),
            by_month: vec![ByMonth {
                month: 3,
                leap: false,
            }],
            by_month_day: vec![15],
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 3, 15, 0, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 3, 15, 0, 0, 0),
                dt(2025, 3, 15, 0, 0, 0),
                dt(2026, 3, 15, 0, 0, 0),
            ]
        );
    }

    #[test]
    fn yearly_thanksgiving() {
        // 4th Thursday of November
        let rule = RecurrenceRule {
            frequency: Frequency::Yearly,
            termination: Termination::Count(3),
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
        let occ = Occurrences::new(rule, dt(2024, 11, 28, 0, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 11, 28, 0, 0, 0),
                dt(2025, 11, 27, 0, 0, 0),
                dt(2026, 11, 26, 0, 0, 0),
            ]
        );
    }

    #[test]
    fn every_other_week() {
        let rule = RecurrenceRule {
            frequency: Frequency::Weekly,
            interval: 2,
            termination: Termination::Count(4),
            by_day: vec![NDay {
                day: Weekday::Tuesday,
                nth: None,
            }],
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 2, 9, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 2, 9, 0, 0),
                dt(2024, 1, 16, 9, 0, 0),
                dt(2024, 1, 30, 9, 0, 0),
                dt(2024, 2, 13, 9, 0, 0),
            ]
        );
    }

    #[test]
    fn into_iterator() {
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            termination: Termination::Count(3),
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 1, 0, 0, 0));
        let dates: Vec<_> = occ.into_iter().collect();
        assert_eq!(dates.len(), 3);
    }

    #[test]
    fn is_finite_checks() {
        let finite = Occurrences::new(
            RecurrenceRule {
                frequency: Frequency::Daily,
                termination: Termination::Count(5),
                ..Default::default()
            },
            dt(2024, 1, 1, 0, 0, 0),
        );
        assert!(finite.is_finite());
        assert_eq!(finite.count(), Some(5));

        let infinite = Occurrences::new(
            RecurrenceRule {
                frequency: Frequency::Daily,
                ..Default::default()
            },
            dt(2024, 1, 1, 0, 0, 0),
        );
        assert!(!infinite.is_finite());
        assert_eq!(infinite.count(), None);
    }

    // r[verify record.rrule.eval.skip.default]
    // r[verify record.rrule.eval.retry]
    #[test]
    fn monthly_31st_skip_omit() {
        // Every month on the 31st, skip=omit → months without 31 days are skipped
        let rule = RecurrenceRule {
            frequency: Frequency::Monthly,
            termination: Termination::Count(5),
            by_month_day: vec![31],
            skip: Skip::Omit,
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 31, 10, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 31, 10, 0, 0),
                dt(2024, 3, 31, 10, 0, 0),
                dt(2024, 5, 31, 10, 0, 0),
                dt(2024, 7, 31, 10, 0, 0),
                dt(2024, 8, 31, 10, 0, 0),
            ]
        );
    }

    #[test]
    fn secondly_count() {
        let rule = RecurrenceRule {
            frequency: Frequency::Secondly,
            termination: Termination::Count(5),
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 1, 0, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 1, 0, 0, 0),
                dt(2024, 1, 1, 0, 0, 1),
                dt(2024, 1, 1, 0, 0, 2),
                dt(2024, 1, 1, 0, 0, 3),
                dt(2024, 1, 1, 0, 0, 4),
            ]
        );
    }

    #[test]
    fn yearly_feb_29_leap() {
        // Yearly on Feb 29 — only occurs in leap years
        let rule = RecurrenceRule {
            frequency: Frequency::Yearly,
            termination: Termination::Count(3),
            by_month: vec![ByMonth {
                month: 2,
                leap: false,
            }],
            by_month_day: vec![29],
            skip: Skip::Omit,
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 2, 29, 0, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(
            dates,
            vec![
                dt(2024, 2, 29, 0, 0, 0),
                dt(2028, 2, 29, 0, 0, 0),
                dt(2032, 2, 29, 0, 0, 0),
            ]
        );
    }

    #[test]
    fn daily_by_month_limit() {
        // Daily but only in January
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            termination: Termination::Count(5),
            by_month: vec![ByMonth {
                month: 1,
                leap: false,
            }],
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 29, 0, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        // Jan 29, 30, 31 — then skips to next January
        assert_eq!(dates[0], dt(2024, 1, 29, 0, 0, 0));
        assert_eq!(dates[1], dt(2024, 1, 30, 0, 0, 0));
        assert_eq!(dates[2], dt(2024, 1, 31, 0, 0, 0));
        assert_eq!(dates[3], dt(2025, 1, 1, 0, 0, 0));
        assert_eq!(dates[4], dt(2025, 1, 2, 0, 0, 0));
    }

    #[test]
    fn interval_zero_treated_as_one() {
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            interval: 0,
            termination: Termination::Count(3),
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 1, 0, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        assert_eq!(dates.len(), 3);
        // Should behave like interval=1
        assert_eq!(dates[0], dt(2024, 1, 1, 0, 0, 0));
        assert_eq!(dates[1], dt(2024, 1, 2, 0, 0, 0));
        assert_eq!(dates[2], dt(2024, 1, 3, 0, 0, 0));
    }

    // r[verify record.rrule.eval.by-set-pos]
    #[test]
    fn by_set_pos_last_weekday_of_month() {
        // Last weekday (Mon-Fri) of each month
        let rule = RecurrenceRule {
            frequency: Frequency::Monthly,
            termination: Termination::Count(3),
            by_day: vec![
                NDay {
                    day: Weekday::Monday,
                    nth: None,
                },
                NDay {
                    day: Weekday::Tuesday,
                    nth: None,
                },
                NDay {
                    day: Weekday::Wednesday,
                    nth: None,
                },
                NDay {
                    day: Weekday::Thursday,
                    nth: None,
                },
                NDay {
                    day: Weekday::Friday,
                    nth: None,
                },
            ],
            by_set_position: vec![-1],
            ..Default::default()
        };
        let occ = Occurrences::new(rule, dt(2024, 1, 31, 10, 0, 0));
        let dates: Vec<_> = occ.iter().collect();
        // Last weekday of Jan 2024 = Wed Jan 31
        // Last weekday of Feb 2024 = Thu Feb 29
        // Last weekday of Mar 2024 = Fri Mar 29
        assert_eq!(
            dates,
            vec![
                dt(2024, 1, 31, 10, 0, 0),
                dt(2024, 2, 29, 10, 0, 0),
                dt(2024, 3, 29, 10, 0, 0),
            ]
        );
    }
}
