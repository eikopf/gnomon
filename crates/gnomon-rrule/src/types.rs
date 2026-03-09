pub type DateTime = jiff::civil::DateTime;
pub type Date = jiff::civil::Date;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Frequency {
    Yearly,
    Monthly,
    Weekly,
    Daily,
    Hourly,
    Minutely,
    Secondly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub fn to_jiff(self) -> jiff::civil::Weekday {
        match self {
            Self::Monday => jiff::civil::Weekday::Monday,
            Self::Tuesday => jiff::civil::Weekday::Tuesday,
            Self::Wednesday => jiff::civil::Weekday::Wednesday,
            Self::Thursday => jiff::civil::Weekday::Thursday,
            Self::Friday => jiff::civil::Weekday::Friday,
            Self::Saturday => jiff::civil::Weekday::Saturday,
            Self::Sunday => jiff::civil::Weekday::Sunday,
        }
    }

    pub fn from_jiff(w: jiff::civil::Weekday) -> Self {
        match w {
            jiff::civil::Weekday::Monday => Self::Monday,
            jiff::civil::Weekday::Tuesday => Self::Tuesday,
            jiff::civil::Weekday::Wednesday => Self::Wednesday,
            jiff::civil::Weekday::Thursday => Self::Thursday,
            jiff::civil::Weekday::Friday => Self::Friday,
            jiff::civil::Weekday::Saturday => Self::Saturday,
            jiff::civil::Weekday::Sunday => Self::Sunday,
        }
    }

    /// Days since Monday (0 = Monday, 6 = Sunday).
    pub fn days_since_monday(self) -> i32 {
        match self {
            Self::Monday => 0,
            Self::Tuesday => 1,
            Self::Wednesday => 2,
            Self::Thursday => 3,
            Self::Friday => 4,
            Self::Saturday => 5,
            Self::Sunday => 6,
        }
    }

    pub const ALL: [Weekday; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NDay {
    pub day: Weekday,
    pub nth: Option<i8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByMonth {
    pub month: u8,
    pub leap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skip {
    #[default]
    Omit,
    Backward,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Termination {
    None,
    Count(u64),
    Until(DateTime),
}

impl Default for Termination {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone)]
pub struct RecurrenceRule {
    pub frequency: Frequency,
    pub interval: u32,
    pub skip: Skip,
    pub week_start: Weekday,
    pub termination: Termination,
    pub by_day: Vec<NDay>,
    pub by_month_day: Vec<i8>,
    pub by_month: Vec<ByMonth>,
    pub by_year_day: Vec<i16>,
    pub by_week_no: Vec<i8>,
    pub by_hour: Vec<u8>,
    pub by_minute: Vec<u8>,
    pub by_second: Vec<u8>,
    pub by_set_position: Vec<i32>,
}

impl Default for RecurrenceRule {
    fn default() -> Self {
        Self {
            frequency: Frequency::Yearly,
            interval: 1,
            skip: Skip::default(),
            week_start: Weekday::Monday,
            termination: Termination::default(),
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            by_month: Vec::new(),
            by_year_day: Vec::new(),
            by_week_no: Vec::new(),
            by_hour: Vec::new(),
            by_minute: Vec::new(),
            by_second: Vec::new(),
            by_set_position: Vec::new(),
        }
    }
}
