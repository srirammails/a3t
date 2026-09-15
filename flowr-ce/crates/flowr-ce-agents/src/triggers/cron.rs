//! Cron expression parser and scheduler

use chrono::{Datelike, Timelike};
use tracing::warn;

/// Match a 5-field cron expression (min hour dom month dow) against a datetime
/// Supports: *, */N, N, N-M, N,M,O
pub fn cron_matches(expression: &str, now: &chrono::DateTime<chrono::Utc>) -> bool {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() != 5 {
        warn!(expression = %expression, "Invalid cron expression (need 5 fields)");
        return false;
    }

    let checks = [
        (fields[0], now.minute(),                          0, 59),
        (fields[1], now.hour(),                            0, 23),
        (fields[2], now.day(),                             1, 31),
        (fields[3], now.month(),                           1, 12),
        (fields[4], now.weekday().num_days_from_sunday(),  0,  6),
    ];

    checks.iter().all(|(field, value, min, max)| field_matches(field, *value, *min, *max))
}

fn field_matches(field: &str, value: u32, _min: u32, _max: u32) -> bool {
    if field == "*" {
        return true;
    }

    // */N — every N
    if let Some(step) = field.strip_prefix("*/") {
        if let Ok(n) = step.parse::<u32>() {
            return n > 0 && value % n == 0;
        }
    }

    // Comma-separated values and ranges: 1,5,10 or 1-5
    for part in field.split(',') {
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                if value >= s && value <= e {
                    return true;
                }
                continue;
            }
        }
        if let Ok(n) = part.parse::<u32>() {
            if value == n {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_every_5_minutes() {
        let dt = chrono::Utc.with_ymd_and_hms(2026, 4, 17, 10, 15, 0).unwrap();
        assert!(cron_matches("*/5 * * * *", &dt));

        let dt = chrono::Utc.with_ymd_and_hms(2026, 4, 17, 10, 13, 0).unwrap();
        assert!(!cron_matches("*/5 * * * *", &dt));
    }

    #[test]
    fn test_specific_time() {
        let dt = chrono::Utc.with_ymd_and_hms(2026, 4, 17, 9, 0, 0).unwrap();
        assert!(cron_matches("0 9 * * *", &dt));
        assert!(!cron_matches("0 10 * * *", &dt));
    }

    #[test]
    fn test_range() {
        let dt = chrono::Utc.with_ymd_and_hms(2026, 4, 17, 10, 0, 0).unwrap(); // Thursday
        assert!(cron_matches("0 * * * 1-5", &dt)); // weekdays
    }

    #[test]
    fn test_star() {
        let dt = chrono::Utc.with_ymd_and_hms(2026, 4, 17, 10, 33, 0).unwrap();
        assert!(cron_matches("* * * * *", &dt));
    }
}
