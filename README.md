# Gnomon

> **Warning:** Gnomon is under active development and is not yet ready for
> production use. The language, CLI, and output formats are all subject to
> breaking changes. Expect missing features and rough edges.

Gnomon is a plaintext language for authoring and maintaining calendars,
designed to compile to [iCalendar](https://datatracker.ietf.org/doc/html/rfc5545)
and [JSCalendar](https://datatracker.ietf.org/doc/html/rfc8984).

## Installation

Gnomon is written in Rust. To build from source:

```sh
git clone https://github.com/eikopf/gnomon.git
cd gnomon
cargo install --path .
```

This places the `gnomon` binary in your Cargo bin directory (usually `~/.cargo/bin`).

## Examples

You can define a calendar entirely in a single file:

```gnomon
;; in main.gnomon

calendar {
    uid: "31169090-6d76-4774-aace-30bc978b1102",
    title: "Work",
    time_zone: "America/New_York",
    entries: [
        event @standup 2026-03-11T09:00 30m "Daily Standup" {
            recur: every day until 2026-12-31,
        },

        event @board-meeting 2026-03-20T10:00 2h "Board Meeting",
        
        task @taxes 2026-04-15T23:59 "File Taxes" {
            priority: 1,
        }
    ]
}
```

Or you can split it across several files:

```gnomon
;; in main.gnomon

calendar {
    uid: "31169090-6d76-4774-aace-30bc978b1102",
    time_zone: "America/New_York",
    entries: import ./2026.gnomon
}
```

```gnomon
;; in 2026.gnomon

event @standup 2026-03-11T09:00 30m "Daily Standup" {
    recur: every day until 2026-12-31,
}

event @board-meeting 2026-03-20T10:00 2h "Board Meeting"

task @taxes 2026-04-15T23:59 "File Taxes" {
    priority: 1,
}
```


Then check the root file for errors:

```sh
gnomon check main.gnomon
```

Or evaluate it to see the resolved output:

```sh
gnomon eval main.gnomon
```

## Language Overview

### Records and Lists

Gnomon's core data types are records (key-value maps) and lists:

```gnomon
let config = { theme: "dark", font_size: 14 }
let tags = ["work", "urgent", "recurring"]
```

### Literals

Gnomon has built-in syntax for dates, times, datetimes, and durations:

```gnomon
let d = 2026-07-04
let t = 09:30
let dt = 2026-07-04T09:30
let dur = 1h30m
```

### Imports

Files can import other Gnomon files, iCalendar (`.ics`), or JSCalendar (`.json`):

```gnomon
let holidays = import <https://calendars.icloud.com/holidays/us_en.ics> in
let local = import ./events.gnomon in

[holidays, local]
```

### Recurrence

The `every` keyword creates recurrence rules:

```gnomon
event @standup 2026-01-05T09:00 30m "Standup" {
    recur: every day until 2026-12-31,
}

event @retro 2026-01-06T14:00 1h "Retrospective" {
    recur: every monday,
}

event @fireworks 2026-07-04T20:00 1h "Fireworks" {
    recur: every year on 07-04,
}
```

### Merge and Concatenation

Records can be merged with `//` (right side wins on conflicts) and lists
concatenated with `++`:

```gnomon
let defaults = { time_zone: "UTC", priority: 5 }
let custom = defaults // { priority: 1 }

let all-events = import ./work.gnomon ++ import ./personal.gnomon
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `gnomon parse <file>` | Print the raw syntax tree |
| `gnomon eval <file>` | Evaluate a file and print the result |
| `gnomon eval --expr '<expr>'` | Evaluate an inline expression |
| `gnomon check <file>` | Validate a calendar project |
| `gnomon repl` | Start an interactive session |
| `gnomon clean` | Clear the URI import cache |

## Specification

The full language specification is in [`spec/gnomon.md`](spec/gnomon.md).

## License

See [LICENSE](LICENSE).
