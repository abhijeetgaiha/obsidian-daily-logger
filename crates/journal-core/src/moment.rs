//! The subset of Moment.js display tokens that Obsidian daily-note formats use.

use chrono::{Datelike, NaiveDate};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Year4,
    Year2,
    Month,
    Month2,
    MonthShort,
    MonthLong,
    Day,
    Day2,
    DayOrdinal,
    DayOfYear,
    DayOfYear3,
    Weekday,
    WeekdayShort,
    WeekdayLong,
    IsoWeekday,
    IsoWeek,
    IsoWeek2,
    IsoWeekOrdinal,
    IsoYear4,
    IsoYear2,
    Quarter,
    QuarterOrdinal,
}

// Longest first, so that e.g. "MMMM" wins over "MM".
const TOKENS: &[(&str, Token)] = &[
    ("YYYY", Token::Year4),
    ("GGGG", Token::IsoYear4),
    ("MMMM", Token::MonthLong),
    ("DDDD", Token::DayOfYear3),
    ("dddd", Token::WeekdayLong),
    ("MMM", Token::MonthShort),
    ("DDD", Token::DayOfYear),
    ("ddd", Token::WeekdayShort),
    ("YY", Token::Year2),
    ("GG", Token::IsoYear2),
    ("MM", Token::Month2),
    ("DD", Token::Day2),
    ("Do", Token::DayOrdinal),
    ("WW", Token::IsoWeek2),
    ("Wo", Token::IsoWeekOrdinal),
    ("Qo", Token::QuarterOrdinal),
    ("M", Token::Month),
    ("D", Token::Day),
    ("d", Token::Weekday),
    ("E", Token::IsoWeekday),
    ("W", Token::IsoWeek),
    ("Q", Token::Quarter),
];

// Moment tokens that are valid but make no sense in, or are ambiguous for, a date-only name.
const UNSUPPORTED: &[&str] = &[
    "YYYYYY", "YYYYY", "gggg", "gg", "ww", "wo", "w", "e", "dd", "do", "LTS", "LT", "LLLL", "LLL",
    "LL", "L", "llll", "lll", "ll", "l", "HH", "H", "hh", "h", "kk", "k", "mm", "m", "ss", "s",
    "SSS", "SS", "S", "a", "A", "X", "x", "ZZ", "Z", "zz", "z", "N", "y", "Y", "G", "g",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Literal(String),
    Token(Token),
}

/// A parsed Moment.js date format such as `YYYY/YYYY-MM/YYYY-MM-DD` or `[Daily] ddd, Do MMM`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateFormat {
    source: String,
    parts: Vec<Part>,
}

impl DateFormat {
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut rest = source;
        'scan: while let Some(first) = rest.chars().next() {
            if first == '[' {
                let Some(end) = rest.find(']') else {
                    return Err(format!("\"{source}\" has an unclosed '['"));
                };
                literal.push_str(&rest[1..end]);
                rest = &rest[end + 1..];
                continue;
            }
            if first.is_ascii_alphabetic() {
                for (text, token) in TOKENS {
                    if let Some(after) = rest.strip_prefix(text) {
                        let longer_unsupported = UNSUPPORTED
                            .iter()
                            .any(|bad| bad.len() > text.len() && rest.starts_with(bad));
                        if !longer_unsupported {
                            if !literal.is_empty() {
                                parts.push(Part::Literal(std::mem::take(&mut literal)));
                            }
                            parts.push(Part::Token(*token));
                            rest = after;
                            continue 'scan;
                        }
                    }
                }
                if let Some(bad) = UNSUPPORTED.iter().find(|bad| rest.starts_with(**bad)) {
                    return Err(format!(
                        "the token \"{bad}\" in \"{source}\" is not supported in daily note \
                         names (time, locale-dependent week, and preset tokens are not \
                         supported)"
                    ));
                }
            }
            literal.push(first);
            rest = &rest[first.len_utf8()..];
        }
        if !literal.is_empty() {
            parts.push(Part::Literal(literal));
        }
        Ok(Self {
            source: source.to_owned(),
            parts,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.source
    }

    pub fn format(&self, date: NaiveDate) -> String {
        let mut output = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => output.push_str(text),
                Part::Token(token) => output.push_str(&render(*token, date)),
            }
        }
        output
    }
}

fn ordinal(number: u32) -> String {
    let suffix = match (number % 10, number % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{number}{suffix}")
}

fn render(token: Token, date: NaiveDate) -> String {
    let month = MONTHS[date.month0() as usize];
    let weekday = WEEKDAYS[date.weekday().num_days_from_sunday() as usize];
    let iso = date.iso_week();
    let quarter = date.month0() / 3 + 1;
    match token {
        Token::Year4 => format!("{:04}", date.year()),
        Token::Year2 => format!("{:02}", date.year().rem_euclid(100)),
        Token::Month => date.month().to_string(),
        Token::Month2 => format!("{:02}", date.month()),
        Token::MonthShort => month[..3].to_owned(),
        Token::MonthLong => month.to_owned(),
        Token::Day => date.day().to_string(),
        Token::Day2 => format!("{:02}", date.day()),
        Token::DayOrdinal => ordinal(date.day()),
        Token::DayOfYear => date.ordinal().to_string(),
        Token::DayOfYear3 => format!("{:03}", date.ordinal()),
        Token::Weekday => date.weekday().num_days_from_sunday().to_string(),
        Token::WeekdayShort => weekday[..3].to_owned(),
        Token::WeekdayLong => weekday.to_owned(),
        Token::IsoWeekday => date.weekday().number_from_monday().to_string(),
        Token::IsoWeek => iso.week().to_string(),
        Token::IsoWeek2 => format!("{:02}", iso.week()),
        Token::IsoWeekOrdinal => ordinal(iso.week()),
        Token::IsoYear4 => format!("{:04}", iso.year()),
        Token::IsoYear2 => format!("{:02}", iso.year().rem_euclid(100)),
        Token::Quarter => quarter.to_string(),
        Token::QuarterOrdinal => ordinal(quarter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn format(source: &str, day: NaiveDate) -> String {
        DateFormat::parse(source).unwrap().format(day)
    }

    #[test]
    fn formats_every_supported_token() {
        let day = date(2026, 9, 27);
        assert_eq!(
            format("YYYY/YYYY-MM/YYYY-MM-DD", day),
            "2026/2026-09/2026-09-27"
        );
        assert_eq!(format("YY M MM MMM MMMM", day), "26 9 09 Sep September");
        assert_eq!(format("D DD Do DDD DDDD", day), "27 27 27th 270 270");
        assert_eq!(format("d ddd dddd E", day), "0 Sun Sunday 7");
        assert_eq!(format("W WW Wo GGGG GG", day), "39 39 39th 2026 26");
        assert_eq!(format("Q Qo", day), "3 3rd");
        assert_eq!(format("D DDD DDDD", date(2026, 1, 5)), "5 5 005");
    }

    #[test]
    fn literals_and_non_letters_are_copied() {
        let day = date(2026, 3, 1);
        assert_eq!(format("[Daily] YYYY-MM-DD", day), "Daily 2026-03-01");
        assert_eq!(format("[YYYY]-YYYY", day), "YYYY-2026");
        assert_eq!(format("YYYY.MM.DD – ddd", day), "2026.03.01 – Sun");
        assert_eq!(format("[]YYYY[]", day), "2026");
        assert_eq!(format("", day), "");
    }

    #[test]
    fn iso_weeks_and_ordinals_cross_year_boundaries() {
        assert_eq!(format("GGGG-[W]WW", date(2027, 1, 1)), "2026-W53");
        assert_eq!(format("GGGG-[W]WW", date(2024, 12, 30)), "2025-W01");
        for (day, expected) in [
            (1, "1st"),
            (2, "2nd"),
            (3, "3rd"),
            (4, "4th"),
            (11, "11th"),
            (12, "12th"),
            (13, "13th"),
            (21, "21st"),
            (22, "22nd"),
            (23, "23rd"),
            (31, "31st"),
        ] {
            assert_eq!(format("Do", date(2026, 1, day)), expected);
        }
    }

    #[test]
    fn unsupported_tokens_are_rejected() {
        for source in [
            "YYYY-MM-DD HH:mm",
            "YYYY-MM-DD h",
            "gggg-ww",
            "YYYY [W]w",
            "dd",
            "L",
            "LL",
            "YYYY-MM-DD a",
            "Y",
            "YYYYY",
            "e",
            "X",
            "[unclosed",
        ] {
            assert!(DateFormat::parse(source).is_err(), "{source}");
        }
        assert!(DateFormat::parse("[HH:mm] YYYY").is_ok());
    }
}
