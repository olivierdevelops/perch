//! Time ops (time.go) including a port of Go's reference-time layout
//! formatting for `now "<layout>"`.
use crate::group_b::util::*;
use chrono::{DateTime, Datelike, FixedOffset, Local, TimeZone, Timelike};
use perch_interpreter::{Args, Handler, Result};
use serde_json::Value;
use std::collections::HashMap;

const RFC3339: &str = "2006-01-02T15:04:05Z07:00";
const RFC822: &str = "02 Jan 06 15:04 MST";

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert("now".into(), pure(op_now));
    m.insert("unix_to_iso".into(), pure(op_unix_to_iso));
}

fn op_now(a: &Args<'_>) -> Result<Value> {
    let layout = arg_string(a, &["format", "_0"]);
    let t = Local::now().fixed_offset();
    let zone = local_zone_abbrev(t.timestamp());
    let s = match layout.as_str() {
        "" | "rfc3339" => format_go(&t, RFC3339, &zone),
        "rfc822" => format_go(&t, RFC822, &zone),
        "unix" => t.timestamp().to_string(),
        "unix_milli" => t.timestamp_millis().to_string(),
        "date" => format_go(&t, "2006-01-02", &zone),
        "time" => format_go(&t, "15:04:05", &zone),
        "datetime" => format_go(&t, "2006-01-02 15:04:05", &zone),
        l => format_go(&t, l, &zone),
    };
    Ok(Value::String(s))
}

fn op_unix_to_iso(a: &Args<'_>) -> Result<Value> {
    let secs = f2i(to_float(a.map.get("_0")));
    let utc = FixedOffset::east_opt(0).unwrap();
    let t = utc.timestamp_opt(secs, 0).single();
    Ok(Value::String(match t {
        Some(t) => format_go(&t, RFC3339, "UTC"),
        None => String::new(),
    }))
}

/// The local zone's abbreviation (`MST` in Go layouts) at `unix`.
#[cfg(unix)]
fn local_zone_abbrev(unix: i64) -> String {
    // SAFETY: localtime_r fills a caller-owned tm; tm_zone points into libc's
    // static tz data and is copied out immediately.
    unsafe {
        let t = unix as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() || tm.tm_zone.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(tm.tm_zone).to_string_lossy().into_owned()
    }
}

#[cfg(not(unix))]
fn local_zone_abbrev(_unix: i64) -> String {
    String::new()
}

const LONG_DAY: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const LONG_MONTH: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
];

#[derive(Debug, PartialEq)]
enum Std {
    LongMonth,
    Month,
    NumMonth,
    ZeroMonth,
    LongWeekDay,
    WeekDay,
    Day,
    UnderDay,
    ZeroDay,
    UnderYearDay,
    ZeroYearDay,
    Hour,
    Hour12,
    ZeroHour12,
    Minute,
    ZeroMinute,
    Second,
    ZeroSecond,
    LongYear,
    Year,
    PM,
    Pm,
    Tz,
    /// numeric zone: (iso8601 "Z" form, colon, seconds, short)
    NumTz { iso: bool, colon: bool, secs: bool, short: bool },
    /// fractional seconds: (separator, digits, trim trailing zeros)
    Frac(char, usize, bool),
}

fn is_digit_at(l: &[char], i: usize) -> bool {
    l.get(i).is_some_and(|c| c.is_ascii_digit())
}

fn starts(l: &[char], i: usize, p: &str) -> bool {
    let pc: Vec<char> = p.chars().collect();
    l.len() >= i + pc.len() && l[i..i + pc.len()] == pc[..]
}

/// Go `nextStdChunk`: (prefix, std, rest) or None when no more tokens.
fn next_chunk(l: &[char], from: usize) -> Option<(usize, Std, usize)> {
    // returns (index of token start, token, index after token)
    let mut i = from;
    while i < l.len() {
        let c = l[i];
        match c {
            'J' => {
                if starts(l, i, "Jan") {
                    if starts(l, i, "January") {
                        return Some((i, Std::LongMonth, i + 7));
                    }
                    if !l.get(i + 3).is_some_and(|c| c.is_ascii_lowercase()) {
                        return Some((i, Std::Month, i + 3));
                    }
                }
            }
            'M' => {
                if starts(l, i, "Mon") {
                    if starts(l, i, "Monday") {
                        return Some((i, Std::LongWeekDay, i + 6));
                    }
                    if !l.get(i + 3).is_some_and(|c| c.is_ascii_lowercase()) {
                        return Some((i, Std::WeekDay, i + 3));
                    }
                }
                if starts(l, i, "MST") {
                    return Some((i, Std::Tz, i + 3));
                }
            }
            '0' => {
                if let Some(&d) = l.get(i + 1) {
                    if ('1'..='6').contains(&d) {
                        let s = match d {
                            '1' => Std::ZeroMonth,
                            '2' => Std::ZeroDay,
                            '3' => Std::ZeroHour12,
                            '4' => Std::ZeroMinute,
                            '5' => Std::ZeroSecond,
                            _ => Std::Year,
                        };
                        return Some((i, s, i + 2));
                    }
                }
                if starts(l, i, "002") {
                    return Some((i, Std::ZeroYearDay, i + 3));
                }
            }
            '1' => {
                if starts(l, i, "15") {
                    return Some((i, Std::Hour, i + 2));
                }
                return Some((i, Std::NumMonth, i + 1));
            }
            '2' => {
                if starts(l, i, "2006") {
                    return Some((i, Std::LongYear, i + 4));
                }
                return Some((i, Std::Day, i + 1));
            }
            '_' => {
                if starts(l, i, "_2") {
                    if starts(l, i, "_2006") {
                        // literal "_" then the year
                        return Some((i + 1, Std::LongYear, i + 5));
                    }
                    return Some((i, Std::UnderDay, i + 2));
                }
                if starts(l, i, "__2") {
                    return Some((i, Std::UnderYearDay, i + 3));
                }
            }
            '3' => return Some((i, Std::Hour12, i + 1)),
            '4' => return Some((i, Std::Minute, i + 1)),
            '5' => return Some((i, Std::Second, i + 1)),
            'P' => {
                if starts(l, i, "PM") {
                    return Some((i, Std::PM, i + 2));
                }
            }
            'p' => {
                if starts(l, i, "pm") {
                    return Some((i, Std::Pm, i + 2));
                }
            }
            '-' => {
                for (p, s) in [
                    ("-07:00:00", (false, true, true, false)),
                    ("-070000", (false, false, true, false)),
                    ("-07:00", (false, true, false, false)),
                    ("-0700", (false, false, false, false)),
                    ("-07", (false, false, false, true)),
                ] {
                    if starts(l, i, p) {
                        let (iso, colon, secs, short) = s;
                        return Some((i, Std::NumTz { iso, colon, secs, short }, i + p.chars().count()));
                    }
                }
            }
            'Z' => {
                for (p, s) in [
                    ("Z07:00:00", (true, true, true, false)),
                    ("Z070000", (true, false, true, false)),
                    ("Z07:00", (true, true, false, false)),
                    ("Z0700", (true, false, false, false)),
                    ("Z07", (true, false, false, true)),
                ] {
                    if starts(l, i, p) {
                        let (iso, colon, secs, short) = s;
                        return Some((i, Std::NumTz { iso, colon, secs, short }, i + p.chars().count()));
                    }
                }
            }
            '.' | ',' => {
                if let Some(&ch) = l.get(i + 1) {
                    if ch == '0' || ch == '9' {
                        let mut j = i + 1;
                        while j < l.len() && l[j] == ch {
                            j += 1;
                        }
                        if !is_digit_at(l, j) {
                            return Some((i, Std::Frac(c, j - (i + 1), ch == '9'), j));
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn fmt_offset(off: i32, iso: bool, colon: bool, secs: bool, short: bool) -> String {
    if iso && off == 0 {
        return "Z".into();
    }
    let sign = if off < 0 { '-' } else { '+' };
    let z = off.abs();
    let (h, m, s) = (z / 3600, (z % 3600) / 60, z % 60);
    let mut out = format!("{sign}{h:02}");
    if short {
        return out;
    }
    if colon {
        out.push(':');
    }
    out.push_str(&format!("{m:02}"));
    if secs {
        if colon {
            out.push(':');
        }
        out.push_str(&format!("{s:02}"));
    }
    out
}

/// Formats `t` with a Go reference-time `layout`; `zone` is the abbreviation
/// substituted for `MST`.
fn format_go(t: &DateTime<FixedOffset>, layout: &str, zone: &str) -> String {
    let l: Vec<char> = layout.chars().collect();
    let mut out = String::new();
    let mut pos = 0;
    let off = t.offset().local_minus_utc();
    let hour = t.hour();
    let h12 = if hour.is_multiple_of(12) { 12 } else { hour % 12 };
    let yday = t.ordinal();
    while let Some((start, std, end)) = next_chunk(&l, pos) {
        out.extend(&l[pos..start]);
        pos = end;
        match std {
            Std::LongMonth => out.push_str(LONG_MONTH[t.month0() as usize]),
            Std::Month => out.push_str(&LONG_MONTH[t.month0() as usize][..3]),
            Std::NumMonth => out.push_str(&t.month().to_string()),
            Std::ZeroMonth => out.push_str(&format!("{:02}", t.month())),
            Std::LongWeekDay => out.push_str(LONG_DAY[t.weekday().num_days_from_sunday() as usize]),
            Std::WeekDay => out.push_str(&LONG_DAY[t.weekday().num_days_from_sunday() as usize][..3]),
            Std::Day => out.push_str(&t.day().to_string()),
            Std::UnderDay => out.push_str(&format!("{:>2}", t.day())),
            Std::ZeroDay => out.push_str(&format!("{:02}", t.day())),
            Std::UnderYearDay => out.push_str(&format!("{yday:>3}")),
            Std::ZeroYearDay => out.push_str(&format!("{yday:03}")),
            Std::Hour => out.push_str(&format!("{hour:02}")),
            Std::Hour12 => out.push_str(&h12.to_string()),
            Std::ZeroHour12 => out.push_str(&format!("{h12:02}")),
            Std::Minute => out.push_str(&t.minute().to_string()),
            Std::ZeroMinute => out.push_str(&format!("{:02}", t.minute())),
            Std::Second => out.push_str(&t.second().to_string()),
            Std::ZeroSecond => out.push_str(&format!("{:02}", t.second())),
            Std::LongYear => out.push_str(&format!("{:04}", t.year())),
            Std::Year => out.push_str(&format!("{:02}", t.year().rem_euclid(100))),
            Std::PM => out.push_str(if hour >= 12 { "PM" } else { "AM" }),
            Std::Pm => out.push_str(if hour >= 12 { "pm" } else { "am" }),
            Std::Tz => {
                if !zone.is_empty() {
                    out.push_str(zone);
                } else {
                    let z = off.abs();
                    out.push(if off < 0 { '-' } else { '+' });
                    out.push_str(&format!("{:02}", z / 3600));
                    if (z % 3600) / 60 != 0 {
                        out.push_str(&format!("{:02}", (z % 3600) / 60));
                    }
                }
            }
            Std::NumTz { iso, colon, secs, short } => out.push_str(&fmt_offset(off, iso, colon, secs, short)),
            Std::Frac(sep, digits, trim) => {
                let ns = format!("{:09}", t.nanosecond() % 1_000_000_000);
                let mut f: String = ns.chars().take(digits.min(9)).collect();
                while f.len() < digits {
                    f.push('0');
                }
                if trim {
                    let f = f.trim_end_matches('0');
                    if !f.is_empty() {
                        out.push(sep);
                        out.push_str(f);
                    }
                } else {
                    out.push(sep);
                    out.push_str(&f);
                }
            }
        }
    }
    out.extend(&l[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn t() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600).unwrap().with_ymd_and_hms(2026, 3, 9, 15, 4, 5).unwrap()
    }

    #[test]
    fn layouts() {
        assert_eq!(format_go(&t(), RFC3339, "CEST"), "2026-03-09T15:04:05+02:00");
        assert_eq!(format_go(&t(), RFC822, "CEST"), "09 Mar 26 15:04 CEST");
        assert_eq!(format_go(&t(), "Mon Jan _2 3:04PM 2006", ""), "Mon Mar  9 3:04PM 2026");
        assert_eq!(format_go(&t(), "Monday, January 2", ""), "Monday, March 9");
        assert_eq!(format_go(&t(), "15h04 .000 .999", ""), "15h04 .000 ");
        assert_eq!(format_go(&t(), "-0700 Z07:00 MST", ""), "+0200 +02:00 +02");
    }

    #[test]
    fn unix_to_iso_epoch() {
        let map: Map<String, Value> = json!({"_0": 86400}).as_object().unwrap().clone();
        let a = Args { map, body: &[] };
        assert_eq!(op_unix_to_iso(&a).unwrap(), json!("1970-01-02T00:00:00Z"));
    }

    #[test]
    fn now_unix_is_numeric() {
        let map: Map<String, Value> = json!({"_0": "unix"}).as_object().unwrap().clone();
        let a = Args { map, body: &[] };
        let v = op_now(&a).unwrap();
        assert!(v.as_str().unwrap().parse::<i64>().unwrap() > 1_700_000_000);
    }
}
