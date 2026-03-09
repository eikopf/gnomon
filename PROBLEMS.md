# Gnomon: Design Gaps and Open Problems

## I. Entirely Missing from the Spec

### 1. Querying / Filtering (future)

The reserved subcommand `query` hints at this. A calendar language without the ability to ask "what's happening next week?" is incomplete. Not urgent, but the spec should sketch the design space.

### 2. Spec Introduction

The spec introduction is still `TODO: write prose introduction`. Not a functional gap, but a documentation one.

---

## II. Implementation Gaps

### 3. Eager Recurrence Materialization

The `check` pipeline in `eval/rrule.rs` eagerly materializes all occurrences and injects an `occurrences` field onto each entry record. This is incorrect: `check` should not mutate entry records, and infinite rules should not need capping. The spec defines recurrence semantics abstractly (`r[record.rrule.eval.infinite]`), and implementations should evaluate occurrences lazily within a queried time range. The `occurrences` field is not part of the data model.

---

## Prioritized Recommendations

**Near-term (design work):**
1. Design query system (#1)

---

## Resolved

The following issues from the original analysis have been fully addressed:

- **Expression-oriented evaluation model** — files now evaluate to values; declarations are syntactic sugar for records (`r[syntax.file.body]`, `r[decl.*.desugar]`); full Pratt parser with operators (`++`, `//`, `==`, `!=`, `.field`, `[index]`)
- **`include` / `bind` removed** — replaced by `import` expressions (`r[expr.import.*]`) and `let` bindings (`r[expr.let.*]`, `r[syntax.file.let]`); imports implemented with cycle detection and relative path resolution
- **`override` keyword removed** — no longer in the language; record mutation handled by `//` merge operator
- **Short-form declaration desugaring** — specified via `r[decl.short-event.desugar]` and `r[decl.short-task.desugar]`
- **`undefined` not a valid expression** — added to expression grammar (`r[expr.literal.syntax+3]`) and lowering
- **Names not valid in expressions** — added to expression grammar (parser and lowering already handled it)
- **`recur` field missing** — added via `r[field.recur.type]`
- **`every` `by_day` weekday desugaring** — fixed to use weekday keyword (`r[record.rrule.every.desugar.subject.weekday+2]`)
- **`group` declarations** — removed from spec and dead code cleaned from implementation
- **Calendrical date validation** — added `r[lexer.date.valid]`, `r[lexer.month-day.valid]`, and implementation
- **Common record field scope** — stated explicitly in spec
- **`check`/`eval`/`merge` CLI subcommands** — specified and `check` unreserved
- **"Local datetime" undefined** — defined via `r[lexer.datetime.local]`
- **Gnomon data model** — specified via `r[model.*]` requirements; implementation aligned (unified `entries` list, `type` field insertion during lowering)
- **Calendar declaration fields and UUIDv5** — specified via `r[model.calendar.uid]` and `r[model.calendar.uid.derivation]`; `uid` enforced via shape-checking; UUIDv5 derivation implemented in `eval/merge.rs` (derives `UUIDv5(calendar_uid, name)` for entries without explicit `uid`; calendar uid must be a valid UUID)
- **Include resolution semantics** — superseded by `import` expressions (`r[expr.import.*]`, `r[model.import.*]`)
- **Binding semantics** — superseded by `let` bindings (`r[expr.let.*]`, `r[syntax.file.let]`)
- **Shape-checking** — specified via `r[model.shape.*]` and implemented in `eval/shape.rs`; validates calendar, event, task, and all nested record types; error-resilient, recursive, preserves open records; wired into validation pipeline
- **Orphaned `local` keyword** — removed from `r[lexer.keyword.weak]`; the "local datetime" concept (`r[lexer.datetime.local]`) remains but needs no keyword since locality is the default (absence of `time_zone` field)
- **Foreign format imports** — iCalendar (`.ics`) and JSCalendar (`.json`) imports implemented via `calico` and `serde_json` respectively; format inferred from file extension or specified with `as icalendar`/`as jscalendar`; `name` requirement relaxed to allow entries with `uid` but no `name` (`r[record.event.name+2]`, `r[record.task.name+2]`, `r[expr.import.format+2]`)
- **UUIDv5 derivation** — implemented in `eval/merge.rs`; derives `UUIDv5(calendar_uid, name)` for entries without explicit `uid`
- **Recurrence rule evaluation semantics** — specified via `r[record.rrule.eval.*]` requirements; expansion algorithm, termination, infinite rule support, and start-required constraint are all formally defined. Implementation still uses eager materialization (see Implementation Gaps #3)
- **URI imports** — `import <https://...>` now fetches content via HTTP(S) using `ureq`; format inferred from `as` keyword, URL path extension, or `Content-Type` header; error diagnostics on network/HTTP failures
- **Foreign import field mappings** — specified via `r[model.import.icalendar.*]` and `r[model.import.jscalendar.*]` requirements; complete field-by-field mapping tables for iCalendar and JSCalendar translation
- **Calendar singularity** — specified via `r[model.calendar.singular]`; exactly one calendar declaration required
- **Non-UUID calendar UID** — specified via `r[model.calendar.uid.derivation.non-uuid]`; derivation skipped with warning when calendar uid is not a valid UUID
- **URI import content-type inference** — specified via `r[expr.import.format.uri]`; format inferred from HTTP Content-Type header for URI imports without explicit format or recognized extension
- **Orphaned normative prose** — leap second tolerance tagged as `r[lexer.time.leap-second]`; whitespace insignificance tagged as `r[lexer.whitespace.insignificant]`
- **String literal import source** — removed from spec grammar (`r[expr.import.syntax+2]`) and implementation; only path and URI literals are valid import sources
- **Multi-file merge composability** — the old `merge` subcommand (which combined peer files) has been replaced by a unified `check` subcommand (`r[cli.subcommand.check+2]`) that evaluates a single root file (which transitively imports other files via `import` expressions), validates the result as a calendar, and warns about unused `.gnomon` files in the project directory (`r[cli.subcommand.check.unused]`)
