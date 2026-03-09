mod expand;
mod iter;
mod table;
pub mod types;
mod util;

pub use types::*;

pub struct Occurrences {
    rule: RecurrenceRule,
    dtstart: DateTime,
}

impl Occurrences {
    pub fn new(rule: RecurrenceRule, dtstart: DateTime) -> Self {
        Self { rule, dtstart }
    }

    pub fn count(&self) -> Option<u64> {
        match &self.rule.termination {
            Termination::Count(n) => Some(*n),
            _ => None,
        }
    }

    pub fn is_finite(&self) -> bool {
        !matches!(self.rule.termination, Termination::None)
    }

    pub fn iter(&self) -> iter::OccurrenceIter<'_> {
        iter::OccurrenceIter::new(&self.rule, self.dtstart)
    }
}

impl IntoIterator for Occurrences {
    type Item = DateTime;
    type IntoIter = iter::OwnedOccurrenceIter;

    fn into_iter(self) -> Self::IntoIter {
        iter::OwnedOccurrenceIter::new(self.rule, self.dtstart)
    }
}
