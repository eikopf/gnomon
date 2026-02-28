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
> - `task`
> - `every`
> - `day`
> - `year`
> - `on`
> - `until`
> - `times`
> - `omit`
> - `forward`
> - `backward`
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

### Signed Integer Literals

A signed integer literal represents an integer in the range `i64::MIN..=i64::MAX`, inclusive.

r[lexer.signed-integer]
A signed integer literal MUST be a sign character (`+` or `-`) followed immediately by a non-empty sequence of ASCII digits.

r[lexer.signed-integer.range]
It is an error for a signed integer literal to have a value outside the range `i64::MIN..=i64::MAX`.

```ebnf
signed integer literal = sign, digit, { digit } ;
sign                   = "+" | "-" ;
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

A literal expression is a string literal, integer literal, signed integer literal, date literal, month-day literal, time literal, datetime literal, duration literal, `true`, or `false`.

> r[expr.literal.syntax]
> The grammar for literal expressions is as follows:
>
> ```ebnf
> literal expr = string literal
>              | integer literal
>              | signed integer literal
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

> r[expr.list.syntax]
> The grammar for list expressions is as follows:
>
> ```ebnf
> list = "[", list elements, "]" ;
> list elements = expr, { ",", expr }, [ "," ] ;
> ```

## Record Types

Gnomon distinguishes specific record types for use in certain contexts. These types are identified by their fields (which may be mandatory or optional), and by the types of the values associated with those fields.

### Events

Events represent scheduled amounts of time on a calendar; they are required to start at a certain point in time and usually have a non-zero duration. They have two mandatory fields, `name` and `start`; these have as values a name and a local datetime respectively.

r[record.event.name]
Records representing events MUST have a field named `name` whose value is a name.

r[record.event.start]
Records representing events MUST have a field named `start` whose value is a local datetime.

The optional `uid` field on events is always assigned a value, which will default to the name of the event if it is omitted.

r[record.event.uid]
Records representing events MUST have a field named `uid` whose value is either a string or a name. If the field is omitted in the source data, it MUST have the same value as the `name` field.

### Tasks

### Recurrence Rules

A recurrence rule is a record describing how a calendar item recurs, and has the semantics of an RFC 5545 recurrence rule.

Before defining the precise fields of a recurrence rule, we need some helper definitions. First, a weekday is one of the seven conventional days of the week.

r[record.rrule.weekday]
A weekday MUST be `monday`, `tuesday`, `wednesday`, `thursday`, `friday`, `saturday`, or `sunday`.

Next, an N-day is the record type used by the `by_day` field on recurrence rules; it represents either a weekday or a specific instance of a weekday within the recurrence period.

r[record.rrule.n-day]
An N-day MUST be a record with a mandatory field `day` and an optional field `nth`. The value of the `day` field MUST be a weekday, and the value of the `nth` field MUST be a nonzero signed integer if set.

Lastly, a leap-month is the record type used by the `by_month` field on recurrence rules; it represents a month together with a flag determining whether it is a leap month or not. While we currently only support the Gregorian calendar, which has only a single leap month, recurrence rules are the one place where alternative calendar scales are commonly used and arbitrary leap month support will eventually be necessary for them.

r[record.rrule.leap-month]
A leap-month MUST be a record with two mandatory fields `month` and `leap`. The value of the `month` field MUST be a strictly positive integer, and the value of the `leap` field MUST be `true` or `false`.

Now we can provide an unambiguous definition for recurrence rules:

> r[record.rrule.syntax]
> A recurrence rule is a record with the following fields:
>
> | Field         | Value Type | Meaning |
> |----------|----|---------|
> | `frequency` | `yearly` \| `monthly` \| `weekly` \| `daily` \| `hourly` \| `minutely` \| `secondly` | The time span covered by each iteration of this rule. |
> | `interval` | Strictly positive integer (default: `1`) | The interval of iteration periods at which the rule repeats. |
> | `skip` | `omit` \| `forward` \| `backward` (default: `omit`) | The behaviour of the rule when an invalid date is generated. |
> | `week_start` | A weekday (default: `monday`) | The first day of the week. |
> | `termination` | A local datetime or unsigned integer or `undefined` (default: `undefined`) | The termination criterion for the rule. This subsumes the `COUNT` and `UNTIL` parts from RFC 5545 |
> | `by_day` | A list of N-day records | The days of the week on which to repeat. |
> | `by_month_day` | A list of nonzero signed integers in the range `-31..=31` | The days of the month on which to repeat. |
> | `by_month` | A list of leap-month records | The months on which to repeat. |
> | `by_year_day` | A list of nonzero signed integers in the range `-366..=366` | The days of the year on which to repeat. |
> | `by_week_no` | A list of nonzero signed integers in the range `-53..=53` | The weeks of the year in which to repeat. |
> | `by_hour` | A list of integers in the range 0..=23 | The hours of the day in which to repeat. |
> | `by_minute` | A list of integers in the range 0..=59 | The minutes of the day in which to repeat. |
> | `by_second` | A list of integers in the range 0..=60 | The seconds of the day in which to repeat. |
> | `by_set_position` | A list of signed integers | The occurrences within the recurrence interval to include in the final results. |
>
> All fields except `frequency` are optional.


#### `every` Expressions

Writing recurrence rules as records is annoying, and so Gnomon includes a small DSL for writing the most common subset of recurrence rules. Expressions in this DSL are called `every` expressions.


> r[record.rrule.every]
> The syntax of an `every` expression is the following:
>
> ```ebnf
> every expr       = "every", every subject, [ "until", every terminator ] ;
> every subject    = "day"
>                  | "year", "on", month day literal
>                  | weekday
>                  ;
> every terminator = datetime literal
>                  | integer literal, "times"
>                  ;
> ```

The exact desugaring of these expressions is left underspecified; there are multiple ways that an arbitrary `every` expression could be desugared and we guarantee only that all possible desugarings will be equivalent under the RFC 5545 interpretation of recurrence rules.

r[record.rrule.every.desugar.equivalence]
The exact desugaring of an `every` expression is implementation-defined, but the chosen desugaring MUST be equivalent to all other valid desugarings.

With that warning in mind, we have the following requirements for desugaring such expressions:

r[record.rrule.every.desugar.subject.day]
If the subject of an `every` expression is the `day` keyword, the `frequency` field in the desugared record MUST be set to the value `daily`.

r[record.rrule.every.desugar.subject.year-on-month-day]
If the subject of an `every` expression is of the form `year on MM-DD`, the `frequency` field in the desugared record MUST be set to the value `yearly` and the `by_year_day` field in the desugared record MUST be set to the singleton list value `[D]` where `D` is the number of days from the start of a non-leap year to `MM-DD`.

r[record.rrule.every.desugar.subject.weekday]
If the subject of an `every` expression is a weekday, the `frequency` field in the desugared record MUST be set to the value `weekly` and the `by_day` field in the desugared record MUST be set to the singleton list value `[{ day: D }]` where `D` is the index of the given weekday (starting from `1` for `monday`).

r[record.rrule.every.desugar.terminator]
If the terminator of an `every` expression is given, its value (the datetime or integer literal) MUST be assigned to the `termination` field in the desugared record. If the terminator is omitted, the `termination` field in the desugared record MUST be omitted or set to `undefined`.

#### Evaluation

TODO: describe the evaluation semantics of recurrence rules

r[record.rrule.eval.empty]
An error SHOULD be produced if a recurrence rule is empty.

## Common Record Fields

TODO: optional record fields which have similar or identical meanings on multiple record types

## Declarations

### Component Short Syntax

r[syntax.short.event]
The event short syntax MUST follow the form: `event` Name? (RecurPat | DateLit) TimeLit? DurationLit? String Alarm* Record?

r[syntax.short.task]
The task short syntax MUST follow the form: `task` Name? DateLit? String Record?

r[syntax.short.name-position]
A name, if present, MUST appear immediately after the keyword.

r[syntax.short.duplicate-field]
A field provided both positionally and in the braced block MUST be an error.

```
event @standup every Monday 09:00 1h "Standup" !15m { tag: work }
event @deep-work 2025-07-14 14:00-16:00 "Deep work block"
task @invoice 2025-07-15 "Submit invoice" { priority: high }
task "Read RFC 7986" { tag: reading }
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
