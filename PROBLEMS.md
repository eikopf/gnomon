# Gnomon: Design Gaps and Open Problems

## I. Entirely Missing from the Spec

### 1. The Gnomon Data Model (critical)

Gnomon compiles to its own calendar object first; iCalendar and JSCalendar are downstream export targets, not the primary data model. The spec defines record types (Event, Task, Group, etc.) with field constraints, but there is no unified description of what a fully-evaluated `Calendar` object looks like, what invariants it holds, or how its parts relate. The implementation has `Calendar<'db>` in `types.rs`, but that structure is implementation-driven, not spec-driven. iCal compatibility matters for import (users should be able to use `*.ics` files as input without migrating everything at once), but the spec should define the Gnomon ontology on its own terms first.

### 2. Recurrence Rule Evaluation (critical)

The spec explicitly marks this as `TODO: describe the evaluation semantics of recurrence rules` (line 733). The evaluation semantics themselves are well-defined by RFC 5545 (and JSCalendar inherits them directly), so the question is not *what* the semantics should be but *when and how* untyped nested records become typed domain objects. Currently, recurrence rules are ordinary records with no structural validation — a reification/type-checking pass is needed to validate field presence, produce typed rrule objects, and then apply RFC 5545 evaluation. Additionally, fields like `by_day` are currently desugared as lists but should be treated as sets (duplicates are meaningless, order is irrelevant). This affects the data model and any future equality/comparison semantics.

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

### 7. Short-form Declaration Desugaring (spec gap)

`event @meeting 2024-01-15T10:00 1h "Team standup" { priority: 1 }` — each subexpression maps to a specific record field, and the exact mapping depends on the introducing keyword (`event` vs `task`). The implementation already does this correctly; it just needs to be written into the spec as formal desugaring rules.

### 8. Calendar Declaration (spec gap)

`calendar { ... }` parses and the implementation enforces exactly one, but the spec defines no fields or semantics. The primary purpose of the calendar declaration is to define the root UID, which serves as the namespace for UUIDv5 derivation: any event, task, or group that omits an explicit `uid` gets a deterministic one computed from `UUIDv5(calendar_uid, object_name)`. Everything else on the calendar declaration (name, title, color, etc.) is metadata. The spec needs to formalize the accepted fields and the UUIDv5 algorithm. All events and tasks belong to a single calendar object (analogous to JSCalendar's Group members).

---

## III. Internal Inconsistencies and Phantom Types

### 9. "Local datetime" — used everywhere, defined nowhere (spec gap)

The term "local datetime" appears in requirements for events (`start`), tasks (`due`, `start`), alerts (`trigger.at`), and recurrence rules (`termination`), but the spec never defines it. The concept comes from iCalendar/JSCalendar, which distinguish UTC datetimes (with `Z` suffix) from local/floating datetimes (without). A local datetime's timezone is determined by context — the `time_zone` field on the enclosing object or the calendar-level default. Gnomon datetime literals are always local (the lexer does not accept a `Z` suffix), so this just needs to be stated explicitly in the spec. The `local` weak keyword's role (if any) in relation to this concept should also be clarified.

### 10. `undefined` is a strict keyword but not a valid expression (spec + impl bug)

`undefined` is listed as a strict keyword and used in recurrence rule termination, but it's not listed in the literal expression grammar (`r[expr.literal.syntax+2]`). The implementation confirms: `lower_literal` has no arm for `UNDEFINED_KW`. This is simply an omission — `undefined` should be a valid expression, serving as a way to remove fields from records (analogous to clearing a field without needing imperative `delete` syntax). Needs to be added to the expression grammar in the spec and handled in lowering.

### 11. Recurrence rules have no attachment point (spec gap)

Rrules are a fully specified record type, and `every` expressions desugar into them, but no field on events or tasks accepts one. Events and tasks need a `recur` field whose value is a recurrence rule. This follows JSCalendar's model of a single recurrence rule per object (iCalendar allows multiple, but you can always compute a single union rule). Needs to be added to the common record fields or the event/task record type definitions.

### 12. Names in expressions vs. declarations (spec + impl bug)

Names (`@foo`) are defined lexically and used in short-form declarations, but cannot appear as expression values — the `expr` grammar has no `name` production. This means the prefix form `event { name: @meeting }` is unparseable. Simply a missing production in the expression grammar.

### 13. Orphaned keywords: `override` and `local`

Both are listed as weak keywords with no grammar production or semantic rule. `local` relates to the "local datetime" concept (#9) — its role just needs to be clarified or dropped if datetimes are always local. `override` came from an earlier brainstorming session as a way to alter record fields, but is superseded by the broader evaluation semantics direction (#3) — record mutation will need a more robust design than a dedicated keyword. Both can be revisited once evaluation semantics take shape.

---

## IV. Implementation Divergences from Spec

### 14. `group` declarations — should be removed

The spec defines groups (`r[record.group.*]`) and dead code exists in the implementation (`ReifiedDecl::Group`, `DeclKind::Group`, merge handling), but the parser never implemented them. Groups were an over-eager encoding of the JSCalendar data model. They should be removed from the spec and the dead code cleaned up from the implementation.

### 15. `check`, `eval`, `merge` subcommands need specification

`check` is currently reserved by `r[cli.subcommand.reserved]` but is already implemented. Along with `eval` and `merge` (#16), all in-use CLI subcommands should be properly specified in the spec and removed from the reserved list as appropriate.

### 16. (folded into #15)

---

## V. Well-Defined Areas Deserving Review

### 17. (folded into #5)

### 18. The `every` expression `by_day` desugaring (spec + impl bug)

`r[record.rrule.every.desugar.subject.weekday]` says to set `day` to "the index of the given weekday (starting from `1` for `monday`)" — an integer — but the rrule type definition says `day` is a weekday keyword. The weekday keyword is correct. The spec and implementation both need to be updated to use the keyword form instead of the integer index.

### 19. Common Record Fields — Scope Ambiguity (spec gap)

The spec defines 15+ common fields with type constraints but never states which record types those constraints apply to. The answer: unless specified otherwise, the constraints apply to events and tasks. Importantly, gnomon records are open — any field can appear on any record. The common fields section doesn't define what's *allowed*, it defines type restrictions for known fields on known record types. This just needs to be stated explicitly in the spec.

### 20. Date Validation — Calendrical vs. Lexical (spec + impl gap)

The lexer accepts `2024-02-30` — validation checks `month in 1..=12` and `day in 1..=31` but does no calendrical validation. Calendrically invalid dates (Feb 30, Apr 31, etc.) should be errors because validation is cheap. By contrast, leap second validation (second = 60 when no leap second actually occurred) should *not* be an error because it is expensive and annoying to verify. The spec should state this distinction and the implementation should add calendrical date checks.

---

## Prioritized Recommendations

**Immediate (straightforward spec/impl fixes):**
1. Define "local datetime" in the spec (#9)
2. Specify short-form declaration desugaring (#7)
3. Add `undefined` to the expression grammar (#10)
4. Add names to the expression grammar (#12)
5. Add `recur` field to events and tasks (#11)
6. Fix `every` `by_day` desugaring to use weekday keywords (#18)
7. Remove `group` from spec and dead code from implementation (#14)
8. Add calendrical date validation (#20)
9. State common field scope explicitly (#19)
10. Specify `check`, `eval`, `merge` CLI subcommands (#15)

**Near-term (requires design work):**
11. Specify the Gnomon data model / calendar object (#1)
12. Specify calendar declaration fields and UUIDv5 derivation (#8)
13. Design the reification pass (untyped records → typed domain objects) (#2)

**Longer-term (depends on evaluation semantics):**
14. Design evaluation semantics for composition/merge (#3)
15. Design binding semantics and data model placement (#5)
16. Design include/import resolution and keyword split (#6)
17. Specify recurrence rule evaluation (follows from #13, per RFC 5545) (#2)
18. Design query system (#4)
19. Clarify or remove `override` and `local` keywords (#13)
