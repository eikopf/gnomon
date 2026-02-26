# Sundial Specification

A plaintext language for authoring and maintaining calendars, designed to compile to iCalendar and JSCalendar.

## Principles

r[principle.records]
Every structured value MUST be a record. Familiar literal forms (dates, times, durations) are syntactic sugar over records.

r[principle.layers]
The language MUST have two syntactic layers: a full syntax where every piece of data is an explicit record field, and a short syntax that desugars to the full syntax.

r[principle.identity]
Concrete UIDs MUST NOT be stored in the data model. Component identity MUST be a function from a root UUID to a derived UUID, evaluated only at render time.

r[principle.unknown-fields]
Unknown fields MUST be allowed by default and MUST participate fully in the data model.

## File Structure

### Directory Layout

r[file.layout.directory]
A calendar MUST be a directory containing exactly one calendar file and any number of item files.

r[file.layout.extension]
All `.cal` files in the directory MUST be assembled by the compiler. Membership is by co-location — no explicit imports.

r[file.layout.isolation]
The language MUST operate on one calendar at a time. Orchestration across calendars is a tool-level concern.

### Calendar File

r[file.calendar.uniqueness]
There MUST be exactly one calendar file per calendar directory.

r[file.calendar.syntax]
The calendar file MUST use the `calendar` keyword followed by a string title and a record containing at least `id` and `tz` fields.

```
calendar "oliver/personal" {
  id: 6ba7b810-9dad-11d1-80b4-00c04fd430c8,
  tz: Europe/Zurich,
}
```

### Item Files

r[file.item.components]
Item files MUST contain components only and MUST inherit configuration from the calendar file.

r[file.item.defaults]
Item files MAY declare a `defaults` block to override calendar-level defaults for all components in that file.

```
defaults {
  tz: America/New_York,
}

event 2025-08-01 09:00 2h "Museum visit"
event 2025-08-02 19:00 3h "Dinner"
```

### Assembled Output

r[file.assembled]
The assembled output of all files MUST be a single record with configuration as named fields and all components in a single `items` list discriminated by a `kind` field.

## Syntax

### Comments

r[syntax.comment]
Comments MUST begin with `--` and extend to the end of the line.

### Identifiers

r[syntax.ident]
Identifiers MUST match `[a-zA-Z_][a-zA-Z0-9_-]*`. Hyphens MUST be allowed to support iCal extension names like `x-custom-field` without quoting.

### Names

r[syntax.name]
Names MUST be prefixed with `@` followed by an identifier. Names are scoped to the calendar, not the file.

r[syntax.name.unique]
Duplicate names across files within the same calendar MUST be a compile error.

r[syntax.name.unresolved]
Unresolvable name references MUST be a compile error.

### Component Short Syntax

r[syntax.short.event]
The event short syntax MUST follow the form: `event` Name? (RecurPat | DateLit) TimeLit? DurationLit? String Alarm* Record?

r[syntax.short.todo]
The todo short syntax MUST follow the form: `todo` Name? DateLit? String Record?

r[syntax.short.name-position]
A name, if present, MUST appear immediately after the keyword.

r[syntax.short.duplicate-field]
A field provided both positionally and in the braced block MUST be an error.

```
event @standup every Monday 09:00 1h "Standup" !15m { tag: work }
event @deep-work 2025-07-14 14:00-16:00 "Deep work block"
todo @invoice 2025-07-15 "Submit invoice" { priority: high }
todo "Read RFC 7986" { tag: reading }
```

### Component Full Syntax

r[syntax.full]
The full syntax MUST express every field by name inside a record. No positional semantics apply.

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

r[syntax.override.short]
The override short syntax MUST follow the form: `override` Name DateLit Record.

r[syntax.override.nesting]
Overrides MUST be nested under their parent component after resolution.

r[syntax.override.cross-file]
Cross-file override references MUST be valid — an override in one file MAY reference a name defined in another file within the same calendar.

```
override @standup 2025-12-15 { start: { hour: 10 } }
override @standup 2025-12-22 { cancelled: true }
```

### Alarms

r[syntax.alarm.short]
Alarm short syntax MUST use the `!` sigil after the title. Each `!` token followed by a duration MUST produce an alarm.

r[syntax.alarm.short-type]
Short-form alarms MUST support only `before` + `display` type.

r[syntax.alarm.full]
For other alarm types, the braced block `alarms` list MUST be used.

```
event @meeting 2025-07-14 14:00 2h "Review" !1h !5m
event @meeting 2025-07-14 14:00 2h "Review" {
  alarms: [
    { before: 1h, type: display },
    { before: 5m, type: email },
  ],
}
```

### Recurrence

#### Short Syntax

r[syntax.recur.every]
The `every` prefix MUST replace the date position in an event or todo to indicate recurrence.

r[syntax.recur.every-grammar]
The `every` mini-grammar MUST support the following mappings:

| Pattern | Desugared |
|---------|-----------|
| `every Monday` | `{ freq: weekly, day: Monday }` |
| `every 2 weeks Monday` | `{ freq: biweekly, day: Monday }` |
| `every 2nd Wednesday` | `{ freq: monthly, ord: 2, day: Wednesday }` |
| `every day` | `{ freq: daily }` |
| `every year 03-15` | `{ freq: yearly, date: { month: 3, day: 15 } }` |

Anything that does not fit this grammar MUST use the full form.

#### Full Syntax

r[syntax.recur.full]
Recurrence MUST be expressible as a `recurs` record with the following fields:

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `freq` | enum | yes | `daily`, `weekly`, `biweekly`, `monthly`, `yearly` |
| `day` | weekday or list | depends on freq | required for weekly/biweekly/monthly |
| `ord` | ordinal | no | e.g. `ord: 2, day: Wednesday` |
| `from` | date | no | defaults to first matching date |
| `until` | date | no | open-ended if omitted |
| `except` | date list | no | |
| `count` | int | no | alternative to `until` |

r[syntax.recur.count-until]
`count` and `until` MUST be mutually exclusive alternatives for bounding recurrence. Both MAY be omitted for open-ended recurrence.

### Dot-Path Access

r[syntax.dot-path]
Nested fields MAY be set using dot-path syntax (e.g., `recurs.from: 2025-07-01`) as an alternative to a full nested block. Both forms MUST be valid.

## Value Types

### Primitives

r[type.primitives]
The language MUST support these true primitive types: `Int`, `String`, `Ident`, `Bool`, and `undefined`. Everything else is a record with syntactic sugar.

| Type | Examples |
|------|----------|
| `Int` | `42`, `0`, `15` |
| `String` | `"Room 3"`, `"hello\nworld"` |
| `Ident` | `work`, `opaque`, `high` |
| `Bool` | `true`, `false` |
| `undefined` | `undefined` |

### Strings

r[type.string.syntax]
Strings MUST be quoted with double quotes and MUST support escape sequences: `\"`, `\\`, `\n`. No multiline literals.

r[type.string.bare-ident]
Bare identifiers MUST be permitted where unambiguous: enum-like fields, tags, and `@names`.

### Records

r[type.record.syntax]
Records MUST use braces with comma-separated fields. Trailing commas MUST be allowed. Newlines MAY substitute for commas.

r[type.record.field]
Record fields MUST follow the pattern `Path ':' '='? Value`, where Path is a dot-separated identifier chain.

### Lists

r[type.list.syntax]
Lists MUST use square brackets with comma-separated values. Single-element lists MUST require brackets — no sugar for single-element lists.

### Temporal Types

r[type.date]
Date literals (`YYYY-MM-DD` or `MM-DD`) MUST desugar to records with `year`, `month`, `day` integer fields. Omitted fields default to `0` (or `undefined` for partial dates like month-day pairs in yearly recurrences).

r[type.time]
Time literals (`HH:MM` or `HH:MM:SS`) MUST desugar to records with `hour`, `minute`, `second` integer fields.

r[type.datetime]
DateTime literals (`YYYY-MM-DDTHH:MM`) MUST desugar to records with `date`, `time`, and `tz` fields.

r[type.duration]
Duration literals (e.g., `1h30m`, `2d`) MUST desugar to records with `weeks`, `days`, `hours`, `minutes`, `seconds` integer fields. No spaces between components.

### Timezone Semantics

r[type.tz.default]
The default value for the `tz` field MUST be `local` (floating time).

r[type.tz.cascade]
A calendar-level or file-level `tz` MUST set the default for contained components. A component MAY opt back into floating time with explicit `tz: local`.

### Enums

r[type.enum]
Enums MUST NOT be a separate primitive type. They MUST be bare identifiers in specific field contexts, validated by the parser:

- `freq`: `daily` | `weekly` | `biweekly` | `monthly` | `yearly`
- `priority`: `low` | `normal` | `high`
- `day`: `Monday` | `Tuesday` | `Wednesday` | `Thursday` | `Friday` | `Saturday` | `Sunday`
- `type`: `display` | `email` | `audio`

### Value Parsing Priority

r[type.parse-priority]
For unknown fields, the parser MUST interpret values in this order:

1. Keyword (`true`, `false`, `undefined`, `local`, weekday names, freq names)
2. Date literal (`2025-07-14`, `07-14`)
3. Time literal (`09:00`, `09:00:30`)
4. Duration literal (`1h30m`, `2d`)
5. Integer (`42`)
6. Bare identifier (`opaque`, `some-value`)
7. Quoted string (`"anything goes"`)

r[type.parse-known]
Known fields MUST validate against their expected type.

## Identity Model

### UUIDv5 Derivation

r[identity.named]
Named components MUST derive their UID as `UUIDv5(calendar_root_uuid, name_string)`.

r[identity.unnamed]
Unnamed components MUST derive their UID from a canonical content signature: `UUIDv5(calendar_root_uuid, "kind|date|time|title")`.

r[identity.sub-component]
Sub-component and override identity MUST derive from the parent's UID:

- Override: `UUIDv5(event_uid, "YYYY-MM-DD")`
- Alarm: `UUIDv5(event_uid, "alarm|duration|before")`

### Renaming

r[identity.alias]
When a component has an `alias` field, the compiler MUST derive the UID from the alias name instead of the current name, preserving identity continuity across renames.

```
event @daily-sync every Monday 09:00 1h "Daily Sync" {
  alias: @standup,
}
```

### Root UUID

r[identity.root]
The calendar's root UUID MUST be declared in the calendar file. It MUST be a UUIDv4, generated once on calendar initialization. It is configuration for the rendering backend, not intrinsic to the data model.

## Override Semantics

r[override.merge]
By default, overrides MUST deep-merge into the base component record recursively.

r[override.replace]
Prefixing a value with `=` MUST replace the entire field rather than merging.

r[override.unset]
Setting a field to `undefined` MUST explicitly remove that field from the instance.

r[override.omit]
Omitting a field in an override MUST mean "keep the original value."

r[override.list]
List fields MUST be replaced by default — no merge semantics apply.

| Syntax | Semantics |
|--------|-----------|
| `field: value` | Deep merge (records), replace (scalars/lists) |
| `field: = value` | Replace entire field |
| `field: undefined` | Unset field |

## Unknown Fields

r[unknown.preserve]
Unknown fields MUST be preserved in the data model. The identifier grammar accommodates iCal extension names (`x-custom-field`) without quoting.

r[unknown.render]
If the rendering backend can map an unknown field to an iCal/JSCalendar property, it MUST do so. Otherwise, it MUST emit the field as an `X-` property (iCal) or custom field (JSCalendar).

## Compilation Pipeline

r[compile.parse]
Each file in the calendar directory MUST be parsed independently.

r[compile.merge]
After parsing, calendar config and all item lists MUST be merged into a single structure.

r[compile.defaults]
Defaults MUST be applied in order: calendar-level first, then file-level overrides.

r[compile.resolve]
Name references MUST be resolved and overrides MUST be nested under their parent components.

r[compile.output]
The result of compilation MUST be a single Calendar record with no concrete UUIDs.

r[compile.render]
At render time, the root UUID MUST be supplied and all component IDs MUST be resolved before emitting iCalendar or JSCalendar output.

## Grammar

r[grammar.file]
A file MUST consist of zero or more of: Calendar, Defaults, Component, Override, or Comment declarations.

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

RecurPat   ::= 'every' (...)
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
