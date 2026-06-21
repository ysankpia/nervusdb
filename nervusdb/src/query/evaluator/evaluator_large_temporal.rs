use super::{LargeDate, LargeDateTime};

pub(super) fn parse_large_date_literal(input: &str) -> Option<LargeDate> {
    let s = input.trim();
    let last_dash = s.rfind('-')?;
    let day_str = &s[last_dash + 1..];
    let left = &s[..last_dash];
    let second_dash = left.rfind('-')?;
    let month_str = &left[second_dash + 1..];
    let year_str = &left[..second_dash];

    if day_str.len() != 2 || month_str.len() != 2 || year_str.is_empty() {
        return None;
    }
    let digits = year_str.trim_start_matches(['+', '-']).len();
    if digits <= 4 {
        return None;
    }

    let year = year_str.parse::<i64>().ok()?;
    let month = month_str.parse::<u32>().ok()?;
    let day = day_str.parse::<u32>().ok()?;
    let max_day = days_in_month_large(year, month)?;
    if day == 0 || day > max_day {
        return None;
    }

    Some(LargeDate { year, month, day })
}

pub(super) fn parse_large_localdatetime_literal(input: &str) -> Option<LargeDateTime> {
    let s = input.trim();
    if let Some((date_part, time_part)) = s.split_once('T') {
        let date = parse_large_date_literal(date_part)?;
        let (base_time, frac_opt) = if let Some((base, frac)) = time_part.split_once('.') {
            (base, Some(frac))
        } else {
            (time_part, None)
        };

        let mut iter = base_time.split(':');
        let hour = iter.next()?.parse::<u32>().ok()?;
        let minute = iter.next()?.parse::<u32>().ok()?;
        let second = iter.next().unwrap_or("0").parse::<u32>().ok()?;
        if iter.next().is_some() {
            return None;
        }
        if hour >= 24 || minute >= 60 || second >= 60 {
            return None;
        }

        let nanos = match frac_opt {
            Some(f) => {
                if f.is_empty() || !f.chars().all(|ch| ch.is_ascii_digit()) {
                    return None;
                }
                let mut frac = f.chars().take(9).collect::<String>();
                while frac.len() < 9 {
                    frac.push('0');
                }
                frac.parse::<u32>().ok()?
            }
            None => 0,
        };

        return Some(LargeDateTime {
            date,
            hour,
            minute,
            second,
            nanos,
        });
    }

    parse_large_date_literal(s).map(|date| LargeDateTime {
        date,
        hour: 0,
        minute: 0,
        second: 0,
        nanos: 0,
    })
}

pub(super) fn format_large_date_literal(date: LargeDate) -> String {
    format!(
        "{}-{:02}-{:02}",
        format_large_year(date.year),
        date.month,
        date.day
    )
}

pub(super) fn format_large_localdatetime_literal(dt: LargeDateTime) -> String {
    let mut out = format!(
        "{}-{:02}-{:02}T{:02}:{:02}",
        format_large_year(dt.date.year),
        dt.date.month,
        dt.date.day,
        dt.hour,
        dt.minute
    );
    if dt.second != 0 || dt.nanos != 0 {
        if dt.nanos == 0 {
            out.push_str(&format!(":{:02}", dt.second));
        } else {
            let mut frac = format!("{:09}", dt.nanos);
            while frac.ends_with('0') {
                frac.pop();
            }
            out.push_str(&format!(":{:02}.{frac}", dt.second));
        }
    }
    out
}

pub(super) fn large_months_and_days_between(lhs: LargeDate, rhs: LargeDate) -> Option<(i64, i64)> {
    let mut months = (rhs.year - lhs.year) * 12 + (rhs.month as i64 - lhs.month as i64);
    let mut pivot = add_months_large_date(lhs, months)?;

    if (rhs.year, rhs.month, rhs.day) >= (lhs.year, lhs.month, lhs.day) {
        while (pivot.year, pivot.month, pivot.day) > (rhs.year, rhs.month, rhs.day) {
            months -= 1;
            pivot = add_months_large_date(lhs, months)?;
        }
        loop {
            let Some(next) = add_months_large_date(lhs, months + 1) else {
                break;
            };
            if (next.year, next.month, next.day) <= (rhs.year, rhs.month, rhs.day) {
                months += 1;
                pivot = next;
            } else {
                break;
            }
        }
    } else {
        while (pivot.year, pivot.month, pivot.day) < (rhs.year, rhs.month, rhs.day) {
            months += 1;
            pivot = add_months_large_date(lhs, months)?;
        }
        loop {
            let Some(next) = add_months_large_date(lhs, months - 1) else {
                break;
            };
            if (next.year, next.month, next.day) >= (rhs.year, rhs.month, rhs.day) {
                months -= 1;
                pivot = next;
            } else {
                break;
            }
        }
    }

    let day_delta = days_from_civil_i128(rhs.year, rhs.month, rhs.day)
        - days_from_civil_i128(pivot.year, pivot.month, pivot.day);
    let days = i64::try_from(day_delta).ok()?;
    Some((months, days))
}

pub(super) fn large_localdatetime_epoch_nanos(dt: LargeDateTime) -> Option<i128> {
    let day_nanos = 86_400_000_000_000i128;
    let days = days_from_civil_i128(dt.date.year, dt.date.month, dt.date.day);
    let seconds = (dt.hour as i128) * 3600 + (dt.minute as i128) * 60 + (dt.second as i128);
    days.checked_mul(day_nanos)?
        .checked_add(seconds.checked_mul(1_000_000_000i128)?)?
        .checked_add(dt.nanos as i128)
}

fn format_large_year(year: i64) -> String {
    if year >= 0 {
        format!("+{year}")
    } else {
        year.to_string()
    }
}

fn is_leap_year_large(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month_large(year: i64, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if is_leap_year_large(year) { 29 } else { 28 }),
        _ => None,
    }
}

fn add_months_large_date(date: LargeDate, delta_months: i64) -> Option<LargeDate> {
    let total_months = date.year * 12 + (date.month as i64 - 1) + delta_months;
    let year = total_months.div_euclid(12);
    let month = (total_months.rem_euclid(12) + 1) as u32;
    let max_day = days_in_month_large(year, month)?;
    let day = date.day.min(max_day);
    Some(LargeDate { year, month, day })
}

fn days_from_civil_i128(year: i64, month: u32, day: u32) -> i128 {
    let mut y = year as i128;
    let m = month as i128;
    let d = day as i128;
    y -= if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
