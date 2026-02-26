# Sundial Specification

A plaintext language for authoring and maintaining calendars, designed to compile to iCalendar and JSCalendar.

## Introduction

TODO: write prose introduction

## Principles

r[principle.records]
Every structured value MUST be a record. Familiar literal forms (dates, times, durations) are syntactic sugar over records.

r[principle.layers]
The language MUST have two syntactic layers: a full syntax where every piece of data is an explicit record field, and a short syntax that desugars to the full syntax.

r[principle.identity]
Concrete UIDs MUST NOT be stored in the data model. Component identity MUST be a function from a root UUID to a derived UUID, evaluated only at render time.

r[principle.unknown-fields]
Unknown fields MUST be allowed by default and MUST participate fully in the data model.

## Lexical Syntax

What follows here is syntax used by the lexer, which converts UTF-8 strings into sequences of tokens. Much of this is modelled on [section 2 of the Rust Reference](https://doc.rust-lang.org/stable/reference/lexical-structure.html).

### Input Format

For the purposes of lexical syntax, a character is a Unicode scalar value. We also single out the null character for special handling.

```ebnf
char = ? a Unicode scalar value ? ;
nul  = ? U+0000 ? ;
```

All source data is interpreted as a sequence of characters encoded in UTF-8. It is an error if the file is not valid UTF-8.

r[lexer.input-format.utf-8]
All source data MUST be interpreted as a sequence of characters encoded in UTF-8.

r[lexer.input-format.malformed]
An error MUST be produced if any source data is not valid UTF-8.

Before the source data can be processed by a lexer, the following rules are applied in order:
1. If the first character in the data is U+FEFF (BYTE ORDER MARK), it is removed.
2. Each CRLF sequence (U+000D followed by U+000A) is replaced by a single U+000A.
3. If the remaining sequence begins with the characters `#!`, the first line is removed.

r[lexer.input-format.bom-removal]
If the source data begins with U+FEFF, it MUST be removed.

r[lexer.input-format.crlf-normalization]
Each CRLF sequence in the source data MUST be replaced by U+000A.

r[lexer.input-format.shebang-removal]
If the source data begins with `#!`, the first line MUST be removed.

r[lexer.input-format.rule-order]
The input format normalization rules MUST be applied in order: the byte order mark removal precedes CRLF normalization precedes shebang removal.

### Comments

Sundial uses Lisp-style semicolon comments for no reason other than that the semicolon is an otherwise unused character.

r[lexer.comment]
Comments MUST begin with `;` and extend to the end of the line.

```ebnf
comment  = ";", { any char - newline }, newline ;
newline  = ? U+000A ? ;
any char = ? any Unicode scalar value ? ;
```

### Whitespace

A whitespace character is any character with the `Pattern_White_Space` Unicode property, and a whitespace string is any non-empty string consisting only of whitespace characters.

r[lexer.whitespace]
Any character with the `Pattern_White_Space` Unicode property MUST be treated as whitespace.

Whitespace does not have any semantic significance, and replacing any whitespace string with any other whitespace string does not change the meaning of a Sundial program.

### Punctuation

r[lexer.punctuation]
Any character matching the regex `[\{\}\[\]:,=!\.\-]` MUST be recognized as punctuation.

| Token | Name |
|-------|------|
| `{` | Left brace |
| `}` | Right brace |
| `[` | Left bracket |
| `]` | Right bracket |
| `:` | Colon |
| `,` | Comma |
| `=` | Equals |
| `!` | Bang |
| `.` | Dot |
| `-` | Hyphen |


### Identifiers

r[lexer.ident]
Identifiers MUST match the regex `[a-zA-Z_][a-zA-Z0-9_-]*`.

```ebnf
identifier = identifier start, { identifier continuation } ;
identifier start = letter | "_" ;
identifier continuation = letter | digit | "_" | "-" ;
letter   = "a" | "b" | (* ... *) "z"
         | "A" | "B" | (* ... *) "Z" ;
digit    = "0" | "1" | "2" | "3" | "4"
         | "5" | "6" | "7" | "8" | "9" ;
```

Note that hyphens are allowed in order to support iCalendar extension names like `x-custom-field` without quoting.

### Keywords

Sundial distinguishes between strict and weak keywords. A strict keyword may only be used as keyword, whereas a weak keyword decays into an identifier outside of the designated contexts in which it operates as a keyword.

The strict keywords are `true`, `false`, and `undefined`.

r[lexer.keyword.strict]
The keywords `true`, `false`, and `undefined` MUST be treated as strict.

All other keywords are weak. These keywords are:
- `calendar`
- `include`
- `bind`
- `override`
- `event`
- `todo`
- `every`
- `day`
- `weeks`
- `year`
- `st`, `nd`, `rd`, `th` (ordinal suffixes)
- `monday`, `tuesday`, `wednesday`, `thursday`, `friday`, `saturday`, `sunday`
- `local`

r[lexer.keyword.weak]
All keywords other than `true`, `false`, and `undefined` MUST be treated as weak.

### Names

A name is a non-empty sequence of identifiers used to refer to an object.

r[syntax.name]
Names MUST be prefixed with `@` followed by a non-empty period-delimited sequence of identifiers.

```ebnf
name = "@", identifier, { ".", identifier } ;
```

### Integer Literals

An integer literal represents a non-negative integer not exceeding `u64::MAX`.

r[lexer.integer]
An integer literal MUST be a non-empty sequence of ASCII digits.

r[lexer.integer.max]
It is an error for an integer literal to have a value exceeding `u64::MAX`.

```ebnf
integer literal = digit, { digit } ;
```

### String Literals

r[lexer.string]
A string literal MUST be delimited by double-quote characters (`"`).

r[lexer.string.escape]
The following escape sequences MUST be recognized within a string literal: `\"` (double quote), `\\` (backslash), `\n` (newline), and `\t` (tab).

r[lexer.string.no-multiline]
String literals MUST NOT span multiple lines. An unescaped newline within a string literal is an error.

```ebnf
string      = '"', { string char }, '"' ;
string char = any char - ( '"' | "\\" | newline )
            | "\\", escape char ;
escape char = '"' | "\\" | "n" | "t" ;
```

### Date Literals

r[lexer.date.full]
A full date literal MUST have the form `YYYY-MM-DD`, where each component is a fixed-width decimal number (4 digits for year, 2 digits for month and day).

r[lexer.date.month-day]
A month-day literal MUST have the form `MM-DD`, where each component is a 2-digit decimal number.

```ebnf
date literal      = full date | month day literal ;
full date         = year, "-", month, "-", day ;
month day literal = month, "-", day ;

year  = digit, digit, digit, digit ;
month = digit, digit ;   (* 01..=12 *)
day   = digit, digit ;   (* 01..=31 *)
```

### Time Literals

r[lexer.time]
A time literal MUST have the form `HH:MM` or `HH:MM:SS`, where each component is a 2-digit decimal number.

```ebnf
time literal = hour, ":", minute, [ ":", second ] ;

hour   = digit, digit ;   (* 00..=23 *)
minute = digit, digit ;   (* 00..=59 *)
second = digit, digit ;   (* 00..=60, allowing leap second *)
```

### DateTime Literals

r[lexer.datetime]
A datetime literal MUST have the form `YYYY-MM-DDTHH:MM` or `YYYY-MM-DDTHH:MM:SS`, joining a full date and a time literal with the character `T`.

```ebnf
datetime literal = full date, "T", time literal ;
```

### Duration Literals

r[lexer.duration]
A duration literal MUST consist of one or more duration parts with no intervening whitespace.

r[lexer.duration.part]
Each duration part MUST be an integer literal immediately followed by a unit suffix: `w` (weeks), `d` (days), `h` (hours), `m` (minutes), or `s` (seconds).

```ebnf
duration literal = duration part, { duration part } ;
duration part    = integer literal, duration unit ;
duration unit    = "w" | "d" | "h" | "m" | "s" ;
```

## Expressions

### Literal Expressions

TODO: fill out

### Dot-Path Access

r[syntax.dot-path]
Nested fields MAY be set using dot-path syntax (e.g., `recurs.from: 2025-07-01`) as an alternative to a full nested block. Both forms MUST be valid.

### Records

r[type.record.syntax]
Records MUST use braces with comma-separated fields. Trailing commas MUST be allowed. Newlines MAY substitute for commas.

r[type.record.field]
Record fields MUST follow the pattern `Path ':' '='? Value`, where Path is a dot-separated identifier chain.

### Lists

r[type.list.syntax]
Lists MUST use square brackets with comma-separated values. Single-element lists MUST require brackets — no sugar for single-element lists.


### Recurrence

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

## Types

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

## Declarations

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
