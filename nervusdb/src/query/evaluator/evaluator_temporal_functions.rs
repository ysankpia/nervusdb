use super::Value;
use super::evaluator_constructors::{
    construct_date, construct_datetime, construct_datetime_from_epoch,
    construct_datetime_from_epoch_millis, construct_duration, construct_local_datetime,
    construct_local_time, construct_time,
};
use super::evaluator_duration_between::evaluate_duration_between;
use super::evaluator_temporal_truncate::evaluate_temporal_truncate;

pub(super) fn evaluate_temporal_function(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        "date" | "date.transaction" | "date.statement" | "date.realtime" => {
            Some(construct_date(args.first()))
        }
        "localtime" | "localtime.transaction" | "localtime.statement" | "localtime.realtime" => {
            Some(construct_local_time(args.first()))
        }
        "time" | "time.transaction" | "time.statement" | "time.realtime" => {
            Some(construct_time(args.first()))
        }
        "localdatetime"
        | "localdatetime.transaction"
        | "localdatetime.statement"
        | "localdatetime.realtime" => Some(construct_local_datetime(args.first())),
        "datetime" | "datetime.transaction" | "datetime.statement" | "datetime.realtime" => {
            Some(construct_datetime(args.first()))
        }
        "datetime.fromepoch" => Some(construct_datetime_from_epoch(args)),
        "datetime.fromepochmillis" => Some(construct_datetime_from_epoch_millis(args)),
        "duration" => Some(construct_duration(args.first())),
        "date.truncate"
        | "localtime.truncate"
        | "time.truncate"
        | "localdatetime.truncate"
        | "datetime.truncate" => Some(evaluate_temporal_truncate(name, args)),
        "duration.between" | "duration.inmonths" | "duration.indays" | "duration.inseconds" => {
            Some(evaluate_duration_between(name, args))
        }
        _ => None,
    }
}
