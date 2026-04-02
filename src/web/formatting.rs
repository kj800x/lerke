use chrono::TimeZone;
use chrono_tz::America::New_York;

fn parse_utc(ts: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

fn to_eastern(ts: &str) -> Option<chrono::DateTime<chrono_tz::Tz>> {
    parse_utc(ts).map(|naive| chrono::Utc.from_utc_datetime(&naive).with_timezone(&New_York))
}

/// Format a UTC timestamp string as human-readable Eastern time.
/// If the date is today, show just the time. Otherwise show full date + time.
pub fn format_eastern(ts: &str) -> String {
    to_eastern(ts)
        .map(|eastern| {
            let today = chrono::Utc::now().with_timezone(&New_York).date_naive();
            if eastern.date_naive() == today {
                eastern.format("%-I:%M %p").to_string()
            } else {
                eastern.format("%b %-d, %Y at %-I:%M %p").to_string()
            }
        })
        .unwrap_or_else(|| ts.to_string())
}

/// Format as just the time portion in Eastern (for history page where date is already shown).
pub fn format_time_eastern(ts: &str) -> String {
    to_eastern(ts)
        .map(|eastern| eastern.format("%-I:%M %p").to_string())
        .unwrap_or_else(|| ts.to_string())
}

/// Format as a date heading in Eastern (e.g. "Wednesday, April 2, 2026").
pub fn format_date_eastern(ts: &str) -> String {
    to_eastern(ts)
        .map(|eastern| eastern.format("%A, %B %-d, %Y").to_string())
        .unwrap_or_else(|| "Unknown date".to_string())
}
