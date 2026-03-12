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

r[lexer.whitespace.insignificant]
Replacing any whitespace string with any other whitespace string MUST NOT change the meaning of a Gnomon program.

### Punctuation

> r[lexer.punctuation+2]
> The following characters and character sequences MUST be recognized as punctuation:
>
> | Token | Name |
> |-------|------|
> | `{` | Left brace |
> | `}` | Right brace |
> | `[` | Left bracket |
> | `]` | Right bracket |
> | `(` | Left paren |
> | `)` | Right paren |
> | `:` | Colon |
> | `,` | Comma |
> | `.` | Dot |
> | `-` | Hyphen |
> | `=` | Equals |
> | `!` | Bang |
> | `==` | Equals-equals |
> | `!=` | Bang-equals |
> | `++` | Plus-plus |
> | `//` | Slash-slash |
> | `+` | Plus |
> | `/` | Slash |


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

> r[lexer.keyword.weak+3]
> All keywords other than `true`, `false`, and `undefined` MUST be treated as weak; these keywords are
>
> - `calendar`
> - `event`
> - `task`
> - `import`
> - `as`
> - `let`
> - `in`
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
> - `gnomon`, `icalendar`, `jscalendar`
> - `override`


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

### Triple-Quoted String Literals

A triple-quoted string literal is a multi-line string delimited by `"""` (three double-quote characters). Triple-quoted strings support the same escape sequences as regular strings, but may span multiple lines and may contain unescaped `"` and `""` characters.

> r[lexer.triple-string]
> The syntax of a triple-quoted string literal is the following:
>
> ```ebnf
> triple string      = '"""', { triple string char }, '"""' ;
> triple string char = any char - '"""'
>                    | "\\", escape char ;
> ```

r[lexer.triple-string.multiline]
Triple-quoted string literals MAY span multiple lines.

r[lexer.triple-string.embedded-quotes]
Unescaped `"` and `""` characters within a triple-quoted string literal MUST be preserved literally. Only the sequence `"""` terminates the string.

r[lexer.triple-string.escape]
The same escape sequences recognized in regular string literals (`\"`, `\\`, `\n`, `\t`) MUST be recognized within triple-quoted string literals.

Triple-quoted string literals are auto-dedented. The dedent algorithm is:
1. Split the content (between the opening and closing `"""` delimiters) into lines.
2. If the first line (immediately after the opening `"""`) is empty, remove it.
3. If the last line (immediately before the closing `"""`) consists only of whitespace, remove it and use its length as the dedent baseline.
4. If no baseline was established in step 3, compute the minimum indentation across all non-empty remaining lines.
5. Strip that many leading whitespace characters from each remaining line.
6. Join the remaining lines with newlines.

r[lexer.triple-string.dedent]
Triple-quoted string literals MUST be auto-dedented using the algorithm described above.

Triple-quoted string literals desugar into strings.

r[lexer.triple-string.desugar]
A triple-quoted string literal MUST desugar into a string value containing the dedented content with escape sequences resolved.

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

r[lexer.date.valid]
A date literal MUST represent a valid Gregorian calendar date. February 29 is valid only in leap years.

r[lexer.time.leap-second]
Implementations MUST NOT reject a second value of 60 (leap second) based on whether a leap second actually occurred at the given time, as this is expensive to verify.

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

r[lexer.month-day.valid]
A month-day literal MUST represent a day that is possible for the given month. Since no year is available, the maximum day count for February MUST be 29.

For example, `02-29` is valid (it is possible in leap years), but `02-30` is not.

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

#### Local Datetimes

iCalendar and JSCalendar distinguish UTC datetimes (with a `Z` suffix) from local or floating datetimes (without one). Explicit UTC offset suffixes are not permitted.

r[lexer.datetime.local]
All Gnomon datetime literals are local datetimes. The timezone of a local datetime is determined by the `time_zone` field on the enclosing event or task, or by the calendar-level default if none is specified.

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

### URI Literals

A URI literal represents a URI as defined by RFC 3986. URI literals are delimited by angle brackets (`<` and `>`), which avoids any ambiguity with existing syntax.

> r[lexer.uri]
> The syntax of a URI literal is the following:
>
> ```ebnf
> uri literal = "<", scheme, ":", uri body, ">" ;
> scheme      = letter, { letter | digit | "+" | "-" | "." } ;
> uri body    = { any char - ">" - newline } ;
> ```

r[lexer.uri.no-multiline]
URI literals MUST NOT span multiple lines. An unescaped newline within a URI literal is an error.

URI literals desugar into strings containing the URI without the angle bracket delimiters.

r[lexer.uri.desugar]
The URI literal `<U>` MUST desugar into the string `"U"`.

### Atom Literals

An atom literal is a shorthand for writing a string literal that contains no whitespace or special characters. It consists of a `#` character followed by an identifier.

> r[lexer.atom]
> The syntax of an atom literal is the following:
>
> ```ebnf
> atom literal = "#", identifier ;
> ```

Atom literals desugar into strings containing the identifier without the `#` prefix.

r[lexer.atom.desugar]
The atom literal `#X` MUST desugar into the string `"X"`.

### Path Literals

A path literal represents a filesystem path. Path literals are used with `import` expressions to reference other source files. A path literal must contain at least one slash to distinguish it from an identifier.

> r[lexer.path]
> The syntax of a path literal is the following:
>
> ```ebnf
> path literal = path segment, "/", { path char } ;
> path segment = "." | ".." | path name ;
> path name    = (letter | digit | "_" | "-" | "."), { letter | digit | "_" | "-" | "." } ;
> path char    = letter | digit | "_" | "-" | "." | "/" ;
> ```

r[lexer.path.slash]
A path literal MUST contain at least one `/` character.

r[lexer.path.relative]
Path literals are resolved relative to the directory containing the file in which they appear.

## Expressions

Gnomon's expression syntax includes literal expressions, record expressions, list expressions, import expressions, let-in expressions, every expressions, calendar expressions, event expressions, task expressions, operator expressions, and parenthesized expressions.

> r[expr.syntax+4]
> The grammar for expressions is as follows:
>
> ```ebnf
> expr = comparison expr ;
>
> comparison expr = concat expr, [ comparison op, concat expr ] ;
> comparison op   = "==" | "!=" ;
>
> concat expr = postfix expr, { concat op, postfix expr } ;
> concat op   = "++"    (* list concatenation *)
>             | "//"    (* record merge *)
>             ;
>
> postfix expr = primary expr
>              | postfix expr, ".", identifier
>              | postfix expr, "[", expr, "]"
>              ;
>
> primary expr = literal expr
>              | record expr
>              | list expr
>              | import expr
>              | let expr
>              | every expr
>              | calendar expr
>              | event expr
>              | task expr
>              | "(", expr, ")"
>              ;
> ```

### Literal Expressions

A literal expression is a string literal, triple-quoted string literal, integer literal, signed integer literal, date literal, month-day literal, time literal, datetime literal, duration literal, URI literal, atom literal, path literal, name, `true`, `false`, or `undefined`.

> r[expr.literal.syntax+5]
> The grammar for literal expressions is as follows:
>
> ```ebnf
> literal expr = string literal
>              | triple string literal
>              | integer literal
>              | signed integer literal
>              | date literal
>              | month day literal
>              | time literal
>              | datetime literal
>              | duration literal
>              | uri literal
>              | atom literal
>              | path literal
>              | name
>              | identifier
>              | "true"
>              | "false"
>              | "undefined"
>              ;
> ```

Date, month-day, time, datetime, and duration literals are syntax sugar for records with specific fields set. In practice, an implementation should probably not desugar these values until a user explicitly asks them to (e.g. during rendering, or as an option when displaying values) in order to provide easier-to-read outputs.

An identifier appearing as a literal expression refers to a variable introduced by a `let` binding.

r[expr.literal.identifier]
An identifier used as a literal expression MUST refer to a variable bound by an enclosing `let` expression. It is an error if the identifier does not refer to any binding in scope.

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

### Import Expressions

An import expression loads and evaluates a source file, producing a Gnomon value. The source may be a Gnomon file, an iCalendar file, a JSCalendar file, or any other supported format. The format is normally inferred from the file extension or content, but may be specified explicitly using the `as` keyword.

> r[expr.import.syntax+2]
> The grammar for import expressions is as follows:
>
> ```ebnf
> import expr = "import", import source, [ "as", format ] ;
> import source = path literal | uri literal ;
> format = "gnomon" | "icalendar" | "jscalendar" ;
> ```

r[expr.import.eval]
An `import` expression MUST evaluate the referenced source and produce the resulting Gnomon value. For Gnomon sources, this is the result of evaluating the file. For foreign formats, this is the translation of the foreign data into the Gnomon data model.

r[expr.import.format+2]
If the `as` keyword is present, the implementation MUST interpret the source in the specified format. If the `as` keyword is absent, the implementation MUST infer the format from the file extension: `.ics` maps to `icalendar`, `.json` maps to `jscalendar`, and all other extensions are treated as `gnomon`.

r[expr.import.format.uri]
For URI imports where the `as` keyword is absent and the URL path does not have a recognized extension, the implementation MUST infer the format from the HTTP `Content-Type` response header: `text/calendar` maps to `icalendar`, `application/json` and `application/jscalendar+json` map to `jscalendar`, and all other content types are treated as `gnomon`.

r[expr.import.eager]
Import expressions MUST be evaluated eagerly.

r[expr.import.cycle]
Circular imports MUST be detected and rejected with an error.

#### URI Import Caching

URI imports may be cached on the local filesystem to avoid redundant network requests. The cache is best-effort: failures to read or write the cache MUST NOT prevent evaluation from proceeding.

r[expr.import.cache.location]
The URI import cache MUST be stored under `$XDG_CACHE_HOME/gnomon/uri`. If `$XDG_CACHE_HOME` is not set, the implementation MUST fall back to the platform-appropriate default cache directory.

r[expr.import.cache.key]
Each cached entry MUST be keyed by the import URI. The mapping from URI to cache key MUST be deterministic.

r[expr.import.cache.content]
The cache MUST store the raw response body and enough metadata to determine freshness and to reproduce the format inference that would occur on a live fetch (at minimum: the URI, the fetch timestamp, and the `Content-Type` header value).

r[expr.import.cache.freshness]
A cached entry MUST be considered fresh if the time elapsed since it was fetched is less than the entry's refresh interval. If the source document specifies a refresh interval (e.g. the iCalendar `REFRESH-INTERVAL` property), that value MUST be used. If no refresh interval is available, the implementation MUST use a default of one day (86 400 seconds).

r[expr.import.cache.miss]
On a cache miss or stale entry, the implementation MUST fetch the URI over the network and update the cache with the response.

r[expr.import.cache.hit]
On a cache hit, the implementation MUST use the cached content and MUST NOT make a network request.

r[expr.import.cache.best-effort]
If the cache directory cannot be created or a cache entry cannot be read or written, the implementation MUST silently fall back to a network fetch. Cache failures MUST NOT produce diagnostics.

r[expr.import.cache.evict]
When writing a new cache entry, the implementation MUST remove any entries whose age exceeds 30 days (2 592 000 seconds). Eviction failures MUST be silently ignored.

### Let Expressions

A `let` expression introduces a local binding that is in scope for the body expression. Let bindings are sequential: a binding may refer to earlier bindings but not to itself or later bindings.

> r[expr.let.syntax]
> The grammar for let expressions is as follows:
>
> ```ebnf
> let expr = "let", identifier, "=", expr, "in", expr ;
> ```

r[expr.let.sequential]
Let bindings MUST be sequential: the bound expression may reference variables from enclosing or preceding `let` bindings, but MUST NOT reference the variable being bound or any later bindings.

r[expr.let.scope]
The variable introduced by a `let` binding MUST be in scope for the body expression (the expression after `in`).

### Operator Precedence and Associativity

> r[expr.op.precedence]
> Operators MUST be evaluated with the following precedence, from highest to lowest:
>
> | Precedence | Operators | Associativity |
> |------------|-----------|---------------|
> | Highest | `.` (field access), `[]` (index access) | Left (postfix) |
> | Middle | `++` (concatenation), `//` (merge) | Right |
> | Lowest | `==` (equality), `!=` (inequality) | Non-associative |

r[expr.op.assoc.concat-merge]
The `++` and `//` operators MUST be right-associative. That is, `a ++ b ++ c` MUST be parsed as `a ++ (b ++ c)`, and `a // b // c` MUST be parsed as `a // (b // c)`.

r[expr.op.assoc.comparison]
The `==` and `!=` operators MUST be non-associative. Chaining comparisons (e.g., `a == b == c` or `a != b == c`) MUST be a parse error.

### Operators

#### List Concatenation (`++`)

The `++` operator concatenates two lists, producing a new list containing all elements of the left operand followed by all elements of the right operand.

r[expr.op.concat]
Both operands of the `++` operator MUST be lists. The result MUST be a list containing all elements of the left operand followed by all elements of the right operand.

#### Record Merge (`//`)

The `//` operator merges two records. Fields from the right operand take precedence when both records contain the same key.

r[expr.op.merge]
Both operands of the `//` operator MUST be records. The result MUST be a record containing all fields from both operands. If a field name appears in both operands, the value from the right operand MUST be used.

#### Field Access (`.`)

The `.` operator accesses a field on a record by name.

r[expr.op.field]
The left operand of the `.` operator MUST be a record and the right operand MUST be an identifier. The result MUST be the value of the named field. It is an error if the field does not exist.

#### Index Access (`[]`)

The `[]` operator accesses an element of a list by index.

r[expr.op.index]
The operand before `[` MUST be a list and the expression inside the brackets MUST evaluate to an unsigned integer. The result MUST be the element at the given zero-based index. It is an error if the index is out of bounds.

#### Equality (`==`) and Inequality (`!=`)

The `==` and `!=` operators compare two values for structural equality.

r[expr.op.eq]
The `==` operator MUST return `true` if the two operands are structurally equal and `false` otherwise. The `!=` operator MUST return the negation of `==`.

### Declaration Expressions

The `calendar`, `event`, and `task` keywords are expression forms that produce record values. They are syntactic sugar for constructing records with specific fields; the keyword itself carries no semantic weight beyond determining the desugared form.

Because they are expressions, they may appear anywhere an expression is expected: at the top level, inside lists, as the body of a `let` expression, or as an operand to `//` (merge) or other operators.

> r[decl.syntax+4]
> The grammar for declaration expressions is as follows:
>
> ```ebnf
> calendar expr = "calendar", record expr ;
>
> event expr = "event", name, short span, [ string literal | triple string literal ], [ record expr ]
>            | "event", record expr
>            ;
>
> task expr = "task", name, [ short dt ], [ string literal | triple string literal ], [ record expr ]
>           | "task", record expr
>           ;
>
> short span = short dt, [ duration literal ] ;
>
> short dt = date literal, time literal
>          | datetime literal
>          ;
> ```

#### Desugaring

Declaration expressions are purely syntactic sugar for record construction. Each keyword inserts a `type` field into the resulting record.

r[decl.calendar.desugar+2]
The calendar expression `calendar { ... }` MUST evaluate to a record containing all fields from the braces with `type` set to `"calendar"`.

r[decl.event.desugar+2]
The event expression `event { ... }` MUST evaluate to a record containing all fields from the braces with `type` set to `"event"`.

r[decl.task.desugar+2]
The task expression `task { ... }` MUST evaluate to a record containing all fields from the braces with `type` set to `"task"`.

#### Short-form Desugaring

The short forms for events and tasks desugar into their corresponding prefix form with a record expression.

r[decl.short-event.desugar+2]
The short event expression `event @name dt [dur] [str] [record]` MUST desugar into the prefix form `event { name: @name, start: dt, duration: dur, title: str, ...record }`, where `dt` is a datetime expression, `dur` is an optional duration literal, `str` is an optional string literal mapped to the `title` field, and `record` is an optional record expression whose fields are merged into the result. Fields from the short form take precedence when they overlap with fields in the trailing record.

r[decl.short-task.desugar+2]
The short task expression `task @name [dt] [str] [record]` MUST desugar into the prefix form `task { name: @name, due: dt, title: str, ...record }`, where `dt` is an optional datetime expression mapped to the `due` field, `str` is an optional string literal mapped to the `title` field, and `record` is an optional record expression whose fields are merged into the result.

## Record Types

Gnomon distinguishes specific record types for use in certain contexts. These types are identified by their fields (which may be mandatory or optional), and by the types of the values associated with those fields.

### Locations

A location represents a physical place associated with a calendar object. It is based on the Location object defined in JSCalendar.

> r[record.location.syntax]
> A location is a record with the following fields:
>
> | Field | Value Type | Meaning |
> |-------|-----------|---------|
> | `name` | string | The human-readable name or address of the location. |
> | `location_types` | A list of strings | The types of this location, such as `hotel` or `airport`. |
> | `coordinates` | string | A `geo:` URI (RFC 5870) identifying the geographic coordinates of the location. |
> | `links` | A list of Link records | Links to external resources describing the location. |
>
> All fields are optional.

### Virtual Locations

A virtual location represents a virtual meeting space or online platform. It is based on the VirtualLocation object defined in JSCalendar.

> r[record.virtual-location.syntax]
> A virtual location is a record with the following fields:
>
> | Field | Value Type | Meaning |
> |-------|-----------|---------|
> | `name` | string | The human-readable name of the virtual location. |
> | `uri` | string | The URI used to connect to the virtual location. |
> | `features` | A list of strings | The features available at this virtual location. |
>
> All fields except `uri` are optional.

r[record.virtual-location.uri]
A virtual location record MUST have a field named `uri` whose value is a string.

r[record.virtual-location.features]
If present, the `features` field on a virtual location MUST be a list of strings. Each string SHOULD be one of `audio`, `chat`, `feed`, `moderator`, `phone`, `screen`, or `video`.

### Links

A link represents an external resource associated with a calendar object. It is based on the Link object defined in JSCalendar.

> r[record.link.syntax]
> A link is a record with the following fields:
>
> | Field | Value Type | Meaning |
> |-------|-----------|---------|
> | `href` | string | The URI of the linked resource. |
> | `content_type` | string | The media type (RFC 6838) of the linked resource. |
> | `size` | Unsigned integer | The size of the linked resource in octets. |
> | `rel` | string | The relation type of the link, as per RFC 8288. |
> | `display` | A list of strings | How the linked resource is intended to be displayed. |
> | `title` | string | A human-readable title for the link. |
>
> All fields except `href` are optional.

r[record.link.href]
A link record MUST have a field named `href` whose value is a string.

r[record.link.display]
If present, the `display` field on a link MUST be a list of strings. Each string SHOULD be one of `badge`, `graphic`, `fullsize`, or `thumbnail`.

### Relations

A relation describes a relationship between a calendar object and another calendar object identified by its UID. It is based on the Relation object defined in JSCalendar.

> r[record.relation.syntax]
> A relation is a record with the following fields:
>
> | Field | Value Type | Meaning |
> |-------|-----------|---------|
> | `uid` | string | The UID of the related calendar object. |
> | `relation` | A list of strings | The types of relationship between the objects. |
>
> All fields except `uid` are optional.

r[record.relation.uid]
A relation record MUST have a field named `uid` whose value is a string.

r[record.relation.relation]
If present, the `relation` field on a relation MUST be a list of strings. Each string SHOULD be one of `first`, `next`, `child`, or `parent`.

### Participants

A participant represents an individual, group, or resource involved in a calendar object. It is based on the Participant object defined in JSCalendar.

> r[record.participant.syntax]
> A participant is a record with the following fields:
>
> | Field | Value Type | Meaning |
> |-------|-----------|---------|
> | `name` | string | The human-readable name of the participant. |
> | `email` | string | The email address of the participant. |
> | `description` | string | Additional information about the participant's role or how to contact them. |
> | `calendar_address` | string | A URI representing the participant's calendar address. |
> | `kind` | `individual` \| `group` \| `location` \| `resource` | The kind of entity the participant represents. |
> | `roles` | A list of strings | The roles the participant has in the calendar object. |
> | `participation_status` | `needs-action` \| `accepted` \| `declined` \| `tentative` \| `delegated` (default: `needs-action`) | The participation status of the participant. |
> | `expect_reply` | boolean (default: `false`) | Whether the participant is expected to reply. |
>
> All fields are optional.

r[record.participant.roles]
If present, the `roles` field on a participant MUST be a list of strings. Each string SHOULD be one of `owner`, `required`, `optional`, `informational`, or `chair`.

### Alerts

An alert represents a notification trigger for a calendar object. It is based on the Alert object defined in JSCalendar.

> r[record.alert.syntax]
> An alert is a record with the following fields:
>
> | Field | Value Type | Meaning |
> |-------|-----------|---------|
> | `trigger` | record | When the alert should be triggered. |
> | `action` | `display` \| `email` (default: `display`) | The action to take when the alert is triggered. |
>
> All fields except `trigger` are optional.

The trigger field describes when the alert fires. It can specify either a relative offset from the start of the calendar object, or an absolute point in time.

r[record.alert.trigger]
The `trigger` field on an alert MUST be a record. It MUST contain either an `offset` field or an `at` field, but not both.

r[record.alert.trigger.offset]
If the `trigger` record has an `offset` field, its value MUST be a duration. A negative duration indicates the alert fires before the start of the calendar object; a positive duration indicates the alert fires after the start.

r[record.alert.trigger.at]
If the `trigger` record has an `at` field, its value MUST be a local datetime representing the absolute time at which the alert fires.

### Events

Events represent scheduled amounts of time on a calendar; they are required to start at a certain point in time and usually have a non-zero duration. Every event must be identifiable by either a `name` or a `uid`.

r[record.event.name+2]
Records representing events MUST have a field named `name` whose value is a name, unless the record has a `uid` field.

r[record.event.start]
Records representing events MUST have a field named `start` whose value is a local datetime.

The `uid` field on events is always assigned a value. If omitted, it is derived per `r[model.calendar.uid.derivation]`.

r[record.event.uid+2]
Records representing events MUST have a field named `uid` whose value is a string. If the field is omitted in the source data, a UID is derived during calendar validation.

Events may also have the following optional fields:

r[record.event.duration]
If present, the `duration` field on an event MUST have a duration value. It represents the length of the event.

r[record.event.status]
If present, the `status` field on an event MUST have a string value of `tentative`, `confirmed`, or `cancelled`.

r[record.event.end-time-zone]
If present, the `end_time_zone` field on an event MUST have a string value that is a valid IANA time zone identifier. It specifies the time zone for the end of the event when it differs from the start.

### Tasks

Tasks represent action items, assignments, TODO items, or other similar objects. They can be given a specific relationship to time, but every task must be identifiable by either a `name` or a `uid`.

r[record.task.name+2]
Records representing tasks MUST have a field named `name` whose value is a name, unless the record has a `uid` field.

The `uid` field on tasks is always assigned a value. If omitted, it is derived per `r[model.calendar.uid.derivation]`.

r[record.task.uid+2]
Records representing tasks MUST have a field named `uid` whose value is a string. If the field is omitted in the source data, a UID is derived during calendar validation.

Tasks may also have the following optional fields:

r[record.task.due]
If present, the `due` field on a task MUST have a local datetime value. It represents the deadline by which the task should be completed.

r[record.task.start]
If present, the `start` field on a task MUST have a local datetime value. It represents the date and time at which the task should be started.

r[record.task.estimated-duration]
If present, the `estimated_duration` field on a task MUST have a duration value. It represents the estimated time required to complete the task.

r[record.task.percent-complete]
If present, the `percent_complete` field on a task MUST have an unsigned integer value in the range `0..=100`. It represents the percentage of the task that has been completed.

r[record.task.progress]
If present, the `progress` field on a task MUST have a string value of `needs-action`, `in-process`, `completed`, `failed`, or `cancelled`.

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

r[record.rrule.every.desugar.subject.weekday+2]
If the subject of an `every` expression is a weekday, the `frequency` field in the desugared record MUST be set to the value `weekly` and the `by_day` field in the desugared record MUST be set to the singleton list value `[{ day: D }]` where `D` is the weekday keyword itself.

For example, `every monday` desugars to a record with `frequency: "weekly"` and `by_day: [{ day: "monday" }]`.

r[record.rrule.every.desugar.terminator+2]
If the terminator of an `every` expression is given, its value (the datetime or integer literal) MUST be assigned to the `termination` field in the desugared record. If the terminator is omitted, the `termination` field in the desugared record MUST be omitted or set to `undefined`. If the terminator is a date literal, it MUST be treated as the corresponding datetime literal where the time component is `00:00:00`.

#### Evaluation

A recurrence rule defines an ordered sequence of local datetimes (occurrences) derived from a starting datetime and the rule's parameters. The expansion follows the RFC 5545 RRULE algorithm with the JSCalendar `skip` extension.

r[record.rrule.eval.expansion]
Expanding a recurrence rule MUST produce a sequence of local datetimes by applying the following pipeline for each recurrence period, starting from `dtstart`:

1. Seed the period by advancing `dtstart` by `interval` periods at the rule's `frequency`.
2. Apply each BY\* rule in order: `by_month`, `by_week_no`, `by_year_day`, `by_month_day`, `by_day`, `by_hour`, `by_minute`, `by_second`. Each BY\* rule either expands the candidate set (generates additional candidates) or limits it (filters candidates), depending on the frequency, per RFC 5545 Section 3.3.10.
3. Sort and deduplicate the candidates within the period.
4. Apply `by_set_position` to select specific positions from the sorted set.
5. Yield the resulting datetimes.

r[record.rrule.eval.dtstart]
The first occurrence in any recurrence expansion MUST be `dtstart` itself, regardless of whether it matches the BY\* filters.

r[record.rrule.eval.termination]
If `termination` is a count (unsigned integer), expansion MUST stop after that many occurrences have been yielded. If `termination` is a datetime, expansion MUST stop after yielding all occurrences up to and including that datetime. If `termination` is absent or `undefined`, the rule defines an infinite sequence.

r[record.rrule.eval.infinite]
Infinite recurrence rules (those without a `termination` value) are valid. Implementations MUST support them by evaluating only the occurrences relevant to the operation being performed (e.g., within a queried time range).

r[record.rrule.eval.start-required]
Expanding a recurrence rule requires a `start` field on the enclosing entry. It is an error if `start` is absent or is not a datetime record.

r[record.rrule.eval.empty]
An error SHOULD be produced if a recurrence rule is empty.

##### Period Advancement

Period advancement computes the seed datetime for a given period by adding `interval × period_index` units at the rule's frequency to `dtstart`.

r[record.rrule.eval.advance.yearly]
Yearly advancement MUST add `interval` years to `dtstart`. If the resulting month has fewer days than `dtstart`'s day, the day MUST be clamped to the last day of the month.

r[record.rrule.eval.advance.monthly]
Monthly advancement MUST add `interval` months to `dtstart` using absolute month arithmetic. If the resulting month has fewer days than `dtstart`'s day, the day MUST be clamped to the last day of the month.

r[record.rrule.eval.advance.weekly]
Weekly advancement MUST add `interval × 7` days to `dtstart`.

r[record.rrule.eval.advance.daily]
Daily advancement MUST add `interval` days to `dtstart`.

r[record.rrule.eval.advance.sub-daily]
Hourly, minutely, and secondly advancement MUST add the corresponding number of seconds (`interval × 3600`, `interval × 60`, or `interval`) to `dtstart`.

##### Expand/Limit Table

Each BY\* rule acts as either an expand (generate additional candidates) or a limit (filter candidates) depending on the rule's frequency. Some combinations are not applicable (N/A) and MUST be ignored.

> r[record.rrule.eval.table]
> The action for each BY\* rule at each frequency MUST be as follows:
>
> | BY\* Rule | YEARLY | MONTHLY | WEEKLY | DAILY | HOURLY | MINUTELY | SECONDLY |
> |-----------|--------|---------|--------|-------|--------|----------|----------|
> | `by_month` | Expand | Limit | Limit | Limit | Limit | Limit | Limit |
> | `by_week_no` | Expand | N/A | N/A | N/A | N/A | N/A | N/A |
> | `by_year_day` | Expand | N/A | N/A | Limit | Limit | Limit | Limit |
> | `by_month_day` | Expand | Expand | N/A | Limit | Limit | Limit | Limit |
> | `by_day` | \* | \*\* | Expand | Limit | Limit | Limit | Limit |
> | `by_hour` | Expand | Expand | Expand | Expand | Limit | Limit | Limit |
> | `by_minute` | Expand | Expand | Expand | Expand | Expand | Limit | Limit |
> | `by_second` | Expand | Expand | Expand | Expand | Expand | Expand | Limit |

r[record.rrule.eval.table.by-day-yearly]
For YEARLY frequency, `by_day` MUST act as Limit if `by_year_day` or `by_month_day` is present. Otherwise it MUST act as Expand.

r[record.rrule.eval.table.by-day-monthly]
For MONTHLY frequency, `by_day` MUST act as Limit if `by_month_day` is present. Otherwise it MUST act as Expand.

##### Negative Indexing

r[record.rrule.eval.negative.month-day]
A negative `by_month_day` value MUST count backward from the end of the month: `-1` is the last day, `-2` is the second-to-last day, and so on.

r[record.rrule.eval.negative.year-day]
A negative `by_year_day` value MUST count backward from the end of the year: `-1` is the last day (December 31 or December 30 in a 365-day year), `-2` is the second-to-last day, and so on.

r[record.rrule.eval.negative.weekday]
A negative `nth` on a `by_day` entry MUST count backward from the end of the applicable scope (month or year): `-1` is the last occurrence of that weekday, `-2` is the second-to-last, and so on.

##### Skip Strategies

When a BY\* expansion produces an invalid date (e.g., February 30), the `skip` field on the recurrence rule determines how to handle it. This follows the JSCalendar `skip` extension.

r[record.rrule.eval.skip.omit]
If `skip` is `omit`, invalid dates produced by expansion MUST be silently discarded.

r[record.rrule.eval.skip.forward]
If `skip` is `forward`, an invalid date MUST be replaced with the first day of the next month.

r[record.rrule.eval.skip.backward]
If `skip` is `backward`, an invalid date MUST be replaced with the last valid day of the same month.

r[record.rrule.eval.skip.default]
If `skip` is not specified, the default MUST be `omit`.

##### BYSETPOS

r[record.rrule.eval.by-set-pos]
`by_set_position` MUST select candidates by 1-based position from the sorted candidate set for each period. Positive values count from the start; negative values count from the end (`-1` is the last candidate). Zero values MUST be ignored.

##### BYWEEKNO and ISO Week Computation

r[record.rrule.eval.by-week-no]
`by_week_no` MUST be applied only at YEARLY frequency. Each value selects all dates within the corresponding ISO week of the year. Negative values count backward from the last week of the year.

r[record.rrule.eval.iso-week]
ISO week numbering MUST respect the `week_start` field on the recurrence rule. Week 1 is the first week containing at least 4 days of the new year. A year has 52 or 53 weeks depending on the weekday of January 1 and December 31.

##### BYDAY Expansion Scope

r[record.rrule.eval.by-day.monthly-expand]
When `by_day` expands at MONTHLY frequency, each entry without an `nth` MUST generate all occurrences of that weekday in the month. Each entry with an `nth` MUST generate the nth occurrence of that weekday in the month.

r[record.rrule.eval.by-day.yearly-expand]
When `by_day` expands at YEARLY frequency, the scope depends on other BY\* rules present: if `by_month` is present, expansion MUST be within each selected month; if `by_week_no` is present, expansion MUST be within each selected week; otherwise, expansion MUST be within the entire year.

##### Empty Period Retry

r[record.rrule.eval.retry]
If a recurrence period produces no candidates after expansion, the implementation MUST advance to subsequent periods. If 1000 consecutive periods produce no candidates, the implementation MUST stop iteration.

## Common Record Fields

The type constraints in this section apply to events and tasks unless otherwise specified. Gnomon records are open — any field may appear on any record. This section defines type restrictions for known fields on known record types, not an exhaustive list of permitted fields.

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

### `time_zone`
Name: `time_zone`

Value: string (no default)

Meaning: The IANA time zone identifier (e.g. `"America/New_York"`) providing the default time zone context for date/time values in this object.

r[field.time-zone.type]
If present, the `time_zone` field MUST have a string value that is a valid IANA time zone identifier.

### `priority`
Name: `priority`

Value: unsigned integer (default: `0`)

Meaning: The priority of the object. A value of `0` means the priority is undefined. A value of `1` is the highest priority and `9` is the lowest.

r[field.priority.type]
If present, the `priority` field MUST have an unsigned integer value in the range `0..=9`.

### `privacy`
Name: `privacy`

Value: string (default: `"public"`)

Meaning: The sharing restriction level of the object. A value of `public` means the object is fully visible, `private` means only free/busy information is visible to others, and `secret` means the object is not visible at all to others.

r[field.privacy.type]
If present, the `privacy` field MUST have a string value of `public`, `private`, or `secret`.

### `free_busy_status`
Name: `free_busy_status`

Value: string (default: `"busy"`)

Meaning: How this object affects free/busy time calculations.

r[field.free-busy-status.type]
If present, the `free_busy_status` field MUST have a string value of `free` or `busy`.

### `show_without_time`
Name: `show_without_time`

Value: boolean (default: `false`)

Meaning: Whether the time component of the object is unimportant for display purposes. When `true`, the object SHOULD be displayed as an all-day item.

r[field.show-without-time.type]
If present, the `show_without_time` field MUST have a boolean value.

### `color`
Name: `color`

Value: string (no default)

Meaning: A CSS3 color value (e.g. `"#ff0000"`, `"rebeccapurple"`) to use when displaying the object.

r[field.color.type]
If present, the `color` field MUST have a string value.

### `keywords`
Name: `keywords`

Value: list of strings (no default)

Meaning: A set of tags or keywords associated with the object.

r[field.keywords.type]
If present, the `keywords` field MUST be a list of string values.

### `categories`
Name: `categories`

Value: list of strings (no default)

Meaning: The categories to which the object belongs.

r[field.categories.type]
If present, the `categories` field MUST be a list of string values.

### `locale`
Name: `locale`

Value: string (no default)

Meaning: A BCP 47 language tag (e.g. `"en-US"`, `"ja"`) identifying the language of the text fields in the object.

r[field.locale.type]
If present, the `locale` field MUST have a string value that is a valid BCP 47 language tag.

### `locations`
Name: `locations`

Value: list of Location records (no default)

Meaning: The physical locations associated with the object.

r[field.locations.type]
If present, the `locations` field MUST be a list of Location records.

### `virtual_locations`
Name: `virtual_locations`

Value: list of VirtualLocation records (no default)

Meaning: The virtual meeting spaces or online platforms associated with the object.

r[field.virtual-locations.type]
If present, the `virtual_locations` field MUST be a list of VirtualLocation records.

### `links`
Name: `links`

Value: list of Link records (no default)

Meaning: External resources associated with the object.

r[field.links.type]
If present, the `links` field MUST be a list of Link records.

### `related_to`
Name: `related_to`

Value: list of Relation records (no default)

Meaning: Relationships between this object and other calendar objects.

r[field.related-to.type]
If present, the `related_to` field MUST be a list of Relation records.

### `participants`
Name: `participants`

Value: list of Participant records (no default)

Meaning: The people, groups, or resources involved in the object.

r[field.participants.type]
If present, the `participants` field MUST be a list of Participant records.

### `alerts`
Name: `alerts`

Value: list of Alert records (no default)

Meaning: Notifications that should be triggered in relation to the object.

r[field.alerts.type]
If present, the `alerts` field MUST be a list of Alert records.

### `recur`
Name: `recur`

Value: recurrence rule record (no default)

Meaning: The recurrence rule for the event or task. Only a single recurrence rule is permitted per object, following JSCalendar's simplification over iCalendar's multi-rule model (a union of multiple recurrence rules can always be expressed as a single rule).

r[field.recur.type]
If present, the `recur` field on an event or task MUST be a recurrence rule record.

## Data Model

Evaluating a Gnomon expression produces a Gnomon value. The value types are strings, integers, signed integers, booleans, records, lists, names, and `undefined`. These types are defined by the expression grammar and carry no intrinsic semantic meaning.

Specific contexts impose shape expectations on the values they receive. The `check` subcommand, for example, expects a value that conforms to the calendar shape; it is an error if evaluation produces something else. This section defines the shapes that the Gnomon tooling recognizes.

### Calendars

A calendar is the primary output of Gnomon evaluation. It is a record representing a collection of calendar entries (events and tasks) together with associated metadata.

> r[model.calendar.uid]
> A calendar record MUST have a field named `uid` whose value is a string.

The `uid` field is the sole mandatory field on a calendar. It serves as the namespace for deterministic UID derivation: any event or task that omits an explicit `uid` receives a UUIDv5 computed from the calendar's `uid` as the namespace and the object's `name` as the key.

> r[model.calendar.uid.derivation]
> When an event or task omits a `uid` field, a UID MUST be derived as `UUIDv5(calendar_uid, name)`, where `calendar_uid` is the value of the `uid` field on the enclosing calendar and `name` is the string representation of the object's `name` field.

> r[model.calendar.uid.derivation.non-uuid]
> If the calendar's `uid` value is not a valid UUID, UID derivation MUST be skipped and a warning diagnostic MUST be produced. The calendar and its entries remain valid; only the automatic derivation is suppressed.

The `uid` field follows RFC 5545 in accepting any string. However, implementations SHOULD produce a warning when a `uid` value in Gnomon source code is not a valid UUID, as non-UUID identifiers limit interoperability and prevent UID derivation.

A calendar may have additional optional metadata fields such as `title`, `description`, `time_zone`, `color`, or other properties. These are not enumerated exhaustively; as with all Gnomon records, calendars are open.

> r[model.calendar.singular+2]
> A calendar project MUST contain exactly one record with `type` set to `"calendar"` among its top-level values. It is an error if no such record is found. It is an error if more than one is found.

### Calendar Entries

A calendar record MUST have a field named `entries` whose value is a list of records. Each entry in the list represents an event or a task.

> r[model.calendar.entries]
> A calendar record MUST have a field named `entries` whose value is a list of records.

> r[model.entry.type]
> Each record in the `entries` list MUST have a field named `type` whose value is `"event"` or `"task"`.

The `type` field distinguishes calendar records, events, and tasks within the top-level list. When a `calendar`, `event`, or `task` expression is used, the corresponding `type` field is inserted automatically. A user may also write the `type` field explicitly.

> r[model.entry.type.infer+2]
> A `calendar` expression MUST produce a record with `type` set to `"calendar"`. An `event` expression MUST produce a record with `type` set to `"event"`. A `task` expression MUST produce a record with `type` set to `"task"`.

Once the `type` field is known, the entry is validated against the corresponding record type definition (see Events and Tasks under Record Types). The remaining field constraints — mandatory fields, optional field types, and common record fields — apply as specified in those sections.

### Names

Names serve as human-readable identifiers for calendar entries.

> r[model.name.unique]
> Within a single calendar, no two entries MAY share the same `name` value. Events and tasks share a single namespace.

A name resolves uniquely without requiring additional type information.

### Import Resolution

An `import` expression evaluates a referenced source file and produces a Gnomon value. The source may be a Gnomon file (evaluated recursively), an iCalendar file, a JSCalendar file, or another supported format. The result is a value in the Gnomon data model: a record, a list of records, or any other Gnomon value depending on the source content.

> r[model.import.resolution]
> Evaluating an `import` expression MUST produce a Gnomon value. For foreign formats, the source data MUST be translated into the Gnomon data model.

> r[model.import.transparent]
> Import expressions are transparent: after evaluation, the result is an ordinary Gnomon value with no trace of its origin. There is no distinct "import" value in the data model.

### Foreign Format Translation

When an import source is in a foreign format, it MUST be translated into the Gnomon data model according to the rules in this section.

> r[model.import.preserve]
> Foreign format translation MUST NOT silently discard properties. All properties on the source object MUST be translated into the resulting record, even if no explicit mapping is defined.

#### iCalendar Translation

> r[model.import.icalendar.components]
> An iCalendar import MUST translate `VEVENT` components into event records and `VTODO` components into task records. `VJOURNAL`, `VFREEBUSY`, `VTIMEZONE`, and all other component types MUST be silently skipped.

> r[model.import.icalendar.calendar]
> An iCalendar import MUST produce a list of calendar records, one per VCALENDAR object. Each calendar record MUST translate VCALENDAR-level properties into fields and place translated component records in an `entries` field. The following VCALENDAR properties MUST be mapped:
>
> | iCalendar Property | Gnomon Field | Type |
> |--------------------|-------------|------|
> | (implicit) | `type` | `"calendar"` |
> | `UID` (RFC 7986) | `uid` | string |
> | `NAME` (RFC 7986) | `name` | string (first value) |
> | `DESCRIPTION` (RFC 7986) | `description` | string (first value) |
> | `COLOR` (RFC 7986) | `color` | string |
> | `URL` (RFC 7986) | `url` | string |
> | `CATEGORIES` (RFC 7986) | `categories` | list of strings |
> | `PRODID` | `prod_id` | string |
> | `LAST-MODIFIED` (RFC 7986) | `last_modified` | datetime record |
> | `REFRESH-INTERVAL` (RFC 7986) | `refresh_interval` | duration record |
> | `SOURCE` (RFC 7986) | `source` | string |

> r[model.import.icalendar.event]
> A `VEVENT` component MUST be translated to a record with the following field mapping:
>
> | iCalendar Property | Gnomon Field | Type |
> |--------------------|-------------|------|
> | (implicit) | `type` | `"event"` |
> | `UID` | `uid` | string |
> | `SUMMARY` | `title` | string |
> | `DESCRIPTION` | `description` | string |
> | `DTSTART` | `start` | datetime or date record |
> | `DTSTART` TZID parameter | `time_zone` | string |
> | `DURATION` | `duration` | duration record |
> | `STATUS` | `status` | string (see status mapping) |
> | `PRIORITY` | `priority` | integer 0–9 (see priority mapping) |
> | `LOCATION` | `location` | string |
> | `COLOR` | `color` | string |
> | `CATEGORIES` | `categories` | list of strings |
> | `DTSTAMP` | `dtstamp` | datetime record |
> | `CLASS` | `class` | string (lowercase) |
> | `CREATED` | `created` | datetime record |
> | `GEO` | `geo` | record `{ latitude, longitude }` (strings) |
> | `LAST-MODIFIED` | `last_modified` | datetime record |
> | `ORGANIZER` | `organizer` | string (URI) |
> | `SEQUENCE` | `sequence` | integer |
> | `TRANSP` | `transparency` | string (lowercase) |
> | `URL` | `url` | string |
> | `RECURRENCE-ID` | `recurrence_id` | datetime or date record |
> | `RRULE` | `recur` | recurrence rule record |
> | `RDATE` | `rdates` | list of datetime/date records |
> | `EXDATE` | `exdates` | list of datetime/date records |
> | `ATTACH` | `attachments` | list of strings (URIs) or records |
> | `ATTENDEE` | `attendees` | list of strings (URIs) |
> | `COMMENT` | `comments` | list of strings |
> | `CONTACT` | `contacts` | list of strings |
> | `RELATED-TO` | `related_to` | list of strings (UIDs) |
> | `RESOURCES` | `resources` | list of lists of strings |
> | `IMAGE` | `images` | list of strings (URIs) or records |
> | `CONFERENCE` | `conferences` | list of strings (URIs) |
> | `REQUEST-STATUS` | `request_statuses` | list of strings |

> r[model.import.icalendar.event.duration-fallback]
> If a `VEVENT` has `DTSTART` and `DTEND` but no `DURATION`, the duration MUST be computed as the difference between `DTEND` and `DTSTART`.

> r[model.import.icalendar.task]
> A `VTODO` component MUST be translated to a record with the following field mapping:
>
> | iCalendar Property | Gnomon Field | Type |
> |--------------------|-------------|------|
> | (implicit) | `type` | `"task"` |
> | `UID` | `uid` | string |
> | `SUMMARY` | `title` | string |
> | `DESCRIPTION` | `description` | string |
> | `DUE` | `due` | datetime or date record |
> | `DTSTART` | `start` | datetime or date record |
> | `DTSTART` TZID parameter | `time_zone` | string |
> | `DURATION` | `estimated_duration` | duration record |
> | `PERCENT-COMPLETE` | `percent_complete` | integer |
> | `STATUS` | `status` | string (see status mapping) |
> | `PRIORITY` | `priority` | integer 0–9 (see priority mapping) |
> | `LOCATION` | `location` | string |
> | `COLOR` | `color` | string |
> | `CATEGORIES` | `categories` | list of strings |
> | `DTSTAMP` | `dtstamp` | datetime record |
> | `CLASS` | `class` | string (lowercase) |
> | `CREATED` | `created` | datetime record |
> | `GEO` | `geo` | record `{ latitude, longitude }` (strings) |
> | `LAST-MODIFIED` | `last_modified` | datetime record |
> | `ORGANIZER` | `organizer` | string (URI) |
> | `SEQUENCE` | `sequence` | integer |
> | `URL` | `url` | string |
> | `COMPLETED` | `completed` | datetime record |
> | `RECURRENCE-ID` | `recurrence_id` | datetime or date record |
> | `RRULE` | `recur` | recurrence rule record |
> | `RDATE` | `rdates` | list of datetime/date records |
> | `EXDATE` | `exdates` | list of datetime/date records |
> | `ATTACH` | `attachments` | list of strings (URIs) or records |
> | `ATTENDEE` | `attendees` | list of strings (URIs) |
> | `COMMENT` | `comments` | list of strings |
> | `CONTACT` | `contacts` | list of strings |
> | `RELATED-TO` | `related_to` | list of strings (UIDs) |
> | `RESOURCES` | `resources` | list of lists of strings |
> | `IMAGE` | `images` | list of strings (URIs) or records |
> | `CONFERENCE` | `conferences` | list of strings (URIs) |
> | `REQUEST-STATUS` | `request_statuses` | list of strings |

> r[model.import.icalendar.status]
> iCalendar status values MUST be translated to lowercase strings: `TENTATIVE` → `"tentative"`, `CONFIRMED` → `"confirmed"`, `CANCELLED` → `"cancelled"`, `NEEDS-ACTION` → `"needs-action"`, `COMPLETED` → `"completed"`, `IN-PROCESS` → `"in-process"`, `DRAFT` → `"draft"`, `FINAL` → `"final"`. Unrecognized status values MUST be translated to `"unknown"`.

> r[model.import.icalendar.priority]
> iCalendar priority values (1–9) MUST be translated to integers in the range 0–9. Priority 0 (undefined) maps to 0. Values 1–3 map to 1–3. Values 4–6 map to 4–6. Values 7–9 map to 7–9.

> r[model.import.icalendar.rrule]
> An iCalendar `RRULE` property MUST be translated to a recurrence rule record using the gnomon recurrence rule schema. If multiple `RRULE` properties exist, only the first is used.

> r[model.import.icalendar.extension]
> Extension properties (X-prefixed properties per RFC 5545) on VCALENDAR, VEVENT, and VTODO components MUST be preserved in the translated record. Property names MUST be lowercased and hyphens replaced with underscores. Property values MUST be translated to their natural Gnomon type (string, integer, boolean, datetime, date, or duration as appropriate).

#### JSCalendar Translation

> r[model.import.jscalendar.types]
> A JSCalendar import MUST translate `Event` objects into event records and `Task` objects into task records. `Group` objects MUST be flattened: each entry in the group is translated individually.

> r[model.import.jscalendar.event]
> A JSCalendar `Event` MUST be translated to a record with the following field mapping:
>
> | JSCalendar Property | Gnomon Field | Type |
> |---------------------|-------------|------|
> | (implicit) | `type` | `"event"` |
> | `uid` | `uid` | string |
> | `title` | `title` | string |
> | `description` | `description` | string |
> | `start` | `start` | datetime record |
> | `duration` | `duration` | duration record |
> | `timeZone` | `time_zone` | string |
> | `status` | `status` | string |
> | `priority` | `priority` | integer 0–9 |
> | `color` | `color` | string |
> | `locale` | `locale` | string |
> | `privacy` | `privacy` | string |
> | `freeBusyStatus` | `free_busy_status` | string |
> | `showWithoutTime` | `show_without_time` | boolean |
> | `categories` | `categories` | list of strings |
> | `keywords` | `keywords` | list of strings |

> r[model.import.jscalendar.task]
> A JSCalendar `Task` MUST be translated to a record with the following field mapping:
>
> | JSCalendar Property | Gnomon Field | Type |
> |---------------------|-------------|------|
> | (implicit) | `type` | `"task"` |
> | `uid` | `uid` | string |
> | `title` | `title` | string |
> | `description` | `description` | string |
> | `start` | `start` | datetime record |
> | `due` | `due` | datetime record |
> | `estimatedDuration` | `estimated_duration` | duration record |
> | `percentComplete` | `percent_complete` | integer |
> | `progress` | `progress` | string |
> | `timeZone` | `time_zone` | string |
> | `priority` | `priority` | integer 0–9 |
> | `color` | `color` | string |
> | `locale` | `locale` | string |
> | `privacy` | `privacy` | string |
> | `freeBusyStatus` | `free_busy_status` | string |
> | `showWithoutTime` | `show_without_time` | boolean |
> | `categories` | `categories` | list of strings |
> | `keywords` | `keywords` | list of strings |

> r[model.import.jscalendar.vendor]
> Vendor-specific properties (property names not defined by RFC 9553) on JSCalendar objects MUST be preserved in the translated record. JSON values MUST be translated recursively: objects to records, arrays to lists, strings to strings, numbers to integers or signed integers, booleans to booleans, and null to `undefined`.

> r[model.import.jscalendar.priority]
> JSCalendar priority values MUST be translated to integers in the range 0–9, using the same mapping as iCalendar priorities.

### Shape-checking

Shape-checking is the process of validating that a Gnomon value conforms to a recognized shape (calendar, event, task, recurrence rule, or any other record type defined in this specification). It enforces mandatory field presence, field value types, and value restrictions.

> r[model.shape.diagnostic]
> Shape-checking MUST report all constraint violations as diagnostics rather than aborting on the first error. A value that fails shape-checking MUST still be preserved to the greatest extent possible.

Shape-checking applies the constraints defined in the Record Types and Common Record Fields sections of this specification. The following invariants are enforced:

> r[model.shape.required]
> If a record type specifies a mandatory field, shape-checking MUST produce a diagnostic if that field is absent.

> r[model.shape.type]
> If a field constraint specifies the type of a field's value, shape-checking MUST produce a diagnostic if the field is present and its value does not conform to the specified type.

> r[model.shape.enum]
> If a field constraint restricts a field's value to a set of permitted values, shape-checking MUST produce a diagnostic if the field is present and its value is not in the permitted set.

Records are open: shape-checking does not reject fields that are not mentioned in the specification. Unknown fields are preserved without type constraints.

> r[model.shape.open]
> Shape-checking MUST NOT reject a record for containing fields not listed in its record type definition. Unknown fields MUST be preserved in the shape-checked output.

Shape-checking is applied recursively: if a field's expected type is itself a record type (e.g., a location, alert, or recurrence rule), the nested record is shape-checked against its own definition.

> r[model.shape.recursive]
> When a field's value is expected to be a record of a specific type, that record MUST itself be shape-checked against the corresponding record type definition.

## File Structure

A Gnomon source file consists of zero or more `let` bindings followed by a body. The body is a sequence of expressions. If the body begins with a declaration keyword (`calendar`, `event`, or `task`), the file is in list mode: all top-level expressions are collected into a list. Otherwise, the body is a single expression whose value is the file's result.

> r[syntax.start+3]
> Source data MUST be parsed according to the following grammar:
>
> ```ebnf
> START = { let binding }, body ;
> let binding = "let", identifier, "=", expr ;
> body = expr, { expr }
>      | (* empty *)
>      ;
> ```

r[syntax.file.let]
Let bindings at the file level MUST precede all expressions. They are in scope for the entire body.

r[syntax.file.body+2]
If the body is empty, the file evaluates to an empty list. If the body begins with a `calendar`, `event`, or `task` keyword, the file is in list mode: each top-level expression is evaluated and the results are collected into a list. Otherwise, the body MUST consist of a single expression, and the file evaluates to that expression's value.

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

#### `check`

The `check` subcommand is a subcommand of the root command; it takes a single file path as a parameter, treated as the root of a calendar project. When executed, `gnomon check <file>` will parse, evaluate, and validate the file and all transitively imported files as a calendar. This subsumes both syntax checking and calendar validation in a single step.

r[cli.subcommand.check+2]
The program MUST provide a `check` subcommand for the root command which takes a single parameter describing a file path. The file is treated as the root of a calendar project.

r[cli.subcommand.check.no-file+2]
If the file path argument to the `check` subcommand cannot be resolved to a file for any reason, the program MUST produce an error.

r[cli.subcommand.check.output+2]
If a file was successfully located, the program MUST evaluate the file (transitively resolving imports), validate the result as a calendar (checking uniqueness constraints, shape conformance, UID derivation, and recurrence expansion), and report all diagnostics to STDERR. The program MUST exit with a non-zero exit code if any errors were found. The `check` subcommand MUST NOT produce output on STDOUT.

r[cli.subcommand.check.unused]
After evaluation, the program MUST recursively scan the root file's parent directory for files matching `*.gnomon`. Any such file that is not the root file and was not transitively imported by the root file MUST produce a warning diagnostic.

r[cli.subcommand.check.refresh]
The `check` subcommand MUST accept a `--refresh` option. When present, all URI imports MUST be re-fetched from the network, bypassing the cache. The fetched content MUST still be written to the cache.

#### `eval`

The `eval` subcommand is a subcommand of the root command; it takes either a file path or an inline expression as input. When executed, `gnomon eval <file>` or `gnomon eval --expr '<expression>'` will parse, validate, and evaluate the input, producing the resulting Gnomon value.

r[cli.subcommand.eval]
The program MUST provide an `eval` subcommand for the root command which takes a single parameter describing a file path.

r[cli.subcommand.eval.expr]
The `eval` subcommand MUST accept a `--expr` option whose value is a Gnomon expression string.

r[cli.subcommand.eval.expr.exclusive]
The `--expr` option and the file path argument MUST be mutually exclusive; exactly one MUST be provided.

r[cli.subcommand.eval.no-file]
If the file path argument to the `eval` subcommand cannot be resolved to a file for any reason, the program MUST produce an error.

r[cli.subcommand.eval.refresh]
The `eval` subcommand MUST accept a `--refresh` option. When present, all URI imports MUST be re-fetched from the network, bypassing the cache. The fetched content MUST still be written to the cache.

r[cli.subcommand.eval.output]
The program MUST write a textual representation of the evaluated value to STDOUT. Any diagnostics MUST be written to STDERR.

The textual representation of a value is defined as follows:

r[cli.subcommand.eval.output.string]
A string value MUST be rendered as its contents surrounded by double quotes, with `"`, `\`, newline, and tab characters escaped as `\"`, `\\`, `\n`, and `\t` respectively.

r[cli.subcommand.eval.output.integer]
An integer value MUST be rendered as its decimal representation.

r[cli.subcommand.eval.output.bool]
A boolean value MUST be rendered as `true` or `false`.

r[cli.subcommand.eval.output.undefined]
The undefined value MUST be rendered as `undefined`.

r[cli.subcommand.eval.output.name]
A name value MUST be rendered with an `@` prefix.

r[cli.subcommand.eval.output.list]
A list value MUST be rendered as a comma-separated sequence of values enclosed in brackets (`[`, `]`).

r[cli.subcommand.eval.output.record]
A record value MUST be rendered as a brace-enclosed block with one `key: value,` pair per line, indented by 4 spaces per nesting level. Fields MUST be sorted lexicographically by key. An empty record MUST be rendered as `{}`.

#### `clean`

The `clean` subcommand removes all cached URI imports from the local cache directory.

r[cli.subcommand.clean]
The program MUST provide a `clean` subcommand for the root command. When executed, it MUST remove all entries from the URI import cache directory and print the number of entries removed to STDOUT.

#### `repl`

The `repl` subcommand starts an interactive read-eval-print loop. Each line of input is
parsed and evaluated as a Gnomon expression; the resulting value is printed to STDOUT.
Let bindings accumulate across inputs, and meta-commands prefixed with `:` control the
session.

r[cli.subcommand.repl]
The program MUST provide a `repl` subcommand for the root command.

r[cli.subcommand.repl.prompt]
The REPL MUST display the prompt `gnomon> ` when waiting for input.

r[cli.subcommand.repl.prompt.continuation]
When the input is incomplete (unclosed delimiters), the REPL MUST display the continuation prompt `  ...> ` and wait for additional input.

r[cli.subcommand.repl.eval]
Each complete input MUST be parsed and evaluated as a Gnomon source file body (supporting expressions, let bindings, and declarations). The resulting value MUST be printed to STDOUT using the same rendering rules as the `eval` subcommand.

r[cli.subcommand.repl.let-persist]
Top-level `let` bindings from previous inputs MUST remain in scope for subsequent inputs.

r[cli.subcommand.repl.import-cwd]
Import paths in REPL input MUST be resolved relative to the current working directory.

r[cli.subcommand.repl.diagnostics]
Parse errors and evaluation diagnostics MUST be printed to STDERR. An input that produces errors MUST NOT modify the persistent let-binding environment.

r[cli.subcommand.repl.meta.reset]
The REPL MUST support a `:reset` meta-command that clears all persistent let bindings.

r[cli.subcommand.repl.meta.type]
The REPL MUST support a `:type <expr>` meta-command that evaluates the expression and prints its type name (e.g. `string`, `integer`, `record`, `list`) to STDOUT.

r[cli.subcommand.repl.meta.parse]
The REPL MUST support a `:parse <expr>` meta-command that parses the expression and prints the debug syntax tree to STDOUT without evaluating it.

r[cli.subcommand.repl.meta.help]
The REPL MUST support a `:help` meta-command that prints a list of available meta-commands.

r[cli.subcommand.repl.meta.quit]
The REPL MUST support `:quit` and `:q` meta-commands to exit the REPL. Pressing Ctrl-D (EOF) MUST also exit.

r[cli.subcommand.repl.multiline]
The REPL MUST detect incomplete input by tracking unmatched opening delimiters (`{`, `[`, `(`) in the token stream and continue reading on the next line.

r[cli.subcommand.repl.history]
The REPL MUST support line-editing history across the session.

#### Reserved Subcommands

We reserve some identifiers for future use as subcommands.

> r[cli.subcommand.reserved+4]
> The following identifiers MUST NOT be used by any implementation:
>
> - `about`
> - `compile`
> - `daemon`
> - `fetch`
> - `lsp`
> - `merge`
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
