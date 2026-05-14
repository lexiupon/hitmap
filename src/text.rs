//! Terminal text rendering backend using Unicode glyphs and ANSI colors.

use crate::palette::{hex_to_rgb, load_palette};
use crate::scale::bucket_for_value;
use crate::sections::YearSection;
use std::io::{self, IsTerminal};
use std::os::fd::AsRawFd;

const EMPTY_GLYPH: &str = "·";
const FILLED_GLYPH: &str = "■";
const MONO_GLYPHS: [&str; 4] = ["▫", "▪", "◾", "■"];
const ROW_LABEL_WIDTH: usize = 4;
const TEXT_CELL_WIDTH: usize = 2;

fn color_enabled() -> bool {
    let stdout = io::stdout();
    if !stdout.is_terminal() {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    !matches!(std::env::var("TERM"), Ok(term) if term.eq_ignore_ascii_case("dumb"))
}

fn style_truecolor(text: &str, color: &str) -> String {
    let (r, g, b) = hex_to_rgb(color);
    format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text)
}

fn fallback_glyph(bucket: usize, max_bucket: usize) -> &'static str {
    if bucket == 0 {
        return EMPTY_GLYPH;
    }
    if max_bucket <= 1 {
        return FILLED_GLYPH;
    }
    let index = ((bucket - 1) * MONO_GLYPHS.len()) / max_bucket;
    MONO_GLYPHS[index.min(MONO_GLYPHS.len() - 1)]
}

fn resolve_layout_width(max_weeks: u32, requested_width: Option<u32>) -> Result<usize, String> {
    let stdout = io::stdout();
    let tty_width = if stdout.is_terminal() {
        crate::terminal::get_terminal_geometry(stdout.as_raw_fd()).cols as usize
    } else {
        120
    };
    let width_limit = requested_width.map(|v| v as usize).unwrap_or(tty_width);
    let required_width = ROW_LABEL_WIDTH + max_weeks as usize * TEXT_CELL_WIDTH;

    if required_width <= width_limit {
        Ok(required_width)
    } else {
        Err(format!(
            "Terminal is too narrow for a text hitmap (need at least {} columns; text mode keeps a fixed cell width)",
            required_width
        ))
    }
}

fn month_line(section: &YearSection, cell_width: usize) -> String {
    let mut chars = vec![' '; ROW_LABEL_WIDTH + section.week_count as usize * cell_width];
    for (week, label) in &section.month_labels {
        let start = ROW_LABEL_WIDTH + *week as usize * cell_width;
        for (offset, ch) in label.chars().enumerate() {
            if start + offset < chars.len() {
                chars[start + offset] = ch;
            }
        }
    }
    chars.into_iter().collect::<String>().trim_end().to_string()
}

fn row_prefix(row: u8) -> &'static str {
    match row {
        1 => "Mon ",
        3 => "Wed ",
        5 => "Fri ",
        _ => "    ",
    }
}

fn render_filled_cell(
    bucket: usize,
    thresholds_len: usize,
    color: &str,
    use_color: bool,
) -> String {
    if use_color {
        style_truecolor(FILLED_GLYPH, color)
    } else {
        fallback_glyph(bucket, thresholds_len).to_string()
    }
}

fn render_row(
    section: &YearSection,
    row: u8,
    cell_width: usize,
    thresholds: &[u64],
    colors: &[String],
    use_color: bool,
) -> String {
    let mut line = String::new();
    line.push_str(row_prefix(row));

    for col in 0..section.week_count {
        let cell_date =
            section.visible_start + chrono::Duration::days((col * 7 + row as u32) as i64);
        let rendered = if cell_date < section.range_start || cell_date > section.range_end {
            " ".to_string()
        } else {
            let count = section.day_counts.get(&(col, row)).copied().unwrap_or(0);
            if count == 0 {
                EMPTY_GLYPH.to_string()
            } else {
                let bucket = bucket_for_value(thresholds, count);
                let color = &colors[bucket.saturating_sub(1)];
                render_filled_cell(bucket, thresholds.len(), color, use_color)
            }
        };

        line.push_str(&rendered);
        if cell_width == 2 {
            line.push(' ');
        }
    }

    line
}

fn render_legend(
    thresholds: &[u64],
    colors: &[String],
    use_color: bool,
    layout_width: usize,
) -> String {
    let mut parts: Vec<(String, usize)> = vec![(
        format!("{} 0", EMPTY_GLYPH),
        format!("{} 0", EMPTY_GLYPH).chars().count(),
    )];
    for (index, threshold) in thresholds.iter().enumerate() {
        let bucket = index + 1;
        let marker = render_filled_cell(bucket, thresholds.len(), &colors[index], use_color);
        let plain = if index == thresholds.len() - 1 {
            format!("{} {}+", FILLED_GLYPH, threshold)
        } else {
            format!("{} {}", FILLED_GLYPH, threshold)
        };
        let rendered = if index == thresholds.len() - 1 {
            format!("{} {}+", marker, threshold)
        } else {
            format!("{} {}", marker, threshold)
        };
        parts.push((rendered, plain.chars().count()));
    }

    let display_width =
        parts.iter().map(|(_, width)| *width).sum::<usize>() + 2 * parts.len().saturating_sub(1);
    let left_padding = " ".repeat(layout_width.saturating_sub(display_width));
    let content = parts
        .into_iter()
        .map(|(text, _)| text)
        .collect::<Vec<_>>()
        .join("  ");
    format!("{}{}", left_padding, content)
}

fn resolve_text_colors(
    dark_mode: bool,
    threshold_count: usize,
    color_profile: &str,
) -> Result<Vec<String>, String> {
    Ok(load_palette(dark_mode, threshold_count, color_profile)?.cells)
}

pub fn render_hitmap_text(
    sections: &[YearSection],
    thresholds: &[u64],
    color_profile: &str,
    dark_mode: bool,
    max_width_cells: Option<u32>,
) -> Result<String, String> {
    if sections.is_empty() {
        return Err("No hitmap sections were generated".to_string());
    }
    if thresholds.is_empty() {
        return Err("Text renderer requires at least one threshold".to_string());
    }

    let max_weeks = sections.iter().map(|s| s.week_count).max().unwrap_or(53);
    let layout_width = resolve_layout_width(max_weeks, max_width_cells)?;
    let cell_width = TEXT_CELL_WIDTH;
    let colors = resolve_text_colors(dark_mode, thresholds.len(), color_profile)?;
    let use_color = color_enabled();

    let mut lines = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.push(section.label.clone());
        lines.push(month_line(section, cell_width));
        for row in 0..7u8 {
            lines.push(render_row(
                section, row, cell_width, thresholds, &colors, use_color,
            ));
        }
    }

    lines.push(String::new());
    lines.push(render_legend(thresholds, &colors, use_color, layout_width));
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn sample_section() -> YearSection {
        let visible_start = NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        let mut day_counts = HashMap::new();
        day_counts.insert((0, 0), 1);
        day_counts.insert((0, 1), 3);
        day_counts.insert((1, 3), 7);
        YearSection {
            label: "2026-01-04 - 2026-01-17".to_string(),
            range_start: visible_start,
            range_end: visible_start + chrono::Duration::days(13),
            visible_start,
            visible_end: visible_start + chrono::Duration::days(13),
            week_count: 2,
            month_labels: vec![(0, "Jan".to_string())],
            day_counts,
        }
    }

    #[test]
    fn month_line_places_labels_on_week_boundaries() {
        let section = sample_section();
        assert_eq!(month_line(&section, 2), "    Jan");
    }

    #[test]
    fn text_render_contains_section_and_legend() {
        let rendered =
            render_hitmap_text(&[sample_section()], &[1, 3, 5], "github", false, Some(80)).unwrap();
        assert!(rendered.contains("2026-01-04 - 2026-01-17"));
        assert!(rendered.contains("· 0"));
        assert!(!rendered.contains("Legend:"));
        assert!(rendered.contains("Mon "));
    }

    #[test]
    fn text_render_uses_fixed_cell_width() {
        let err = render_hitmap_text(&[sample_section()], &[1, 3, 5], "github", false, Some(7))
            .unwrap_err();
        assert!(err.contains("fixed cell width"));
    }

    #[test]
    fn legend_is_right_aligned_within_layout() {
        let legend = render_legend(&[1], &["#40c463".to_string()], false, 20);
        assert!(legend.starts_with(" "));
        assert!(legend.trim_start().starts_with("· 0"));
        assert!(!legend.contains("Legend:"));
    }

    #[test]
    fn text_colors_match_palette_bucket_order_for_github_dark() {
        let colors = resolve_text_colors(true, 7, "github").unwrap();
        assert_eq!(colors[0], "#0e4429");
        assert_eq!(colors[1], "#006d32");
        assert_eq!(colors[2], "#26a641");
        assert_eq!(colors[3], "#39d353");
        assert_eq!(colors[4], "#52d989");
        assert_eq!(colors[5], "#52d989");
        assert_eq!(colors[6], "#52d989");
    }

    #[test]
    fn text_colors_match_kitty_palette_for_all_profiles_and_themes() {
        for dark_mode in [false, true] {
            for profile_name in crate::palette::COLOR_PROFILE_NAMES {
                let expected = load_palette(dark_mode, 7, profile_name).unwrap().cells;
                let actual = resolve_text_colors(dark_mode, 7, profile_name).unwrap();
                assert_eq!(
                    actual, expected,
                    "profile={} dark_mode={}",
                    profile_name, dark_mode
                );
            }
        }
    }
}
