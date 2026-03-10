# Spec TODOs

All previously identified underspecified areas have been addressed with new spec requirements. The remaining work is adding `r[impl]` and `r[verify]` tags to the implementation code.

## Uncovered Requirements (28)

The following new requirements need `r[impl ...]` tags in the implementation and `r[verify ...]` tags in tests:

### RRULE Expansion Engine (21 requirements)

Files: `crates/gnomon-rrule/src/{expand,iter,table,util}.rs`

- `record.rrule.eval.advance.yearly` — `util::advance_period` (Yearly branch)
- `record.rrule.eval.advance.monthly` — `util::advance_period` (Monthly branch)
- `record.rrule.eval.advance.weekly` — `util::advance_period` (Weekly branch)
- `record.rrule.eval.advance.daily` — `util::advance_period` (Daily branch)
- `record.rrule.eval.advance.sub-daily` — `util::advance_period` (Hourly/Minutely/Secondly)
- `record.rrule.eval.table` — `table::action`
- `record.rrule.eval.table.by-day-yearly` — `table::action` (ByDay/Yearly branch)
- `record.rrule.eval.table.by-day-monthly` — `table::action` (ByDay/Monthly branch)
- `record.rrule.eval.negative.month-day` — `util::resolve_month_day`
- `record.rrule.eval.negative.year-day` — `util::resolve_year_day`
- `record.rrule.eval.negative.weekday` — `util::nth_weekday_in_month`/`nth_weekday_in_year` (negative nth)
- `record.rrule.eval.skip.omit` — `util::apply_skip` (Omit branch)
- `record.rrule.eval.skip.forward` — `util::apply_skip` (Forward branch)
- `record.rrule.eval.skip.backward` — `util::apply_skip` (Backward branch)
- `record.rrule.eval.skip.default` — `types::Skip::default()`
- `record.rrule.eval.by-set-pos` — `expand::apply_by_set_pos`
- `record.rrule.eval.by-week-no` — `expand::apply_by_week_no`
- `record.rrule.eval.iso-week` — `util::dates_in_iso_week`/`iso_weeks_in_year`
- `record.rrule.eval.by-day.monthly-expand` — `expand::apply_by_day` (Monthly/Expand)
- `record.rrule.eval.by-day.yearly-expand` — `expand::apply_by_day` (Yearly/Expand)
- `record.rrule.eval.retry` — `iter::OccurrenceIter::next` (1000-period retry loop)

### Output Format (7 requirements)

Files: `crates/gnomon-db/src/eval/render.rs`

- `cli.subcommand.eval.output.string` — `write_value` (String branch)
- `cli.subcommand.eval.output.integer` — `write_value` (Integer branch)
- `cli.subcommand.eval.output.bool` — `write_value` (Bool branch)
- `cli.subcommand.eval.output.undefined` — `write_value` (Undefined branch)
- `cli.subcommand.eval.output.name` — `write_value` (Name branch)
- `cli.subcommand.eval.output.list` — `write_value` (List branch)
- `cli.subcommand.eval.output.record` — `write_record`
