use chrono::{Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, Timelike};

pub(super) fn timezone_named_offset(name: &str, date: NaiveDate) -> Option<FixedOffset> {
    match name {
        "Europe/Stockholm" => {
            if date.year() <= 1818 {
                FixedOffset::east_opt(53 * 60 + 28)
            } else if is_dst_europe(date) {
                FixedOffset::east_opt(2 * 3600)
            } else {
                FixedOffset::east_opt(3600)
            }
        }
        "Europe/London" => {
            if is_dst_europe(date) {
                FixedOffset::east_opt(3600)
            } else {
                FixedOffset::east_opt(0)
            }
        }
        "America/New_York" => {
            if is_dst_us(date) {
                FixedOffset::west_opt(4 * 3600)
            } else {
                FixedOffset::west_opt(5 * 3600)
            }
        }
        "Pacific/Honolulu" => FixedOffset::west_opt(10 * 3600),
        "Australia/Eucla" => FixedOffset::east_opt(8 * 3600 + 45 * 60),
        _ => None,
    }
}

pub(super) fn timezone_named_offset_local(
    name: &str,
    date: NaiveDate,
    time: NaiveTime,
) -> Option<FixedOffset> {
    match name {
        "Europe/Stockholm" => {
            if date.year() <= 1818 {
                FixedOffset::east_opt(53 * 60 + 28)
            } else if is_dst_europe_local(date, time) {
                FixedOffset::east_opt(2 * 3600)
            } else {
                FixedOffset::east_opt(3600)
            }
        }
        "Europe/London" => {
            if is_dst_europe_local(date, time) {
                FixedOffset::east_opt(3600)
            } else {
                FixedOffset::east_opt(0)
            }
        }
        "America/New_York" => {
            if is_dst_us_local(date, time) {
                FixedOffset::west_opt(4 * 3600)
            } else {
                FixedOffset::west_opt(5 * 3600)
            }
        }
        "Pacific/Honolulu" => FixedOffset::west_opt(10 * 3600),
        "Australia/Eucla" => FixedOffset::east_opt(8 * 3600 + 45 * 60),
        _ => None,
    }
}

pub(super) fn timezone_named_offset_standard(name: &str) -> Option<FixedOffset> {
    match name {
        "Europe/Stockholm" => FixedOffset::east_opt(3600),
        "Europe/London" => FixedOffset::east_opt(0),
        "America/New_York" => FixedOffset::west_opt(5 * 3600),
        "Pacific/Honolulu" => FixedOffset::west_opt(10 * 3600),
        "Australia/Eucla" => FixedOffset::east_opt(8 * 3600 + 45 * 60),
        _ => None,
    }
}

pub(super) fn parse_fixed_offset(s: &str) -> Option<FixedOffset> {
    if s.is_empty() {
        return None;
    }

    let sign = if s.starts_with('+') {
        1
    } else if s.starts_with('-') {
        -1
    } else {
        return None;
    };

    let (hour, minute, second) =
        if s.len() == 9 && s.as_bytes().get(3) == Some(&b':') && s.as_bytes().get(6) == Some(&b':')
        {
            let hour: i32 = s[1..3].parse().ok()?;
            let minute: i32 = s[4..6].parse().ok()?;
            let second: i32 = s[7..9].parse().ok()?;
            (hour, minute, second)
        } else if s.len() == 7 {
            let hour: i32 = s[1..3].parse().ok()?;
            let minute: i32 = s[3..5].parse().ok()?;
            let second: i32 = s[5..7].parse().ok()?;
            (hour, minute, second)
        } else if s.len() == 6 && s.as_bytes().get(3) == Some(&b':') {
            let hour: i32 = s[1..3].parse().ok()?;
            let minute: i32 = s[4..6].parse().ok()?;
            (hour, minute, 0)
        } else if s.len() == 5 {
            let hour: i32 = s[1..3].parse().ok()?;
            let minute: i32 = s[3..5].parse().ok()?;
            (hour, minute, 0)
        } else if s.len() == 3 {
            let hour: i32 = s[1..3].parse().ok()?;
            (hour, 0, 0)
        } else {
            return None;
        };

    let secs = sign * (hour * 3600 + minute * 60 + second);
    FixedOffset::east_opt(secs)
}

pub(super) fn format_offset(offset: FixedOffset) -> String {
    let secs = offset.local_minus_utc();
    if secs == 0 {
        return "Z".to_string();
    }
    let sign = if secs < 0 { '-' } else { '+' };
    let abs = secs.abs();
    let hour = abs / 3600;
    let minute = (abs % 3600) / 60;
    let second = abs % 60;
    if second == 0 {
        format!("{sign}{hour:02}:{minute:02}")
    } else {
        format!("{sign}{hour:02}:{minute:02}:{second:02}")
    }
}

fn is_dst_europe(date: NaiveDate) -> bool {
    let year = date.year();
    if year < 1980 {
        return false;
    }

    let Some(start_day) = last_weekday_of_month(year, 3, chrono::Weekday::Sun) else {
        return false;
    };

    let end_month = if year < 1996 { 9 } else { 10 };
    let Some(end_day) = last_weekday_of_month(year, end_month, chrono::Weekday::Sun) else {
        return false;
    };

    let Some(start) = NaiveDate::from_ymd_opt(year, 3, start_day) else {
        return false;
    };
    let Some(end) = NaiveDate::from_ymd_opt(year, end_month, end_day) else {
        return false;
    };

    date >= start && date < end
}

fn is_dst_europe_local(date: NaiveDate, time: NaiveTime) -> bool {
    let year = date.year();
    if year < 1980 {
        return false;
    }

    let Some(start_day) = last_weekday_of_month(year, 3, chrono::Weekday::Sun) else {
        return false;
    };
    let end_month = if year < 1996 { 9 } else { 10 };
    let Some(end_day) = last_weekday_of_month(year, end_month, chrono::Weekday::Sun) else {
        return false;
    };

    let Some(start) = NaiveDate::from_ymd_opt(year, 3, start_day) else {
        return false;
    };
    let Some(end) = NaiveDate::from_ymd_opt(year, end_month, end_day) else {
        return false;
    };

    if date > start && date < end {
        return true;
    }
    if date == start {
        return time.hour() >= 2;
    }
    if date == end {
        return time.hour() < 3;
    }
    false
}

fn is_dst_us(date: NaiveDate) -> bool {
    let year = date.year();
    let Some(start_day) = nth_weekday_of_month(year, 3, chrono::Weekday::Sun, 2) else {
        return false;
    };
    let Some(end_day) = nth_weekday_of_month(year, 11, chrono::Weekday::Sun, 1) else {
        return false;
    };
    let Some(start) = NaiveDate::from_ymd_opt(year, 3, start_day) else {
        return false;
    };
    let Some(end) = NaiveDate::from_ymd_opt(year, 11, end_day) else {
        return false;
    };
    date >= start && date < end
}

fn is_dst_us_local(date: NaiveDate, time: NaiveTime) -> bool {
    let year = date.year();
    let Some(start_day) = nth_weekday_of_month(year, 3, chrono::Weekday::Sun, 2) else {
        return false;
    };
    let Some(end_day) = nth_weekday_of_month(year, 11, chrono::Weekday::Sun, 1) else {
        return false;
    };
    let Some(start) = NaiveDate::from_ymd_opt(year, 3, start_day) else {
        return false;
    };
    let Some(end) = NaiveDate::from_ymd_opt(year, 11, end_day) else {
        return false;
    };

    if date > start && date < end {
        return true;
    }
    if date == start {
        return time.hour() >= 2;
    }
    if date == end {
        return time.hour() < 2;
    }
    false
}

fn last_weekday_of_month(year: i32, month: u32, weekday: chrono::Weekday) -> Option<u32> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let mut cursor =
        NaiveDate::from_ymd_opt(next_year, next_month, 1)?.checked_sub_signed(Duration::days(1))?;

    while cursor.weekday() != weekday {
        cursor = cursor.checked_sub_signed(Duration::days(1))?;
    }

    Some(cursor.day())
}

fn nth_weekday_of_month(year: i32, month: u32, weekday: chrono::Weekday, nth: u32) -> Option<u32> {
    let mut cursor = NaiveDate::from_ymd_opt(year, month, 1)?;
    while cursor.weekday() != weekday {
        cursor = cursor.checked_add_signed(Duration::days(1))?;
    }
    let target = cursor.checked_add_signed(Duration::days(i64::from((nth - 1) * 7)))?;
    if target.month() == month {
        Some(target.day())
    } else {
        None
    }
}
