//! Year-section building: week matrices, month labels, day counts.

use chrono::{Datelike, NaiveDate, Utc};

use crate::common::*;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single year section of the hitmap.
#[derive(Debug, Clone)]
pub struct YearSection {
    pub label: String,
    pub range_start: NaiveDate,
    pub range_end: NaiveDate,
    pub visible_start: NaiveDate,
    pub visible_end: NaiveDate,
    pub week_count: u32,
    pub month_labels: Vec<(u32, String)>,
    pub day_counts: std::collections::HashMap<(u32, u8), u64>,
}

/// Period window for rendering.
#[derive(Debug, Clone)]
pub struct PeriodWindow {
    pub start: chrono::NaiveDateTime,
    pub end: chrono::NaiveDateTime,
    pub exact: bool,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Iterate dates from start to end, inclusive.
pub fn iter_dates(start: NaiveDate, end: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    let mut current = start;
    std::iter::from_fn(move || {
        if current > end {
            return None;
        }
        let date = current;
        current += chrono::Duration::days(1);
        Some(date)
    })
}

/// Create the visible week matrix and month labels for a date slice.
pub fn build_year_section(
    day_counts: &std::collections::HashMap<String, u64>,
    start_date: NaiveDate,
    end_date: NaiveDate,
    label: Option<String>,
) -> YearSection {
    let visible_start = sunday_on_or_before(start_date);
    let visible_end = saturday_on_or_after(end_date);
    let week_count = ((visible_end - visible_start).num_days() + 1) as u32 / 7;

    // Build the day counts map for the visible grid
    let mut visible_counts = std::collections::HashMap::new();
    for current in iter_dates(start_date, end_date) {
        let key = current.format("%Y-%m-%d").to_string();
        if let Some(&count) = day_counts.get(&key) {
            if count > 0 {
                let week_index = (current - visible_start).num_days() as u32 / 7;
                let row_index = day_to_row_index(&current);
                visible_counts.insert((week_index, row_index), count);
            }
        }
    }

    // Compute month labels at month boundaries with collision avoidance
    let mut month_labels = Vec::new();
    let mut last_labeled_week: Option<u32> = None;
    for current in iter_dates(start_date, end_date) {
        let week_index = (current - visible_start).num_days() as u32 / 7;
        let should_label = current == start_date || current.day() == 1;
        if should_label {
            if last_labeled_week.map_or(true, |lw| week_index != lw) {
                month_labels.push((week_index, current.format("%b").to_string()));
                last_labeled_week = Some(week_index);
            }
        }
    }

    let section_label =
        label.unwrap_or_else(|| format_section_label(&start_date, &end_date, false));

    YearSection {
        label: section_label,
        range_start: start_date,
        range_end: end_date,
        visible_start,
        visible_end,
        week_count,
        month_labels,
        day_counts: visible_counts,
    }
}

/// Split an explicit date range into year-bounded sections.
pub fn build_sections_for_explicit_range(
    day_counts: &std::collections::HashMap<String, u64>,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Vec<YearSection> {
    let mut sections = Vec::new();
    let mut current_start = start_date;
    let final_end = end_date;

    while current_start <= final_end {
        let year = current_start.year();
        let current_end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
        let actual_end = std::cmp::min(current_end, final_end);

        let section_start = chrono::NaiveDate::from_ymd_opt(
            current_start.year(),
            current_start.month(),
            current_start.day(),
        )
        .unwrap();
        let section_end = chrono::NaiveDate::from_ymd_opt(
            actual_end.year(),
            actual_end.month(),
            actual_end.day(),
        )
        .unwrap();

        let label = format_section_label(&section_start, &section_end, true);
        sections.push(build_year_section(
            day_counts,
            section_start,
            section_end,
            Some(label),
        ));

        current_start = actual_end + chrono::Duration::days(1);
    }

    sections
}

/// Build render sections for either an exact or rolling window.
pub fn build_sections_for_period(
    day_counts: &std::collections::HashMap<String, u64>,
    period_window: &PeriodWindow,
) -> Vec<YearSection> {
    if period_window.exact {
        let start = period_window.start.date();
        let end = period_window.end.date();
        build_sections_for_explicit_range(day_counts, start, end)
    } else {
        let start = period_window.start.date();
        let end = period_window.end.date();
        [build_year_section(
            day_counts,
            start,
            end,
            Some(format_section_label(&start, &end, true)),
        )]
        .into_iter()
        .collect()
    }
}

/// Resolve the period window from the render arguments.
pub fn resolve_period_window(
    from_date: Option<&chrono::DateTime<Utc>>,
    to_date: Option<&chrono::DateTime<Utc>>,
    last_value: Option<&str>,
) -> Result<PeriodWindow, String> {
    let resolved_to = to_date
        .copied()
        .unwrap_or_else(|| Utc::now())
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap();

    // Handle explicit range
    if let Some(from_dt) = from_date {
        let resolved_from = from_dt.date_naive().and_hms_opt(0, 0, 0).unwrap();
        if resolved_from > resolved_to {
            return Err("--from must be on or before --to".to_string());
        }
        return Ok(PeriodWindow {
            start: resolved_from,
            end: resolved_to,
            exact: true,
        });
    }

    // Handle --last window (default: 1y)
    let last = last_value.unwrap_or("1y");
    let (amount, unit) = parse_last_window(last)?;
    let resolved_from =
        shift_datetime_back(&resolved_to, amount, unit).map_err(|e| e.to_string())?;
    Ok(PeriodWindow {
        start: resolved_from,
        end: resolved_to,
        exact: false,
    })
}
