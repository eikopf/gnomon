# Gnomon: Design Gaps and Open Problems

## I. Entirely Missing from the Spec

### 1. UUIDv5 Derivation (specified, not yet implemented)

The spec defines `r[model.calendar.uid.derivation]`: entries without an explicit `uid` should derive one as `UUIDv5(calendar_uid, name)`. The `uid` field on the calendar is enforced via shape-checking, and the derivation rule is specified, but the actual UUIDv5 computation has not been implemented yet.

### 2. Recurrence Rule Evaluation (partially addressed)

The spec marks this as `TODO: describe the evaluation semantics of recurrence rules`. The evaluation semantics are well-defined by RFC 5545 / JSCalendar, so the question is not *what* they should be but *when and how* they are applied. Shape-checking validates recurrence rule records against their type definitions (`r[model.shape.*]`). What remains is the actual expansion of a rule into occurrences, and the treatment of `by_day` and similar fields as sets rather than lists.

### 3. Foreign Format Imports (specified, not yet implemented)

The spec defines `import` expressions with `as icalendar` and `as jscalendar` format specifiers (`r[expr.import.format]`). Only Gnomon-to-Gnomon imports are implemented. Importing iCalendar (`.ics`) or JSCalendar (`.json`) files — translating foreign data into the Gnomon data model — is not yet supported. URI-based imports are also unimplemented.

### 4. Querying / Filtering (future)

The reserved subcommand `query` hints at this. A calendar language without the ability to ask "what's happening next week?" is incomplete. Not urgent, but the spec should sketch the design space.

---

## II. Implementation Gaps

### 5. Multi-file Merge Semantics (functional, could evolve)

The `merge` subcommand works: it evaluates each source file, flattens the resulting values into records, separates calendar properties from entries, checks uniqueness constraints (single calendar, unique names), and runs shape-checking. However, the merge pipeline is a fixed post-evaluation stage rather than something expressible in the language itself. With `import` and `let` now available, a user *could* compose files via a root file that imports and merges others using `//` and `++`, but the `merge` CLI subcommand still applies its own uniqueness and shape-checking logic. Whether merge should eventually become a library of Gnomon functions rather than a hardcoded pipeline step is an open design question.

### 6. Orphaned `local` Keyword

`local` is listed as a weak keyword with no grammar production or semantic rule. It relates to the "local datetime" concept (defined in the spec via `r[lexer.datetime.local]`) but has no parser or evaluator support. It should be clarified or removed.

---

## Prioritized Recommendations

**Near-term (concrete next steps):**
1. Implement UUIDv5 derivation for entries missing explicit `uid` (#1)
2. Implement recurrence rule evaluation — expanding a rule into occurrences (#2)
3. Implement iCalendar/JSCalendar import (#3)

**Longer-term (design work):**
4. Design query system (#4)
5. Consider making merge composable in-language (#5)
6. Clarify or remove `local` keyword (#6)

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
- **Calendar declaration fields and UUIDv5** — specified via `r[model.calendar.uid]` and `r[model.calendar.uid.derivation]`; `uid` enforced via shape-checking; UUIDv5 derivation pending
- **Include resolution semantics** — superseded by `import` expressions (`r[expr.import.*]`, `r[model.import.*]`)
- **Binding semantics** — superseded by `let` bindings (`r[expr.let.*]`, `r[syntax.file.let]`)
- **Shape-checking** — specified via `r[model.shape.*]` and implemented in `eval/shape.rs`; validates calendar, event, task, and all nested record types; error-resilient, recursive, preserves open records; wired into merge pipeline
