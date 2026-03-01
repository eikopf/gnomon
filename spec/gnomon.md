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

Gnomon's expression syntax consists of only three grammar rules: literal expressions, record expressions, and list expressions.

> r[expr.syntax]
> The grammar for expressions is as follows:
>
> ```ebnf
> expr = literal expr
>      | record expr
>      | list expr
>      ;
> ```

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

Tasks represent action items, assignments, TODO items, or other similar objects. They can be given a specific relationship to time, but by default nothing is required except a `name` field whose value is a name.

r[record.task.name]
Records representing tasks MUST have a field named `name` whose value is a name.

The optional `uid` field on tasks is always assigned a value, which will default to the name of the task if it is omitted.

r[record.task.uid]
Records representing tasks MUST have a field named `uid` whose value is either a string or a name. If the field is omitted in the source data, it MUST have the same value as the `name` field.

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


> r[record.rrule.every+2]
> The syntax of an `every` expression is the following:
>
> ```ebnf
> every expr       = "every", every subject, [ "until", every terminator ] ;
> every subject    = "day"
>                  | "year", "on", month day literal
>                  | weekday
>                  ;
> every terminator = datetime literal
>                  | date literal
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

r[record.rrule.every.desugar.terminator+2]
If the terminator of an `every` expression is given, its value (the datetime or integer literal) MUST be assigned to the `termination` field in the desugared record. If the terminator is omitted, the `termination` field in the desugared record MUST be omitted or set to `undefined`. If the terminator is a date literal, it MUST be treated as the corresponding datetime literal where the time component is `00:00:00`.

#### Evaluation

TODO: describe the evaluation semantics of recurrence rules

r[record.rrule.eval.empty]
An error SHOULD be produced if a recurrence rule is empty.

## Common Record Fields

### `uid`
Name: `uid`

Value: string (no default)

Meaning: The unique identifier of the object. This will usually be a UUID, but older implementations of iCalendar can still use arbitrary strings as unique identifiers. If the value is omitted, a stable UUIDv5 is computed using the `name` field of the object as the local key and the `uid` of the calendar as the namespace.

r[field.uid.type]
If present, the `uid` field MUST have a string value.

### `title`
Name: `title`

Value: string (no default)

Meaning: A short summary of the object.

r[field.title.type]
If present, the `title` field MUST have a string value.

### `description`
Name: `description`

Value: string or record `{ type: string, content: string }` (no default)

Meaning: A longer description of the object. If the value is a record, the `content` field is the body of the description and the `type` is an RFC 6838 media type. If the value is a string, the implied media type is `text/plain`.

r[field.description.type]
If present, the `description` field MUST have a string or record value.

r[field.description.type.string]
If the value of the `description` field is a string, it MUST be equivalent to specifying the value as a record whose `type` field has the value `"text/plain"` and whose `content` field matches the given string value.

r[field.description.type.record]
If the value of the `description` field is a record, it MUST have fields named `type` and `content` whose values MUST be strings. The value of the `type` field MUST be an RFC 6838 media type, MUST be a subtype of the `text` type, and SHOULD be `text/plain` or `text/html`. The given media type MAY include parameters, and the `charset` parameter value MUST be `utf-8` if specified.

## More Fields

TODO: define the most commonly used fields in both event and task (see JSCalendar spec for details)

## Declarations

Declarations are the top-level grammar element in Gnomon, and source data is ultimately parsed as a sequence of declarations.

> r[syntax.start]
> Source data MUST be parsed as a sequence of declarations.
>
> ```ebnf
> START = { decl } ;
> ```

> r[decl.syntax]
> The grammar for declarations is as follows:
>
> ```ebnf
> decl = inclusion
>      | binding
>      | short event
>      | short task
>      | decl prefix, record expr
>      ;
>
> inclusion = "include", string literal ;
>
> binding = "bind", name, string literal ;
>
> decl prefix = "calendar"
>             | "event"
>             | "task"
>             ;
>
> short event = "event", name,  short span,  [ string literal ], [ record expr ] ;
> short task  = "task",  name, [ short dt ], [ string literal ], [ record expr ] ;
>
> short span = short dt, [ duration literal ] ;
>
> short dt = date literal, time literal
>          | datetime literal
>          ;
> ```

## CLI

A valid implementation of Gnomon must produce a program (hereafter called `gnomon`, although it may be installed with a different name) whose command-line interface has the behavior described in this section. A valid implementation may introduce additional subcommands so long as they do not conflict with existing or reserved subcommands, but must implement all the subcommands described here unless stated otherwise.

> r[cli.syntax]
> The command-line interface MUST have the following syntax:
> 
> ```ebnf
> CLI START    = command name, [ options ], [ subcommand, [ options ] ];
> command name = shell ident;
> shell ident  = ? a POSIX-compatible shell identifier ? ;
>
> options = { option } ;
> option  = ? a POSIX-compatible shell identifier starting with one or two hyphens ? ;
>
> subcommand = shell ident, [ options ], [ subcommand ] ;
> ```

While it is not an explicit requirement, implementations SHOULD try to comply with the guidelines outlined in the [Command Line Interface Guidelines](https://clig.dev).

### The Root Command

If no subcommand passed, the root command is run. This command does not perform any expensive or potentially dangerous action, and just returns information based on the options passed.

r[cli.root]
The result of running the root command without any options MUST be the same as running the root command with only the `--help` option.

### Subcommands

A subcommand is essentially a smaller program invoked by passing an identifier to the root command. Subcommands may have their own subcommands, and their meaning is independent of the order and positioning of any options that are passed along with them.

r[cli.subcommand.order]
The subcommand being selected MUST be independent of any options being passed, and MUST be uniquely described by the relative ordering of the subcommand identifiers.

#### `help`

The `help` subcommand is a subcommand of the root command and also a subcommand of all other subcommands except itself. All immediate subcommands of the parent command are also subcommands of the `help` subcommand. When `help` is run as the subcommand of the root command, its behavior is to print a help message about the entire program; if it is run as the second-last subcommand then its behavior is to print a help message about the final subcommand.

r[cli.subcommand.help]
The program MUST provide a `help` subcommand for the root command and for every other subcommand.

r[cli.subcommand.help.penultimate]
When `help` is the second-last (penultimate) subcommand, its behavior MUST be to print a help message about the last subcommand.

r[cli.subcommand.help.root]
When `help` is the only subcommand of the root command, its behavior MUST be to print a help message about the entire program.

#### `parse`

The `parse` subcommand is a subcommand of the root command; it takes a single file path as a parameter. When executed, `gnomon parse <file>` will resolve the file path (failing if it cannot be found) and then produce an output which describes the result of parsing the file.

r[cli.subcommand.parse]
The program MUST provide a `parse` subcommand for the root command which takes a single parameter describing a file path.

r[cli.subcommand.parse.no-file]
If the file path argument to the `parse` subcommand cannot be resolved to a file for any reason, the program MUST produce an error.

r[cli.subcommand.parse.output]
If a file was successfully located, the program MUST write a textual representation of the result of applying a Gnomon parser to the file to STDOUT.

#### Reserved Subcommands

We reserve some identifiers for future use as subcommands.

> r[cli.subcommand.reserved]
> The following identifiers MUST NOT be used by any implementation:
>
> - `about`
> - `check`
> - `clean`
> - `compile`
> - `daemon`
> - `fetch`
> - `lsp`
> - `query`
> - `run`

### Options

r[cli.option.order]
The behavior of the program MUST be independent of the relative ordering of the options passed to it.

#### `--help`

The `--help` option may occur on any command and is mutually exclusive with all other options. Passing the `--help` option must be equivalent to using the `help` subcommand immediately before the final subcommand (i.e. `gnomon foo bar baz --help` is equivalent to `gnomon foo bar help baz`).

r[cli.option.help]
The program MUST admit a `--help` option on the root command and all subcommands.

r[cli.option.help.short]
The program MUST provide the `-h` option as a short form of `--help`.

r[cli.option.help.xor]
If the `--help` option is passed with any other options, the program MUST produce an error.

r[cli.option.help.behavior.root]
When run with the root command, the `--help` option MUST be equivalent to running the `help` subcommand without any additional subcommands or options.

r[cli.option.help.behavior.subcommand]
When run with a subcommand, the `--help` option MUST be equivalent to running the same shell command with the `--help` option removed and the `help` subcommand inserted directly before the final subcommand.

#### `--version`

The `--version` option is only permitted on the root command and is mutually exclusive with all other options. Running `gnomon --version` will cause the version string to be printed to STDOUT, after which the program will immediately exit.

r[cli.option.version]
The program MUST admit a `--version` option on the root command. If this option is passed to any subcommand, the program MUST produce an error.

r[cli.option.version.short]
The program MUST provide the `-v` option as a short form of `--version`.

r[cli.option.version.xor]
If the `--version` option is passed with any other options, the program MUST produce an error.

r[cli.option.version.behavior]
When run with the root command, the `--version` option MUST cause the program to print the version string to STDOUT and then immediately exit.
