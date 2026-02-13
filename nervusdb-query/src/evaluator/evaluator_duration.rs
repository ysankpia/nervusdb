use super::{DurationParts, Value};
use std::collections::BTreeMap;

pub(super) fn duration_value(parts: DurationParts) -> Value {
    duration_value_wide(parts.months as i64, parts.days, parts.nanos)
}

pub(super) fn duration_value_wide(months: i64, days: i64, nanos: i64) -> Value {
    let mut out = std::collections::BTreeMap::new();
    out.insert("__kind".to_string(), Value::String("duration".to_string()));
    out.insert("months".to_string(), Value::Int(months));
    out.insert("days".to_string(), Value::Int(days));
    out.insert("nanos".to_string(), Value::Int(nanos));

    let seconds = days
        .saturating_mul(86_400)
        .saturating_add(nanos.div_euclid(1_000_000_000));
    let nanos_of_second = nanos.rem_euclid(1_000_000_000);
    out.insert("seconds".to_string(), Value::Int(seconds));
    out.insert(
        "nanosecondsOfSecond".to_string(),
        Value::Int(nanos_of_second),
    );
    out.insert(
        "__display".to_string(),
        Value::String(duration_iso_components(months, days, nanos)),
    );
    Value::Map(out)
}

pub(super) fn duration_iso_components(months: i64, days: i64, nanos: i64) -> String {
    let mut out = String::from("P");

    let years = months / 12;
    let months = months % 12;
    if years != 0 {
        out.push_str(&format!("{years}Y"));
    }
    if months != 0 {
        out.push_str(&format!("{months}M"));
    }
    if days != 0 {
        out.push_str(&format!("{days}D"));
    }

    let time = duration_time_iso(nanos);
    if !time.is_empty() {
        out.push('T');
        out.push_str(&time);
    }

    if out == "P" { "PT0S".to_string() } else { out }
}

pub(super) fn duration_iso_from_nanos_i128(total_nanos: i128) -> String {
    if total_nanos == 0 {
        return "PT0S".to_string();
    }

    let mut rem = total_nanos;
    let hour = rem / 3_600_000_000_000i128;
    rem -= hour * 3_600_000_000_000i128;

    let minute = rem / 60_000_000_000i128;
    rem -= minute * 60_000_000_000i128;

    let second = rem / 1_000_000_000i128;
    let nano = rem - second * 1_000_000_000i128;

    let mut out = String::from("PT");
    if hour != 0 {
        out.push_str(&format!("{hour}H"));
    }
    if minute != 0 {
        out.push_str(&format!("{minute}M"));
    }
    if second != 0 || nano != 0 {
        if nano == 0 {
            out.push_str(&format!("{second}S"));
        } else {
            let sign = if second < 0 || nano < 0 { "-" } else { "" };
            let mut frac = format!("{:09}", nano.abs());
            while frac.ends_with('0') {
                frac.pop();
            }
            out.push_str(&format!("{sign}{}.{frac}S", second.abs()));
        }
    }
    if out == "PT" { "PT0S".to_string() } else { out }
}

pub(super) fn duration_from_value(value: &Value) -> Option<DurationParts> {
    let Value::Map(map) = value else {
        return None;
    };

    match map.get("__kind") {
        Some(Value::String(kind)) if kind == "duration" => {
            let months = i32::try_from(duration_map_i64(map, "months")?).ok()?;
            let days = duration_map_i64(map, "days")?;
            let nanos = duration_map_i64(map, "nanos")?;
            Some(DurationParts {
                months,
                days,
                nanos,
            })
        }
        _ => None,
    }
}

pub(super) fn duration_from_map(map: &BTreeMap<String, Value>) -> DurationParts {
    const DAY_NANOS: f64 = 86_400_000_000_000.0;
    const AVG_MONTH_NANOS: f64 = 2_629_746_000_000_000.0;

    let years = duration_map_number_any(map, &["years", "year"]).unwrap_or(0.0);
    let months = duration_map_number_any(map, &["months", "month"]).unwrap_or(0.0);
    let weeks = duration_map_number_any(map, &["weeks", "week"]).unwrap_or(0.0);
    let days = duration_map_number_any(map, &["days", "day"]).unwrap_or(0.0);
    let hours = duration_map_number_any(map, &["hours", "hour"]).unwrap_or(0.0);
    let minutes = duration_map_number_any(map, &["minutes", "minute"]).unwrap_or(0.0);
    let seconds = duration_map_number_any(map, &["seconds", "second"]).unwrap_or(0.0);
    let millis = duration_map_number_any(map, &["milliseconds", "millisecond"]).unwrap_or(0.0);
    let micros = duration_map_number_any(map, &["microseconds", "microsecond"]).unwrap_or(0.0);
    let nanos = duration_map_number_any(map, &["nanoseconds", "nanosecond"]).unwrap_or(0.0);

    let total_months = years * 12.0 + months;
    let whole_months = total_months.trunc();
    let fractional_months = total_months - whole_months;

    let month_fraction_nanos_total = fractional_months * AVG_MONTH_NANOS;
    let month_fraction_days = (month_fraction_nanos_total / DAY_NANOS).trunc();
    let month_fraction_nanos = month_fraction_nanos_total - month_fraction_days * DAY_NANOS;

    let total_days = weeks * 7.0 + days;
    let whole_days = total_days.trunc();
    let day_fraction_nanos = (total_days - whole_days) * DAY_NANOS;

    let nanos_total = hours * 3_600_000_000_000.0
        + minutes * 60_000_000_000.0
        + seconds * 1_000_000_000.0
        + millis * 1_000_000.0
        + micros * 1_000.0
        + nanos
        + day_fraction_nanos
        + month_fraction_nanos;

    DurationParts {
        months: whole_months as i32,
        days: (whole_days + month_fraction_days) as i64,
        nanos: nanos_total.trunc() as i64,
    }
}

pub(super) fn parse_duration_literal(input: &str) -> Option<DurationParts> {
    let s = input.trim();
    if !s.starts_with('P') {
        return None;
    }
    let body = &s[1..];
    if body.is_empty() {
        return None;
    }

    // Support the component form used by the TCK: `PYYYY-MM-DDThh:mm:ss[.fff]`.
    //
    // Example:
    // - `P2012-02-02T14:37:21.545` => `P2012Y2M2DT14H37M21.545S`
    if looks_like_duration_components(body) {
        return parse_duration_components(body);
    }

    // ISO 8601 duration form: `PnYnMnWnDTnHnMnS`, with optional fractions in any component.
    let (date_part, time_part) = if let Some(idx) = body.find('T') {
        (&body[..idx], Some(&body[idx + 1..]))
    } else {
        (body, None)
    };

    let mut years = 0.0f64;
    let mut months = 0.0f64;
    let mut weeks = 0.0f64;
    let mut days = 0.0f64;
    let mut hours = 0.0f64;
    let mut minutes = 0.0f64;
    let mut seconds = 0.0f64;
    let mut saw_component = false;

    let mut idx = 0usize;
    while idx < date_part.len() {
        let (next, number, _has_fraction) = parse_duration_number(date_part, idx)?;
        if next >= date_part.len() {
            return None;
        }
        let unit = date_part.as_bytes()[next];
        let value = number.parse::<f64>().ok()?;
        match unit {
            b'Y' => years += value,
            b'M' => months += value,
            b'W' => weeks += value,
            b'D' => days += value,
            _ => return None,
        }
        saw_component = true;
        idx = next + 1;
    }

    if let Some(time_part) = time_part {
        if time_part.is_empty() && !saw_component {
            return None;
        }

        let mut idx = 0usize;
        while idx < time_part.len() {
            let (next, number, _has_fraction) = parse_duration_number(time_part, idx)?;
            if next >= time_part.len() {
                return None;
            }
            let unit = time_part.as_bytes()[next];
            let value = number.parse::<f64>().ok()?;
            match unit {
                b'H' => hours += value,
                b'M' => minutes += value,
                b'S' => seconds += value,
                _ => return None,
            }
            saw_component = true;
            idx = next + 1;
        }
    }

    if !saw_component {
        return None;
    }

    let mut map = BTreeMap::new();
    if years != 0.0 {
        map.insert("years".to_string(), Value::Float(years));
    }
    if months != 0.0 {
        map.insert("months".to_string(), Value::Float(months));
    }
    if weeks != 0.0 {
        map.insert("weeks".to_string(), Value::Float(weeks));
    }
    if days != 0.0 {
        map.insert("days".to_string(), Value::Float(days));
    }
    if hours != 0.0 {
        map.insert("hours".to_string(), Value::Float(hours));
    }
    if minutes != 0.0 {
        map.insert("minutes".to_string(), Value::Float(minutes));
    }
    if seconds != 0.0 {
        map.insert("seconds".to_string(), Value::Float(seconds));
    }

    Some(duration_from_map(&map))
}

pub(super) fn add_duration_parts(lhs: &DurationParts, rhs: &DurationParts) -> DurationParts {
    DurationParts {
        months: lhs.months.saturating_add(rhs.months),
        days: lhs.days.saturating_add(rhs.days),
        nanos: lhs.nanos.saturating_add(rhs.nanos),
    }
}

pub(super) fn sub_duration_parts(lhs: &DurationParts, rhs: &DurationParts) -> DurationParts {
    DurationParts {
        months: lhs.months.saturating_sub(rhs.months),
        days: lhs.days.saturating_sub(rhs.days),
        nanos: lhs.nanos.saturating_sub(rhs.nanos),
    }
}

pub(super) fn scale_duration_parts(parts: DurationParts, factor: f64) -> Option<DurationParts> {
    if !factor.is_finite() {
        return None;
    }

    const AVG_MONTH_NANOS: f64 = 2_629_746_000_000_000.0;
    const DAY_NANOS_F64: f64 = 86_400_000_000_000.0;

    let scaled_months = (parts.months as f64) * factor;
    let months_whole = scaled_months.trunc();
    let month_fraction = scaled_months - months_whole;
    let month_fraction_nanos_total = month_fraction * AVG_MONTH_NANOS;
    let month_fraction_days = (month_fraction_nanos_total / DAY_NANOS_F64).trunc();
    let month_fraction_nanos = month_fraction_nanos_total - month_fraction_days * DAY_NANOS_F64;

    let scaled_days = (parts.days as f64) * factor;
    let days_whole = scaled_days.trunc();
    let day_fraction_nanos = (scaled_days - days_whole) * DAY_NANOS_F64;

    let scaled_nanos = (parts.nanos as f64) * factor;
    let nanos = (scaled_nanos + day_fraction_nanos + month_fraction_nanos).trunc();

    Some(DurationParts {
        months: months_whole as i32,
        days: (days_whole + month_fraction_days) as i64,
        nanos: nanos as i64,
    })
}

fn duration_time_iso(nanos: i64) -> String {
    if nanos == 0 {
        return String::new();
    }

    let mut rem = nanos;
    let hour = rem / 3_600_000_000_000;
    rem -= hour * 3_600_000_000_000;

    let minute = rem / 60_000_000_000;
    rem -= minute * 60_000_000_000;

    let second = rem / 1_000_000_000;
    let nano = rem - second * 1_000_000_000;

    let mut out = String::new();
    if hour != 0 {
        out.push_str(&format!("{hour}H"));
    }
    if minute != 0 {
        out.push_str(&format!("{minute}M"));
    }

    if second != 0 || nano != 0 {
        if nano == 0 {
            out.push_str(&format!("{second}S"));
        } else {
            let sign = if second < 0 || nano < 0 { "-" } else { "" };
            let mut frac = format!("{:09}", nano.abs());
            while frac.ends_with('0') {
                frac.pop();
            }
            out.push_str(&format!("{sign}{}.{frac}S", second.abs()));
        }
    }

    out
}

fn parse_duration_number(s: &str, start: usize) -> Option<(usize, &str, bool)> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if start >= len {
        return None;
    }

    let mut idx = start;
    if bytes[idx] == b'+' || bytes[idx] == b'-' {
        idx += 1;
    }
    let digits_start = idx;
    while idx < len && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == digits_start {
        return None;
    }

    let mut has_fraction = false;
    if idx < len && bytes[idx] == b'.' {
        has_fraction = true;
        idx += 1;
        let fraction_start = idx;
        while idx < len && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == fraction_start {
            return None;
        }
    }

    Some((idx, &s[start..idx], has_fraction))
}

fn parse_duration_seconds_to_nanos(number: &str) -> Option<i128> {
    const SECOND_NANOS: i128 = 1_000_000_000;

    let bytes = number.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let mut idx = 0usize;
    let mut sign = 1i128;
    if bytes[idx] == b'+' {
        idx += 1;
    } else if bytes[idx] == b'-' {
        sign = -1;
        idx += 1;
    }
    if idx >= bytes.len() {
        return None;
    }

    let digits_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == digits_start {
        return None;
    }

    let int_part = number[digits_start..idx].parse::<i128>().ok()?;
    let mut frac_nanos = 0i128;
    if idx < bytes.len() {
        if bytes[idx] != b'.' {
            return None;
        }
        idx += 1;
        let frac_start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == frac_start {
            return None;
        }
        let frac_digits = &number[frac_start..idx];
        if frac_digits.len() > 9 {
            return None;
        }
        let frac_value = frac_digits.parse::<i128>().ok()?;
        let scale = 10i128.pow((9 - frac_digits.len()) as u32);
        frac_nanos = frac_value.checked_mul(scale)?;
    }

    if idx != bytes.len() {
        return None;
    }

    let nanos = int_part
        .checked_mul(SECOND_NANOS)?
        .checked_add(frac_nanos)?;
    nanos.checked_mul(sign)
}

fn looks_like_duration_components(body: &str) -> bool {
    if !body.contains('-') {
        return false;
    }
    // Component format does not use unit letters (Y/M/W/D/H/S).
    !body
        .chars()
        .any(|ch| matches!(ch, 'Y' | 'M' | 'W' | 'D' | 'H' | 'S'))
}

fn parse_duration_components(body: &str) -> Option<DurationParts> {
    const HOUR_NANOS: i128 = 3_600_000_000_000;
    const MINUTE_NANOS: i128 = 60_000_000_000;

    let (date_part, time_part) = body.split_once('T')?;

    let mut date_iter = date_part.split('-');
    let year: i64 = date_iter.next()?.parse().ok()?;
    let month: i64 = date_iter.next()?.parse().ok()?;
    let day: i64 = date_iter.next()?.parse().ok()?;
    if date_iter.next().is_some() {
        return None;
    }

    let mut time_iter = time_part.split(':');
    let hour: i128 = time_iter.next()?.parse().ok()?;
    let minute: i128 = time_iter.next()?.parse().ok()?;
    let seconds_raw = time_iter.next()?;
    if time_iter.next().is_some() {
        return None;
    }

    let seconds_nanos = parse_duration_seconds_to_nanos(seconds_raw)?;
    let nanos = hour
        .checked_mul(HOUR_NANOS)?
        .checked_add(minute.checked_mul(MINUTE_NANOS)?)?
        .checked_add(seconds_nanos)?;

    let months_total = year.checked_mul(12)?.checked_add(month)?;
    Some(DurationParts {
        months: i32::try_from(months_total).ok()?,
        days: day,
        nanos: i64::try_from(nanos).ok()?,
    })
}

fn duration_map_i64(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key) {
        Some(Value::Int(v)) => Some(*v),
        Some(Value::Float(v)) => Some(*v as i64),
        _ => None,
    }
}

fn duration_map_number(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Int(v)) => Some(*v as f64),
        Some(Value::Float(v)) => Some(*v),
        _ => None,
    }
}

fn duration_map_number_any(map: &BTreeMap<String, Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(value) = duration_map_number(map, key) {
            return Some(value);
        }
    }
    None
}
