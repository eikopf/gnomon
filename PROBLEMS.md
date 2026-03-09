# Gnomon: Design Gaps and Open Problems

## I. Entirely Missing from the Spec

### 1. Recurrence Rule Evaluation (partially addressed)

The spec marks this as `TODO: describe the evaluation semantics of recurrence rules`. The evaluation semantics are well-defined by RFC 5545 / JSCalendar, so the question is not *what* they should be but *when and how* they are applied. Shape-checking validates recurrence rule records against their type definitions (`r[model.shape.*]`). What remains is the actual expansion of a rule into occurrences, and the treatment of `by_day` and similar fields as sets rather than lists.

### 2. Querying / Filtering (future)

The reserved subcommand `query` hints at this. A calendar language without the ability to ask "what's happening next week?" is incomplete. Not urgent, but the spec should sketch the design space.

### 3. Spec Introduction

The spec introduction is still `TODO: write prose introduction`. Not a functional gap, but a documentation one.

---

## II. Implementation Gaps

### 4. URI Imports

Import expressions accept URI literals syntactically, but evaluation emits `"URI imports are not yet supported"` and returns `undefined`. Only path-based imports (Gnomon, iCalendar, JSCalendar) are functional.

### 5. Multi-file Merge Semantics (functional, could evolve)

The `merge` subcommand works: it evaluates each source file, flattens the resulting values into records, separates calendar properties from entries, checks uniqueness constraints (single calendar, unique names), and runs shape-checking. However, the merge pipeline is a fixed post-evaluation stage rather than something expressible in the language itself. With `import` and `let` now available, a user *could* compose files via a root file that imports and merges others using `//` and `++`, but the `merge` CLI subcommand still applies its own uniqueness and shape-checking logic. Whether merge should eventually become a library of Gnomon functions rather than a hardcoded pipeline step is an open design question.

---

## Prioritized Recommendations

**Near-term (concrete next steps):**
1. Implement recurrence rule evaluation — expanding a rule into occurrences (#1)
2. Implement URI imports (#4)

**Longer-term (design work):**
3. Design query system (#2)
4. Consider making merge composable in-language (#5)

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
- **Shape-checking** — specified via `r[model.shape.*]` and implemented in `eval/shape.rs`; validates calendar, event, task, and all nested record types; error-resilient, recursive, preserves open records; wired into merge pipeline
- **Orphaned `local` keyword** — removed from `r[lexer.keyword.weak]`; the "local datetime" concept (`r[lexer.datetime.local]`) remains but needs no keyword since locality is the default (absence of `time_zone` field)
- **Foreign format imports** — iCalendar (`.ics`) and JSCalendar (`.json`) imports implemented via `calico` and `serde_json` respectively; format inferred from file extension or specified with `as icalendar`/`as jscalendar`; `name` requirement relaxed to allow entries with `uid` but no `name` (`r[record.event.name+2]`, `r[record.task.name+2]`, `r[expr.import.format+2]`)
- **UUIDv5 derivation** — implemented in `eval/merge.rs`; derives `UUIDv5(calendar_uid, name)` for entries without explicit `uid`
