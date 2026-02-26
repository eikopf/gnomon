# Calendar Language Design

A plaintext language for authoring and maintaining calendars, designed to compile to iCalendar and JSCalendar.

## Principles

- Every structured value is a record. Familiar literal forms (dates, times, durations) are syntactic sugar over records.
- The language has two syntactic layers: a **full syntax** where every piece of data is an explicit record field, and a **short syntax** that desugars to the full syntax.
- Concrete UIDs are never stored in the data model. Component identity is a function from a root UUID to a derived UUID, evaluated only at render time.
- Unknown fields are allowed by default and participate fully in the data model.

## File Structure

### Directory layout

A calendar is a directory. It contains exactly one **calendar file** declaring configuration, and any number of **item files** containing components.

```
personal/
  calendar.cal          -- calendar record (config, id)
  recurring.cal         -- recurring events
  2025-07.cal           -- one-off events for July
  todos.cal             -- open todos
```

All `.cal` files in the directory are assembled by the compiler. No explicit imports — membership is by co-location.

Multiple calendars are separate directories. The language operates on one calendar at a time; orchestration across calendars is a tool-level concern.

### Calendar file

Exactly one per calendar. Declares configuration and render-time identity.

```
calendar "oliver/personal" {
  id: 6ba7b810-9dad-11d1-80b4-00c04fd430c8,
  tz: Europe/Zurich,
}
```

### Item files

Contain components only. Inherit configuration from the calendar file. May declare file-level default overrides:

```
defaults {
  tz: America/New_York,
}

event 2025-08-01 09:00 2h "Museum visit"
event 2025-08-02 19:00 3h "Dinner"
```

### Desugared top-level form

The assembled output is a single record:

```
{
  title: "oliver/personal",
  tz: Europe/Zurich,
  items: [
    { kind: event, name: @standup, ... },
    { kind: todo, name: @invoice, ... },
  ],
}
```

Configuration lives as named fields. Components live in a single `items` list discriminated by a `kind` field. Overrides are nested under their parent component after resolution.

## Syntax

### Comments

```
-- This is a comment
```

### Identifiers

```
Ident ::= [a-zA-Z_] [a-zA-Z0-9_-]*
```

Hyphens are allowed, enabling iCal extension names like `x-custom-field` without quoting.

### Names

Local reference handles, scoped to the calendar (not the file). Used for overrides and cross-references. Not UIDs.

```
@standup
@deep-work
@invoice
```

### Component short syntax

Positional fields followed by an optional braced block for additional properties.

```
event <date> [<time> [<duration>]] <title> [<alarm>...] [{ ... }]
todo  [<date>] <title> [{ ... }]
```

| Keyword | Position 1 | Position 2 | Position 3 | Quoted string |
|---------|-----------|-----------|-----------|--------------|
| `event` | `date`   | `start`  | `duration`| `title`      |
| `todo`  | `due`    | —        | —         | `title`      |

A name, if present, appears immediately after the keyword. The `every` prefix replaces the date position with a recurrence pattern.

```
event @standup every Monday 09:00 1h "Standup" !15m { tag: work }
event @deep-work 2025-07-14 14:00-16:00 "Deep work block"
todo @invoice 2025-07-15 "Submit invoice" { priority: high }
todo "Read RFC 7986" { tag: reading }
```

Any field provided both positionally and in the braced block is an error.

### Component full syntax

Every field is named. No positional semantics.

```
event {
  name: @standup,
  title: "Standup",
  start: { hour: 9, minute: 0 },
  duration: { hours: 1 },
  recurs: {
    freq: weekly,
    day: Monday,
    from: { year: 2025, month: 7, day: 1 },
  },
  alarms: [{ before: { minutes: 15 }, type: display }],
  tag: work,
}
```

### Overrides

Short syntax:

```
override @standup 2025-12-15 { start: { hour: 10 } }
override @standup 2025-12-22 { cancelled: true }
```

Desugared, overrides are nested under their parent component:

```
{
  kind: event,
  name: @standup,
  ...,
  overrides: [
    { instance: { year: 2025, month: 12, day: 15 }, start: { hour: 10 } },
    { instance: { year: 2025, month: 12, day: 22 }, cancelled: true },
  ],
}
```

Cross-file references are valid — an override in `2025-12.cal` can reference `@standup` defined in `recurring.cal`. Unresolvable names are a compile error. Duplicate names across files are a compile error.

### Alarms

Short syntax uses the `!` sigil after the title:

```
event @meeting 2025-07-14 14:00 2h "Review" !1h !5m
```

Each `!` token produces an alarm. Short form supports only `before` + `display`. For other alarm types, use the braced block:

```
event @meeting 2025-07-14 14:00 2h "Review" {
  alarms: [
    { before: 1h, type: display },
    { before: 5m, type: email },
  ],
}
```

### Recurrence

#### Short syntax

The `every` prefix replaces the date position in an event or todo:

```
event @standup every Monday 09:00 1h "Standup" {
  recurs.from: 2025-07-01,
  recurs.except: [2025-12-22],
}
```

The `every` mini-grammar maps to record fields:

```
every Monday         → { freq: weekly, day: Monday }
every 2 weeks Monday → { freq: biweekly, day: Monday }
every 2nd Wednesday  → { freq: monthly, ord: 2, day: Wednesday }
every day            → { freq: daily }
every year 03-15     → { freq: yearly, date: { month: 3, day: 15 } }
```

Anything that doesn't fit this grammar must use the full form.

#### Full syntax

```
recurs: {
  freq: weekly,
  day: Monday,
  from: 2025-07-01,
  until: 2025-12-31,
  except: [2025-12-22, 2025-12-29],
}
```

#### Recurrence record fields

| Field   | Type              | Required       | Notes                                    |
|---------|-------------------|----------------|------------------------------------------|
| `freq`  | enum              | yes            | `daily`, `weekly`, `biweekly`, `monthly`, `yearly` |
| `day`   | weekday or list   | depends on freq| required for weekly/biweekly/monthly     |
| `ord`   | ordinal           | no             | "2nd Wednesday" → `ord: 2, day: Wednesday` |
| `from`  | date              | no             | defaults to first matching date          |
| `until` | date              | no             | open-ended if omitted                    |
| `except`| date list         | no             |                                          |
| `count` | int               | no             | alternative to `until`, as in RRULE      |

#### Dot-path access

Nested fields can be set without a full nested block:

```
event every Monday 09:00 1h "Standup" {
  recurs.from: 2025-07-01,
  recurs.except: [2025-12-22],
}
```

This avoids forcing a nested block to set one or two sub-fields. Both forms (dot-path and nested block) are valid.

## Value Types

### Primitives

The true primitives are:

| Type     | Examples                      |
|----------|-------------------------------|
| `Int`    | `42`, `0`, `15`               |
| `String` | `"Room 3"`, `"hello\nworld"` |
| `Ident`  | `work`, `opaque`, `high`      |
| `Bool`   | `true`, `false`               |
| `undefined` | `undefined`                |

Everything else is a record with syntactic sugar.

### Strings

Quoted with double quotes. Required when the value contains spaces or special characters. Escape sequences: `\"`, `\\`, `\n`. No multiline literals.

Bare identifiers are permitted where unambiguous: enum-like fields (`priority: high`), tags (`tag: work`), and `@names`.

### Records

```
Record ::= '{' (Field (',' Field)* ','?)? '}'
Field  ::= Path ':' '='? Value
Path   ::= Ident ('.' Ident)*
```

Trailing commas are allowed but not required. Newlines may substitute for commas.

### Lists

```
List ::= '[' (Value (',' Value)* ','?)? ']'
```

Square brackets, comma-separated. Single-element lists require brackets. List fields always use bracket syntax — no sugar for single-element lists.

### Temporal types as records

All temporal types desugar to records with integer fields.

#### Date

```
2025-07-14    → { year: 2025, month: 7, day: 14 }
07-14         → { month: 7, day: 14 }
```

Schema: `{ year: Int, month: Int, day: Int }`. Omitted fields default to `0` (or `undefined` for partial dates like month-day pairs in yearly recurrences).

#### Time

```
09:00         → { hour: 9, minute: 0 }
09:00:30      → { hour: 9, minute: 0, second: 30 }
```

Schema: `{ hour: Int, minute: Int, second: Int }`.

#### DateTime

Rarely needed — date and time are usually separate fields. Available as `2025-07-14T09:00` when both must be atomic.

Schema: `{ date: Date, time: Time, tz: String }`.

#### Duration

```
1h            → { hours: 1 }
30m           → { minutes: 30 }
1h30m         → { hours: 1, minutes: 30 }
2d            → { days: 2 }
1w            → { weeks: 1 }
```

Schema: `{ weeks: Int, days: Int, hours: Int, minutes: Int, seconds: Int }`. No spaces between components.

### Timezone semantics

```
tz: local          -- floating, follows the viewer (default)
tz: Europe/Zurich  -- pinned to a specific timezone
```

The default for the `tz` field is `local` (floating). A calendar-level or file-level `tz` sets the default for contained components. A component can opt back into floating with an explicit `tz: local`.

### Enums

Not a separate primitive. Bare identifiers in specific field contexts, validated by the parser:

```
freq:     daily | weekly | biweekly | monthly | yearly
priority: low | normal | high
day:      Monday | Tuesday | Wednesday | Thursday | Friday | Saturday | Sunday
type:     display | email | audio
```

### Value parsing priority

For unknown fields, the parser interprets values in this order:

1. Keyword (`true`, `false`, `undefined`, `local`, weekday names, freq names)
2. Date literal (`2025-07-14`, `07-14`)
3. Time literal (`09:00`, `09:00:30`)
4. Duration literal (`1h30m`, `2d`)
5. Integer (`42`)
6. Bare identifier (`opaque`, `some-value`)
7. Quoted string (`"anything goes"`)

Known fields validate against their expected type. Unknown fields take whatever the value parses as and are preserved in the data model.

## Identity Model

### UUIDv5 derivation

Components do not store concrete UUIDs. Identity is a function from a root UUID to a derived UUID, evaluated only when rendering to iCalendar or JSCalendar.

Named components derive identity from their `@name`:

```
event UID = UUIDv5(calendar_root_uuid, "standup")
```

Unnamed components derive identity from a canonical content signature:

```
event UID = UUIDv5(calendar_root_uuid, "event|2025-07-14|09:00|Standup")
```

Sub-component and override identity derives from the parent:

```
override UID  = UUIDv5(event_uid, "2025-12-15")
alarm UID     = UUIDv5(event_uid, "alarm|15m|before")
```

### Implementation

The `id` is not a closure at runtime, but a stored hash input with deferred evaluation:

```rust
enum ComponentId {
    Named(String),
    ContentDerived(String),
}

impl ComponentId {
    fn resolve(&self, root: Uuid) -> Uuid {
        match self {
            Self::Named(name) => Uuid::new_v5(&root, name.as_bytes()),
            Self::ContentDerived(sig) => Uuid::new_v5(&root, sig.as_bytes()),
        }
    }
}
```

### Renaming

Renaming a `@name` changes the derived UID. An alias preserves continuity:

```
event @daily-sync every Monday 09:00 1h "Daily Sync" {
  alias: @standup,
}
```

The compiler derives the UID from `@standup` instead of `@daily-sync`.

### Root UUID

The calendar's root UUID is declared in `calendar.cal`. It is configuration for the rendering backend, not intrinsic to the data model. A UUIDv4, generated once on calendar initialisation.

## Override Semantics

### Deep merge by default

An override merges into the base component record recursively:

```
-- Base: start is { hour: 9, minute: 0, second: 0 }
override @standup 2025-12-15 { start: { hour: 10 } }
-- Result: start is { hour: 10, minute: 0, second: 0 }
```

### Explicit replace

Prefix the value with `=` to replace the entire field rather than merging:

```
override @standup 2025-12-15 { start: = { hour: 10 } }
-- Result: start is { hour: 10 } (minute and second are default/0)
```

### Unset

Use `undefined` to explicitly remove a field:

```
override @standup 2025-12-15 { location: undefined }
```

Omitting a field in an override means "keep the original." Setting it to `undefined` means "this instance has no value for this field."

### List fields

Lists are replaced by default (no merge key), making `=` redundant for list fields.

### Summary

| Syntax                  | Semantics                           |
|-------------------------|-------------------------------------|
| `field: value`          | Deep merge (records), replace (scalars/lists) |
| `field: = value`        | Replace entire field                |
| `field: undefined`      | Unset field                         |

## Unknown Fields

Unknown fields are allowed and preserved in the data model. The identifier grammar accommodates iCal extension names (`x-custom-field`) without quoting:

```
event @meeting 2025-07-14 14:00 2h "Review" {
  x-custom-field: some-value,
  transp: opaque,
}
```

If the rendering backend can map an unknown field to an iCal/JSCalendar property, it does. Otherwise it is emitted as an `X-` property (iCal) or custom field (JSCalendar).

## Compilation Pipeline

```
files in directory
       │
       ▼
parse each file independently
       │
       ▼
merge: calendar config + concat all item lists
       │
       ▼
apply defaults (calendar-level, then file-level overrides)
       │
       ▼
resolve @name references, nest overrides under parents
       │
       ▼
single Calendar record (no concrete UUIDs)
       │
       ▼
supply root UUID, resolve all component IDs
       │
       ▼
emit iCalendar / JSCalendar
```

## Grammar Summary

```
File       ::= (Calendar | Defaults | Component | Override | Comment)*
Comment    ::= '--' .*

Calendar   ::= 'calendar' String Record
Defaults   ::= 'defaults' Record

Component  ::= EventShort | EventFull | TodoShort | TodoFull
Override   ::= 'override' Name DateLit Record

EventShort ::= 'event' Name? (RecurPat | DateLit) TimeLit? DurationLit? String Alarm* Record?
EventFull  ::= 'event' Record
TodoShort  ::= 'todo' Name? DateLit? String Record?
TodoFull   ::= 'todo' Record

RecurPat   ::= 'every' (...)   -- see recurrence mini-grammar
Alarm      ::= '!' DurationLit

Name       ::= '@' Ident
Ident      ::= [a-zA-Z_] [a-zA-Z0-9_-]*
Path       ::= Ident ('.' Ident)*

Value      ::= Keyword | DateLit | TimeLit | DurationLit
             | Int | Ident | String
             | Record | List
Record     ::= '{' (Field (',' Field)* ','?)? '}'
Field      ::= Path ':' '='? Value
List       ::= '[' (Value (',' Value)* ','?)? ']'

DateLit    ::= YYYY-MM-DD | MM-DD
TimeLit    ::= HH:MM | HH:MM:SS
DurationLit::= (<int>w | <int>d | <int>h | <int>m | <int>s)+
Int        ::= [0-9]+
String     ::= '"' ... '"'
Keyword    ::= 'true' | 'false' | 'undefined' | 'local' | ...
```
