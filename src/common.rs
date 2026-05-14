//! Shared data models and utility functions.

use chrono::Datelike;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use chrono::Timelike;
use std::io::IsTerminal;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Author selection filter.
#[derive(Debug, Clone)]
pub struct AuthorSelection {
    pub all_authors: bool,
    pub author_names: Vec<String>,
    pub author_emails: Vec<String>,
    pub normalized_names: Vec<String>,
    pub normalized_emails: Vec<String>,
}

/// Author summary (grouped identity).
#[derive(Debug, Clone)]
pub struct AuthorSummary {
    pub commits: u64,
    pub primary_name: String,
    pub primary_email: String,
    pub names: Vec<String>,
    pub emails: Vec<String>,
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Resolve the requested theme option to a boolean dark-mode flag.
pub fn resolve_dark_mode(theme: &str) -> bool {
    match theme.to_lowercase().as_str() {
        "dark" => true,
        "light" => false,
        "auto" => guess_dark_theme(),
        _ => false,
    }
}

fn infer_dark_mode_from_rgb(rgb: (u8, u8, u8)) -> bool {
    let (r, g, b) = rgb;
    let brightness = 0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64;
    brightness < 128.0
}

fn guess_dark_theme_from_colorfgbg() -> Option<bool> {
    let colorfgbg = std::env::var("COLORFGBG").ok()?;
    let bg = colorfgbg.split(';').next_back()?;
    let value = bg.parse::<u8>().ok()?;
    Some(value < 8)
}

/// Best-effort theme guess from the active terminal, falling back to environment hints.
pub fn guess_dark_theme() -> bool {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        if let Ok(fd) = crate::terminal::open_render_tty() {
            let query_result = crate::terminal::query_terminal_background_rgb(fd, 0.15);
            unsafe {
                libc::close(fd);
            }
            if let Ok(Some(rgb)) = query_result {
                return infer_dark_mode_from_rgb(rgb);
            }
        }
    }

    guess_dark_theme_from_colorfgbg().unwrap_or(false)
}

/// Deduplicate strings while preserving their original order.
pub fn dedupe_preserving_order(values: &[String]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result: Vec<String> = Vec::new();
    for value in values {
        if value.trim().is_empty() {
            continue;
        }
        let marker = normalize_text(value);
        if seen.contains(&marker) {
            continue;
        }
        seen.insert(marker.clone());
        result.push(value.clone());
    }
    result
}

/// Resolve the requested author filter into an `AuthorSelection`.
/// When `default_all` is true and no filters are given, defaults to all authors.
pub fn resolve_author_selection(
    all_authors: bool,
    author_names: &[String],
    author_emails: &[String],
    default_all: bool,
) -> Result<AuthorSelection, String> {
    let cleaned_names = dedupe_preserving_order(author_names);
    let cleaned_emails = dedupe_preserving_order(author_emails);

    if all_authors && (!cleaned_names.is_empty() || !cleaned_emails.is_empty()) {
        return Err(
            "--all-authors cannot be combined with --author-name or --author-email".to_string(),
        );
    }

    let all_authors = if all_authors {
        true
    } else if cleaned_names.is_empty() && cleaned_emails.is_empty() {
        if default_all {
            true
        } else {
            return Err(
                "Select authors with --all-authors, --author-name, or --author-email".to_string(),
            );
        }
    } else {
        false
    };

    let normalized_names = cleaned_names.iter().map(|v| normalize_text(v)).collect();
    let normalized_emails = cleaned_emails.iter().map(|v| normalize_text(v)).collect();

    Ok(AuthorSelection {
        all_authors,
        author_names: cleaned_names,
        author_emails: cleaned_emails,
        normalized_names,
        normalized_emails,
    })
}

/// Return a concise human-readable author filter description.
pub fn describe_author_selection(selection: &AuthorSelection) -> String {
    if selection.all_authors {
        return "all authors".to_string();
    }
    let mut parts = Vec::new();
    if !selection.author_names.is_empty() {
        parts.push("name=".to_owned() + &selection.author_names.join(", "));
    }
    if !selection.author_emails.is_empty() {
        parts.push("email=".to_owned() + &selection.author_emails.join(", "));
    }
    parts.join(" or ")
}

/// Normalize text for case-insensitive matching.
pub fn normalize_text(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Check whether an author matches the configured selection.
pub fn author_matches(selection: &AuthorSelection, author_name: &str, author_email: &str) -> bool {
    if selection.all_authors {
        return true;
    }
    let normalized_name = normalize_text(author_name);
    let normalized_email = normalize_text(author_email);
    selection.normalized_names.contains(&normalized_name)
        || selection.normalized_emails.contains(&normalized_email)
}

/// Map a date to a hitmap row index where Sunday is row 0.
/// Convert a date to a row index for the hitmap grid.
/// Sunday (row 0) at the top, Saturday (row 6) at the bottom — matching GitHub.
pub fn day_to_row_index(date: &NaiveDate) -> u8 {
    date.weekday().num_days_from_sunday() as u8
}

/// Return the Sunday on or before the given date.
pub fn sunday_on_or_before(date: NaiveDate) -> NaiveDate {
    let days_since_sunday = date.weekday().num_days_from_sunday() as i64;
    date - chrono::Duration::days(days_since_sunday)
}

/// Return the Saturday on or after the given date.
pub fn saturday_on_or_after(date: NaiveDate) -> NaiveDate {
    let days_until_saturday = (6 - date.weekday().num_days_from_sunday()) as i64;
    date + chrono::Duration::days(days_until_saturday)
}

/// Shift a date by a calendar-month delta.
pub fn shift_date_by_months(date: &NaiveDate, months: i32) -> NaiveDate {
    let month_index = (date.year() * 12) + (date.month() as i32 - 1) + months;
    let year = month_index / 12;
    let month = ((month_index % 12 + 12) % 12 + 1) as u32;
    let max_day = days_in_month(year, month);
    let day = std::cmp::min(date.day(), max_day);
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

/// Compute days in a month for a given year.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30, // shouldn't happen
    }
}

/// Shift a datetime backward by the requested duration string.
pub fn shift_datetime_back(
    dt: &NaiveDateTime,
    amount: u32,
    unit: char,
) -> Result<NaiveDateTime, String> {
    match unit.to_ascii_lowercase() {
        'd' => {
            let base = *dt;
            Ok(base - chrono::Duration::days(amount as i64))
        }
        'w' => {
            let base = *dt;
            Ok(base - chrono::Duration::weeks(amount as i64))
        }
        'm' => {
            let shifted = shift_date_by_months(&dt.date(), -(amount as i32));
            Ok(shifted
                .and_hms_opt(dt.hour(), dt.minute(), dt.second())
                .unwrap())
        }
        'y' => {
            let shifted = shift_date_by_months(&dt.date(), -((amount * 12) as i32));
            Ok(shifted
                .and_hms_opt(dt.hour(), dt.minute(), dt.second())
                .unwrap())
        }
        _ => Err(format!("Invalid --last unit: {}", unit)),
    }
}

/// Parse the --last window value.
pub fn parse_last_window(s: &str) -> Result<(u32, char), String> {
    let s = s.trim();
    if s.len() < 2 {
        return Err(format!("Invalid --last value: '{}'", s));
    }
    let digit_end = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("Invalid --last value: '{}'", s))?;
    if digit_end == 0 {
        return Err(format!("Invalid --last value: '{}'", s));
    }
    let amount: u32 = s[..digit_end]
        .parse()
        .map_err(|_| format!("Invalid --last value: '{}'", s))?;
    let unit = s[digit_end..]
        .chars()
        .next()
        .ok_or_else(|| format!("Invalid --last value: '{}'", s))?;
    if !"dwmyDWMY".contains(unit) {
        return Err(format!("Invalid --last unit: '{}'", unit));
    }
    Ok((amount, unit))
}

/// Format a section label for the hitmap.
pub fn format_section_label(start: &NaiveDate, end: &NaiveDate, exact: bool) -> String {
    if exact {
        format!("{} - {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d"))
    } else {
        format!("{} - {}", start.format("%Y"), end.format("%Y"))
    }
}

#[cfg(test)]
mod tests {
    use super::{guess_dark_theme_from_colorfgbg, infer_dark_mode_from_rgb};

    #[test]
    fn infer_dark_mode_from_rgb_distinguishes_dark_and_light_backgrounds() {
        assert!(infer_dark_mode_from_rgb((30, 30, 46)));
        assert!(!infer_dark_mode_from_rgb((239, 241, 245)));
    }

    #[test]
    fn guess_dark_theme_from_colorfgbg_parses_background_index() {
        unsafe {
            std::env::set_var("COLORFGBG", "15;0");
        }
        assert_eq!(guess_dark_theme_from_colorfgbg(), Some(true));
        unsafe {
            std::env::set_var("COLORFGBG", "0;15");
        }
        assert_eq!(guess_dark_theme_from_colorfgbg(), Some(false));
        unsafe {
            std::env::remove_var("COLORFGBG");
        }
    }
}
