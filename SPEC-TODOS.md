# Spec TODOs

Areas where the implementation has normative behavior not yet covered by spec requirements.

## RRULE Expansion Engine (~20 new requirements)

The spec has 6 high-level `record.rrule.eval.*` requirements but `gnomon-rrule` implements the full RFC 5545 §3.3.10 algorithm. Needs requirements for:

- **Expand/limit table**: Each BY* rule (BYMONTH, BYWEEKNO, BYYEARDAY, BYMONTHDAY, BYDAY, BYHOUR, BYMINUTE, BYSECOND) behaves differently depending on frequency — expand, limit, or N/A. BYDAY has 6 conditional branches depending on which other BY rules are present.
- **Skip strategies**: `omit`/`forward`/`backward` for handling invalid dates produced during expansion (e.g. "every month on the 31st" in months with fewer days).
- **Negative indexing**: BYMONTHDAY(-1) = last day of month, BYYEARDAY(-1) = last day of year, nth weekday from end of month/year.
- **Period advancement**: Leap-year-safe month arithmetic with day-of-month clamping on overflow.
- **ISO week computation**: Week numbering with custom WKST (week start day), 52/53-week year handling.
- **BYSETPOS**: 1-based position filtering over the candidate set, with negative indices counting from end.
- **Empty-period retry**: Iterator tries up to 1000 empty periods before stopping (prevents infinite loops on rules that produce no occurrences).

Files: `crates/gnomon-rrule/src/{expand,iter,table,util,types}.rs`

## String Escape Sequences (~1 requirement)

`r[lexer.string.escape]` says escapes are recognized but doesn't enumerate which ones are valid. The implementation supports `\"`, `\\`, `\n`, `\t`. Needs a requirement listing the supported escape sequences.

Files: `crates/gnomon-db/src/eval/literals.rs`

## Import Path Resolution (~2 requirements)

- Relative path imports resolve from the importing file's parent directory.
- Cycle detection uses canonicalized (absolute) paths.

Files: `crates/gnomon-db/src/eval/lower.rs`

## Gregorian Calendar Validation (~1 requirement)

Date validation uses the Gregorian calendar (leap year algorithm, max days per month) but the spec just says dates must be "valid" without defining the calendar system.

Files: `crates/gnomon-parser/src/validate.rs`

## Prefix Form Required Fields (~2 requirements)

- `event { ... }` (prefix form) requires `name` and `start` fields.
- `task { ... }` (prefix form) requires `name` field.

Short-form desugaring implies these indirectly but the prefix form has no explicit field requirements in the spec.

Files: `crates/gnomon-parser/src/validate.rs`

## Output Format (optional, ~1-2 requirements)

`render.rs` defines how `eval` prints values (record syntax, list syntax, string quoting). Currently the spec just says "write a textual representation." Could be pinned down if a stable output format is desired.

Files: `crates/gnomon-db/src/eval/render.rs`
