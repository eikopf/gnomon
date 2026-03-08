# Gnomon: Design Gaps and Open Problems

## I. Entirely Missing from the Spec

### 1. The Gnomon Data Model (critical)

Gnomon compiles to its own calendar object first; iCalendar and JSCalendar are downstream export targets, not the primary data model. The spec defines record types (Event, Task, etc.) with field constraints, but there is no unified description of what a fully-evaluated `Calendar` object looks like, what invariants it holds, or how its parts relate. The implementation has `Calendar<'db>` in `types.rs`, but that structure is implementation-driven, not spec-driven. iCal compatibility matters for import (users should be able to use `*.ics` files as input without migrating everything at once), but the spec should define the Gnomon ontology on its own terms first.

### 2. Recurrence Rule Evaluation (critical)

The spec explicitly marks this as `TODO: describe the evaluation semantics of recurrence rules`. The evaluation semantics themselves are well-defined by RFC 5545 (and JSCalendar inherits them directly), so the question is not *what* the semantics should be but *when and how* untyped nested records become typed domain objects. Currently, recurrence rules are ordinary records with no structural validation — a reification/type-checking pass is needed to validate field presence, produce typed rrule objects, and then apply RFC 5545 evaluation. Additionally, fields like `by_day` are currently desugared as lists but should be treated as sets (duplicates are meaningless, order is irrelevant). This affects the data model and any future equality/comparison semantics.

### 3. Multi-file Merge Semantics (important, evolving)

The implementation has a `merge` subcommand and a 1000-line `merge.rs`, but the spec says nothing about merging. The current implementation hardcodes a specific merge strategy (exactly-one calendar declaration, name uniqueness, binding collision detection), but merge should ultimately be an emergent behavior of a proper (terminating) evaluation semantics rather than a fixed pipeline stage. A user should be able to define a root calendar file as a small program that pulls in other sources and combines them — including extracting calendar data from Markdown, Org files, or other formats. Think Hledger/Beancount or a weaker Nix: facilities for composition that guarantee termination without enforcing a specific project layout. This connects directly to #5 (bindings become variable definitions) and #6 (includes become source expressions) — both are placeholders for mechanisms that only make sense once evaluation exists. Expect this area to evolve significantly.

### 4. Querying / Filtering (future but worth noting)

The reserved subcommand `query` hints at this. A calendar language without the ability to ask "what's happening next week?" is incomplete. Not urgent, but the spec should at least sketch the design space.

---

## II. Specified Syntactically but Semantically Void

### 5. Bindings (`bind`) — purpose unclear, design deferred

`bind @name "string"` is syntactically defined but semantically void. The original intent is not variable substitution or macro expansion — bindings are most useful as a way to give calendar objects stable, human-friendly handles for use in an eventual query system (referring to objects by name rather than UID). A user might also want to bind objects by path rather than UID. The current syntax is misleading because it looks like an alias mechanism, but the real design space is "naming things for later reference." Additionally, it's unclear where binding data lives in the data model: if you import a Gnomon file that has bindings, what do you see? Do they become fields in a record? A top-level list? This is underspecified. This area depends on both the evaluation semantics (#3) and query system (#4) and can be revisited once those take shape.

### 6. Includes (`include`) — no resolution semantics, possible keyword split

The syntax parses `include "path/or/url"`, and lowering distinguishes paths from URIs, but the spec defines no resolution behavior. The original design intended `include` exclusively for foreign files (`.ics`, `.json`/JSCalendar). With an evaluation semantics (#3), Gnomon also needs a way to reference other Gnomon files — but this is a semantically different operation: foreign inclusion is data import (parse an opaque blob into the data model), while Gnomon-to-Gnomon reference is module composition (evaluate and bring definitions into scope). These have different error modes and composition rules. Open question: retain `include` for foreign data and use a different keyword (`import`, `use`, `from`, etc.) for Gnomon sources, or use a single keyword for both? The answer depends on what evaluation semantics look like — if Gnomon files become module-like, the distinction is natural; if they're more like fragments, it's weaker. The boundary also blurs for hybrid cases (Gnomon blocks in Markdown).

### 7. Calendar Declaration (spec gap)

`calendar { ... }` parses and the implementation enforces exactly one, but the spec defines no fields or semantics. The primary purpose of the calendar declaration is to define the root UID, which serves as the namespace for UUIDv5 derivation: any event or task that omits an explicit `uid` gets a deterministic one computed from `UUIDv5(calendar_uid, object_name)`. Everything else on the calendar declaration (name, title, color, etc.) is metadata. The spec needs to formalize the accepted fields and the UUIDv5 algorithm. All events and tasks belong to a single calendar object (analogous to JSCalendar's Group members).

---

## III. Orphaned Keywords

### 8. `override` and `local`

Both are listed as weak keywords with no grammar production or semantic rule. `local` relates to the "local datetime" concept (now defined in the spec via `r[lexer.datetime.local]`) — its role just needs to be clarified or dropped if datetimes are always local. `override` came from an earlier brainstorming session as a way to alter record fields, but is superseded by the broader evaluation semantics direction (#3) — record mutation will need a more robust design than a dedicated keyword. Both can be revisited once evaluation semantics take shape.

---

## Prioritized Recommendations

**Near-term (requires design work):**
1. Specify the Gnomon data model / calendar object (#1)
2. Specify calendar declaration fields and UUIDv5 derivation (#7)
3. Design the reification pass (untyped records → typed domain objects) (#2)

**Longer-term (depends on evaluation semantics):**
4. Design evaluation semantics for composition/merge (#3)
5. Design binding semantics and data model placement (#5)
6. Design include/import resolution and keyword split (#6)
7. Specify recurrence rule evaluation (follows from #3, per RFC 5545) (#2)
8. Design query system (#4)
9. Clarify or remove `override` and `local` keywords (#8)

---

## Resolved

The following issues from the original analysis have been addressed:

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
