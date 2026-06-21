use super::evaluator_temporal_math::add_months;
use super::evaluator_timezone::{timezone_named_offset_local, timezone_named_offset_standard};
use super::{DurationMode, DurationParts, TemporalAnchor, TemporalOperand, TemporalValue};
use chrono::{Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

pub(super) fn build_duration_parts(
    mode: DurationMode,
    lhs: &TemporalOperand,
    rhs: &TemporalOperand,
) -> Option<DurationParts> {
    let lhs_anchor = temporal_anchor(lhs);
    let rhs_anchor = temporal_anchor(rhs);

    let fallback_date = NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid epoch date");
    let shared_date = if lhs_anchor.has_date {
        lhs_anchor.date
    } else if rhs_anchor.has_date {
        rhs_anchor.date
    } else {
        fallback_date
    };

    let lhs_date = if lhs_anchor.has_date {
        lhs_anchor.date
    } else {
        shared_date
    };
    let rhs_date = if rhs_anchor.has_date {
        rhs_anchor.date
    } else {
        shared_date
    };

    let fallback_offset = lhs_anchor
        .offset
        .or(rhs_anchor.offset)
        .or_else(|| FixedOffset::east_opt(0))
        .expect("UTC offset");
    let shared_zone = lhs_anchor
        .zone_name
        .clone()
        .or_else(|| rhs_anchor.zone_name.clone());
    let lhs_offset = resolve_anchor_offset(
        &lhs_anchor,
        lhs_date,
        shared_zone.as_deref(),
        fallback_offset,
    );
    let rhs_offset = resolve_anchor_offset(
        &rhs_anchor,
        rhs_date,
        shared_zone.as_deref(),
        fallback_offset,
    );

    let lhs_local = lhs_date.and_time(lhs_anchor.time);
    let rhs_local = rhs_date.and_time(rhs_anchor.time);

    let lhs_dt = lhs_offset.from_local_datetime(&lhs_local).single()?;
    let rhs_dt = rhs_offset.from_local_datetime(&rhs_local).single()?;
    let diff_nanos = rhs_dt.signed_duration_since(lhs_dt).num_nanoseconds()?;

    let both_date_based = lhs_anchor.has_date && rhs_anchor.has_date;

    match mode {
        DurationMode::InSeconds => Some(DurationParts {
            months: 0,
            days: 0,
            nanos: diff_nanos,
        }),
        DurationMode::InDays => {
            const DAY_NANOS: i64 = 86_400_000_000_000;
            Some(DurationParts {
                months: 0,
                days: diff_nanos / DAY_NANOS,
                nanos: 0,
            })
        }
        DurationMode::InMonths => {
            if !both_date_based {
                return Some(DurationParts::default());
            }
            let (months, _, _) = calendar_months_and_remainder_with_offsets(
                lhs_local, rhs_local, lhs_offset, rhs_offset,
            )?;
            Some(DurationParts {
                months,
                days: 0,
                nanos: 0,
            })
        }
        DurationMode::Between => {
            if both_date_based {
                let (months, days, nanos) = calendar_months_and_remainder_with_offsets(
                    lhs_local, rhs_local, lhs_offset, rhs_offset,
                )?;
                Some(DurationParts {
                    months,
                    days,
                    nanos,
                })
            } else {
                const DAY_NANOS: i64 = 86_400_000_000_000;
                let days = diff_nanos / DAY_NANOS;
                let nanos = diff_nanos - days * DAY_NANOS;
                Some(DurationParts {
                    months: 0,
                    days,
                    nanos,
                })
            }
        }
    }
}

fn temporal_anchor(operand: &TemporalOperand) -> TemporalAnchor {
    let fallback = NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid epoch date");
    match &operand.value {
        TemporalValue::Date(date) => TemporalAnchor {
            has_date: true,
            date: *date,
            time: NaiveTime::from_hms_opt(0, 0, 0).expect("valid midnight"),
            offset: None,
            zone_name: operand.zone_name.clone(),
        },
        TemporalValue::LocalTime(time) => TemporalAnchor {
            has_date: false,
            date: fallback,
            time: *time,
            offset: None,
            zone_name: operand.zone_name.clone(),
        },
        TemporalValue::Time { time, offset } => TemporalAnchor {
            has_date: false,
            date: fallback,
            time: *time,
            offset: Some(*offset),
            zone_name: operand.zone_name.clone(),
        },
        TemporalValue::LocalDateTime(dt) => TemporalAnchor {
            has_date: true,
            date: dt.date(),
            time: dt.time(),
            offset: None,
            zone_name: operand.zone_name.clone(),
        },
        TemporalValue::DateTime(dt) => TemporalAnchor {
            has_date: true,
            date: dt.naive_local().date(),
            time: dt.naive_local().time(),
            offset: Some(*dt.offset()),
            zone_name: operand.zone_name.clone(),
        },
    }
}

fn resolve_anchor_offset(
    anchor: &TemporalAnchor,
    effective_date: NaiveDate,
    shared_zone: Option<&str>,
    fallback: FixedOffset,
) -> FixedOffset {
    if let Some(offset) = anchor.offset {
        if let Some(zone) = anchor.zone_name.as_deref() {
            return timezone_named_offset_local(zone, effective_date, anchor.time)
                .or_else(|| timezone_named_offset_standard(zone))
                .unwrap_or(offset);
        }
        return offset;
    }

    if let Some(zone) = shared_zone {
        return timezone_named_offset_local(zone, effective_date, anchor.time)
            .or_else(|| timezone_named_offset_standard(zone))
            .unwrap_or(fallback);
    }

    fallback
}

fn calendar_months_and_remainder_with_offsets(
    lhs: NaiveDateTime,
    rhs: NaiveDateTime,
    lhs_offset: FixedOffset,
    rhs_offset: FixedOffset,
) -> Option<(i32, i64, i64)> {
    const DAY_NANOS: i64 = 86_400_000_000_000;

    let lhs_dt = lhs_offset.from_local_datetime(&lhs).single()?;
    let rhs_dt = rhs_offset.from_local_datetime(&rhs).single()?;

    let mut months = (rhs.year() - lhs.year()) * 12 + (rhs.month() as i32 - lhs.month() as i32);
    let mut pivot_local = add_months_to_naive_datetime(lhs, months)?;
    let mut pivot_dt = lhs_offset.from_local_datetime(&pivot_local).single()?;

    if rhs_dt >= lhs_dt {
        while pivot_dt > rhs_dt {
            months -= 1;
            pivot_local = add_months_to_naive_datetime(lhs, months)?;
            pivot_dt = lhs_offset.from_local_datetime(&pivot_local).single()?;
        }
        loop {
            let Some(next_local) = add_months_to_naive_datetime(lhs, months + 1) else {
                break;
            };
            let Some(next_dt) = lhs_offset.from_local_datetime(&next_local).single() else {
                break;
            };
            if next_dt <= rhs_dt {
                months += 1;
                pivot_dt = next_dt;
            } else {
                break;
            }
        }
    } else {
        while pivot_dt < rhs_dt {
            months += 1;
            pivot_local = add_months_to_naive_datetime(lhs, months)?;
            pivot_dt = lhs_offset.from_local_datetime(&pivot_local).single()?;
        }
        loop {
            let Some(next_local) = add_months_to_naive_datetime(lhs, months - 1) else {
                break;
            };
            let Some(next_dt) = lhs_offset.from_local_datetime(&next_local).single() else {
                break;
            };
            if next_dt >= rhs_dt {
                months -= 1;
                pivot_dt = next_dt;
            } else {
                break;
            }
        }
    }

    let remainder_nanos = rhs_dt.signed_duration_since(pivot_dt).num_nanoseconds()?;
    let days = remainder_nanos / DAY_NANOS;
    let nanos = remainder_nanos - days * DAY_NANOS;
    Some((months, days, nanos))
}

fn add_months_to_naive_datetime(dt: NaiveDateTime, delta_months: i32) -> Option<NaiveDateTime> {
    let date = add_months(dt.date(), delta_months)?;
    Some(date.and_time(dt.time()))
}
