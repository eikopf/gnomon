//! Compilation of Gnomon values to foreign calendar formats.
//!
//! Supports iCalendar (RFC 5545) via `calico` and JSCalendar (RFC 9553) as JSON.
//!
//! This crate is salsa-free — it consumes [`ImportValue`](gnomon_import::ImportValue) trees
//! (the same intermediate form used by `gnomon-import`) and produces serialized output.

mod ical;
mod jscal;

pub use ical::emit_icalendar;
pub use jscal::emit_jscalendar;
