# Gnomon Specification

A plaintext language for authoring and maintaining calendars, designed to compile to iCalendar and JSCalendar.

## Introduction

TODO: write prose introduction

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

Gnomon uses Lisp-style semicolon comments for no reason other than that the semicolon is an otherwise unused character.

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

Whitespace does not have any semantic significance, and replacing any whitespace string with any other whitespace string does not change the meaning of a Gnomon program.

### Punctuation

> r[lexer.punctuation]
> The following characters MUST be recognized as punctuation:
> 
> | Token | Name |
> |-------|------|
> | `{` | Left brace |
> | `}` | Right brace |
> | `[` | Left bracket |
> | `]` | Right bracket |
> | `:` | Colon |
> | `,` | Comma |
> | `=` | Equals |
> | `!` | Bang |
> | `.` | Dot |
> | `-` | Hyphen |


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

Gnomon distinguishes between strict and weak keywords. A strict keyword may only be used as keyword, whereas a weak keyword decays into an identifier outside of the designated contexts in which it operates as a keyword.

The strict keywords are `true`, `false`, and `undefined`.

r[lexer.keyword.strict]
The keywords `true`, `false`, and `undefined` MUST be treated as strict.

All other keywords are weak.

> r[lexer.keyword.weak]
> All keywords other than `true`, `false`, and `undefined` MUST be treated as weak; these keywords are
> 
> - `calendar`
> - `include`
> - `bind`
> - `override`
> - `event`
> - `todo`
> - `every`
> - `day`
> - `weeks`
> - `year`
> - `omit`
> - `forward`
> - `backward`
> - `st`, `nd`, `rd`, `th` (ordinal suffixes)
> - `monday`, `tuesday`, `wednesday`, `thursday`, `friday`, `saturday`, `sunday`
> - `local`


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

A date literal represents an ISO 8601/RFC 3339 date.

> r[lexer.date]
> The syntax of a date literal is the following:
>
> ```ebnf
> date literal = year, "-", month, "-", day ;
> 
> year  = digit, digit, digit, digit ;
> month = digit, digit ;   (* 01..=12 *)
> day   = digit, digit ;   (* 01..=31 *)
> ```

Date literals desugar into records with three integer fields named `year`, `month`, and `day`.

r[lexer.date.desugar]
The date literal `YYYY-MM-DD` MUST desugar into the record `{ year: YYYY, month: MM, day: DD }`.

### Month-Day Literals
A month-day literal represents an ISO 8601/RFC 3339 date with the year omitted.

> r[lexer.month-day]
> The syntax of a month-day literal is the following:
>
> ```ebnf
> month day literal = month, "-", day ;
> 
> month = digit, digit ;   (* 01..=12 *)
> day   = digit, digit ;   (* 01..=31 *)
> ```

Month-day literals desugar into records with two integer fields named `month` and `day`.

r[lexer.month-day.desugar]
The month-day literal `MM-DD` MUST desugar into the record `{ month: MM, day: DD }`.

### Time Literals
A time literal represents an ISO 8601/RFC 3339 time with no fractional second component.

> r[lexer.time]
> The syntax of a time literal is the following:
> 
> ```ebnf
> time literal = hour, ":", minute, [ ":", second ] ;
> 
> hour   = digit, digit ;   (* 00..=23 *)
> minute = digit, digit ;   (* 00..=59 *)
> second = digit, digit ;   (* 00..=60 *)
> ```

When the `second` is omitted, it is treated as zero.

r[lexer.time.default-second]
The time literal `HH:MM` MUST be equivalent to `HH:MM:00`.

Time literals desugar into records with three integer fields named `hour`, `minute`, and `second`.

r[lexer.time.desugar]
The time literal `HH:MM:SS` MUST desugar into the record `{ hour: HH, minute: MM, second: SS }`. 

### Datetime Literals

A datetime literal represents the composite of a [date literal](#lexical-syntax--date-literals) and a [time literal](#lexical-syntax--time-literals).

> r[lexer.datetime]
> The syntax of a datetime literal is the following:
>
> ```ebnf
> datetime literal = date literal, "T", time literal ;
> ```

Datetime literals desugar into records with two fields named `date` and `time`; each field contains the desugared record of the corresponding component.

> r[lexer.datetime.desugar]
> The datetime literal `YYYY-MM-DDTHH:mm:SS` must desugar into the following record:
>
> ```gnomon
> {
>    date: {
>        year: YYYY,
>        month: MM,
>        day: DD,
>    },
>    time: {
>        hour: HH,
>        minute: mm,
>        second: SS,
>    },
> } 
> ```

### Duration Literals

A duration literal represents an RFC 5545 duration.

> r[lexer.duration]
> The syntax of a duration literal is the following:
> 
> ```ebnf
> duration literal = [ sign ], duration part, { duration part } ;
> duration part    = integer literal, duration unit ;
> duration unit    = "w" | "d" | "h" | "m" | "s" ;
> sign             = "+" | "-" ;
> ```

No unit may occur more than once in the same duration literal.

r[lexer.duration.part.multiplicity]
Each duration unit MUST occur at most once in a duration literal.

Following with the convention of RFC 5545, the sign defaults to positive.

r[lexer.duration.sign]
If the sign is omitted from a duration literal, it MUST be treated as though it has a positive sign.

Likewise any omitted unit is treated as having a value of zero.

r[lexer.duration.part.default]
Any omitted duration unit MUST be treated as though it had been given with the integer literal `0`.

Duration literals desugar into records with five integer fields named `weeks`, `days`, `hours`, `minutes`, and `seconds`.

> r[lexer.duration.desugar]
> Let `DUR` be a duration literal with weeks `W`, days `D`, hours `H`, minutes `M`, and seconds `S`; then `DUR` MUST desugar into the following record:
> 
> ```gnomon
> {
>    weeks: W,
>    days: D,
>    hours: H,
>    minutes: M,
>    seconds: S,
> }
> ```

## Expressions

### Literal Expressions

A literal expression is a string literal, integer literal, date literal, month-day literal, time literal, datetime literal, duration literal, `true`, or `false`.

> r[expr.literal.syntax]
> The grammar for literal expressions is as follows:
>
> ```ebnf
> literal expr = string literal
>              | integer literal
>              | date literal
>              | month day literal
>              | time literal
>              | datetime literal
>              | duration literal
>              | "true"
>              | "false"
>              ;
> ```

Date, month-day, time, datetime, and duration literals are syntax sugar for records with specific fields set. In practice, an implementation should probably not desugar these values until a user explicitly asks them to (e.g. during rendering, or as an option when displaying values) in order to provide easier-to-read outputs.

### Records

A record is a table mapping from identifiers to values.

> r[expr.record.syntax]
> The grammar for record expressions is as follows:
> 
> ```ebnf
> record = "{", [ fields ], "}" ;
> fields = field, { ",", field }, [ "," ] ;
> field  = identifier, ":", expr ;
> ```

An identifier may occur at most once as a key in a record.

r[expr.record.keys]
An identifier MUST NOT appear more than once as a key in a record.

### Lists

A list is a contiguous sequence of zero or more values.

> r[type.list.syntax]
> The grammar for list expressions is as follows:
>
> ```ebnf
> list = "[", list elements, "]" ;
> list elements = expr, { ",", expr }, [ "," ] ;
> ```

### Recurrence Rules

A recurrence rule is a record describing how a calendar item recurs, and has the semantics of an RFC 5545 recurrence rule.

```gnomon
;; a record approximating a recurrence rule
{
    ;; yearly | monthly | weekly | daily | hourly | minutely | secondly
    frequency: daily
    ;; positive integer, defaulting to 1
    interval: 1
    ;; omit | backward | forward, defaulting to omit
    skip: omit
    ;; a weekday, defaulting to monday
    first_day_of_week: monday
    ;; local datetime | integer | undefined, defaulting to undefined
    termination: undefined,
    ;; list of { day: weekday, offset: signed integer } 
    by_day: [],
    ;; list of signed integers in ranges -31..=1 and 1..=31
    by_month_day: [],
    ;; list of { month: integer, leap: bool }
    by_month: [],
    ;; list of signed integers in ranges -366..=1 and 1..=366
    by_year_day: [],
    ;; list of signed integers in ranges -53..=1 and 1..=53
    by_week_no: [],
    ;; list of integers in range 0..=23
    by_hour: [],
    ;; list of integers in range 0..=59
    by_minute: [],
    ;; list of integers in range 0..=60
    by_second: [],
    ;; list of signed integers
    by_set_position: [],
}
```

#### `every`

r[syntax.recur.every]
The `every` prefix MUST replace the date position in an event or todo to indicate recurrence.

r[syntax.recur.every-grammar]
The `every` mini-grammar MUST support the forms: `every <weekday>` (weekly), `every <n> weeks <weekday>` (biweekly), `every <ord> <weekday>` (monthly), `every day` (daily), and `every year <MM-DD>` (yearly). Anything outside this grammar MUST use the full form.

| Pattern | Desugared |
|---------|-----------|
| `every Monday` | `{ freq: weekly, day: Monday }` |
| `every 2 weeks Monday` | `{ freq: biweekly, day: Monday }` |
| `every 2nd Wednesday` | `{ freq: monthly, ord: 2, day: Wednesday }` |
| `every day` | `{ freq: daily }` |
| `every year 03-15` | `{ freq: yearly, date: { month: 3, day: 15 } }` |

Anything that does not fit this grammar MUST use the full form.

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
Sub-component and override identity MUST derive from the parent's UID, as `UUIDv5(parent_uid, instance_key)` where the instance key is the override date or alarm signature.

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
