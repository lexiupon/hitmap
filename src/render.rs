//! PNG rendering engine — hitmap grid, labels, and legend.
//!
//! Matches Python layout: fixed cell size from 53-week reference,
//! image dimensions adapt to actual content.

use crate::palette::{Palette, hex_to_rgb};
use crate::scale::{self as scale_mod, ScaleProfile};
use crate::sections::YearSection;
use crate::terminal::{DEFAULT_CELL_HEIGHT_PX, DEFAULT_CELL_WIDTH_PX, TerminalGeometry};
use clap::{Args, ValueEnum};
use image::RgbaImage;
use rusttype::{Font, Scale as FontScale};
use std::cmp::max;

// ---------------------------------------------------------------------------
// RenderArgs
// ---------------------------------------------------------------------------

const RENDER_AFTER_HELP: &str = r#"Examples:
  hitmap
  hitmap --last 90d --author-name "Jane Doe"
  hitmap render --theme dark --color-profile fire
  hitmap render --renderer text
  hitmap render --output hitmap.png

Available color profiles:
  github, aurora, ocean, fire,
  catppuccin-latte, catppuccin-frappe,
  catppuccin-macchiato, catppuccin-mocha

Scale profile examples:
  linear-5-plus
  linear-10-plus
  fibonacci-8-plus
  fibonacci-21-plus"#;

fn validate_date_string(value: &str) -> Result<String, String> {
    crate::parse_date(value).map(|_| value.to_string())
}

fn validate_last_window(value: &str) -> Result<String, String> {
    crate::common::parse_last_window(value)
        .map(|_| value.to_string())
        .map_err(|_| "Use forms like 90d, 12w, 6m, or 1y.".to_string())
}

pub const DEFAULT_RENDERER: Renderer = Renderer::Kitty;
pub const DEFAULT_RENDER_SCALE: f64 = 2.0;
pub const DEFAULT_THEME: &str = "auto";
pub const DEFAULT_COLOR_PROFILE: &str = "github";
pub const DEFAULT_SCALE_PROFILE: &str = "fibonacci-21-plus";
pub const DEFAULT_SCALE_MULTIPLIER: u32 = 1;

pub fn validate_render_scale_number(value: f64) -> Result<f64, String> {
    if value < 1.0 {
        return Err("Render scale must be greater than or equal to 1.0.".to_string());
    }
    Ok(value)
}

fn validate_render_scale(value: &str) -> Result<f64, String> {
    let parsed: f64 = value
        .parse()
        .map_err(|_| "Render scale must be a number greater than or equal to 1.0.".to_string())?;
    validate_render_scale_number(parsed)
}

pub fn validate_theme_name(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "light" | "dark" => Ok(value.trim().to_ascii_lowercase()),
        _ => Err("Theme must be one of: auto, light, dark".to_string()),
    }
}

pub fn validate_scale_profile_name(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    scale_mod::parse_scale_profile(&normalized, 1)
        .map(|_| normalized)
        .map_err(|_| {
            "Use linear-N-plus or fibonacci-N-plus, for example linear-5-plus or fibonacci-21-plus."
                .to_string()
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Renderer {
    Kitty,
    Text,
}

impl std::fmt::Display for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Renderer::Kitty => write!(f, "kitty"),
            Renderer::Text => write!(f, "text"),
        }
    }
}

pub fn parse_renderer_name(value: &str) -> Result<Renderer, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "kitty" => Ok(Renderer::Kitty),
        "text" => Ok(Renderer::Text),
        _ => Err("Renderer must be one of: kitty, text".to_string()),
    }
}

#[derive(Debug, Clone, Args)]
#[command(after_help = RENDER_AFTER_HELP)]
pub struct RenderArgs {
    #[arg(
        default_value = ".",
        value_name = "REPO_PATH",
        help = "Repository path to inspect"
    )]
    pub repo_path: String,

    #[arg(
        long,
        help_heading = "Author selection",
        help = "Render commits for all authors (also the default when no author filter is given)",
        conflicts_with_all = ["author_name", "author_email"]
    )]
    pub all_authors: bool,

    #[arg(
        long,
        value_name = "NAME",
        help_heading = "Author selection",
        help = "Match an exact author name (repeatable, case-insensitive)"
    )]
    pub author_name: Vec<String>,

    #[arg(
        long,
        value_name = "EMAIL",
        help_heading = "Author selection",
        help = "Match an exact author email (repeatable, case-insensitive)"
    )]
    pub author_email: Vec<String>,

    #[arg(
        long,
        value_name = "CELLS",
        help_heading = "Rendering",
        value_parser = clap::value_parser!(u32).range(1..),
        help = "Maximum render width in terminal cells; defaults to terminal width inline, or full content width with --output"
    )]
    pub max_width_cells: Option<u32>,

    #[arg(
        long,
        value_name = "FACTOR",
        help_heading = "Rendering",
        value_parser = validate_render_scale,
        help = "Render the PNG at a higher internal scale for sharper output (default: config value or 2.0)"
    )]
    pub render_scale: Option<f64>,

    #[arg(
        long,
        value_name = "PATH",
        help_heading = "Output",
        help = "Write the rendered PNG to PATH instead of displaying it inline (kitty renderer only)"
    )]
    pub output: Option<String>,

    #[arg(
        long,
        value_name = "BACKEND",
        help_heading = "Rendering",
        value_enum,
        help = "Rendering backend: kitty or text"
    )]
    pub renderer: Option<Renderer>,

    #[arg(
        long,
        help_heading = "Rendering",
        help = "Shortcut for --renderer text"
    )]
    pub text_mode: bool,

    #[arg(
        long,
        value_name = "THEME",
        help_heading = "Rendering",
        value_parser = validate_theme_name,
        help = "Background theme for the image; auto guesses from terminal colors (default: config value or auto)"
    )]
    pub theme: Option<String>,

    #[arg(
        long,
        default_value_t = 0,
        value_name = "COUNT",
        help_heading = "Rendering",
        help = "Number of non-zero color buckets to render; 0 uses the selected scale profile length"
    )]
    pub cell_count: usize,

    #[arg(
        long,
        visible_alias = "color",
        value_name = "PROFILE",
        help_heading = "Rendering",
        value_parser = crate::palette::validate_color_profile_name,
        help = "Color profile for commit cells (default: config value or github)"
    )]
    pub color_profile: Option<String>,

    #[arg(
        long,
        value_name = "PROFILE",
        help_heading = "Rendering",
        value_parser = validate_scale_profile_name,
        help = "Threshold profile. Use linear-N-plus or fibonacci-N-plus (default: config value or fibonacci-21-plus)"
    )]
    pub scale_profile: Option<String>,

    #[arg(
        long,
        value_name = "N",
        help_heading = "Rendering",
        value_parser = clap::value_parser!(u32).range(1..),
        help = "Multiply every threshold in the selected scale profile (default: config value or 1)"
    )]
    pub scale_multiplier: Option<u32>,

    #[arg(
        long,
        visible_alias = "from",
        value_name = "YYYY-MM-DD",
        help_heading = "Time window",
        value_parser = validate_date_string,
        help = "Inclusive start date in YYYY-MM-DD format"
    )]
    pub from_date: Option<String>,

    #[arg(
        long,
        visible_alias = "to",
        value_name = "YYYY-MM-DD",
        help_heading = "Time window",
        value_parser = validate_date_string,
        help = "Inclusive end date in YYYY-MM-DD format; defaults to today"
    )]
    pub to_date: Option<String>,

    #[arg(
        long,
        default_value = "1y",
        value_name = "WINDOW",
        help_heading = "Time window",
        value_parser = validate_last_window,
        help = "Rolling window like 90d, 12w, 6m, or 1y; ignored when --from-date is set"
    )]
    pub last: String,

    #[arg(
        long,
        help_heading = "Output",
        conflicts_with = "verbose",
        help = "Suppress non-error diagnostics"
    )]
    pub quiet: bool,

    #[arg(
        long,
        help_heading = "Output",
        conflicts_with = "quiet",
        help = "Print render configuration and stats"
    )]
    pub verbose: bool,
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

pub struct RenderResult {
    pub png_bytes: Vec<u8>,
    pub display_width_px: u32,
}

#[derive(Debug, Clone)]
struct ResolvedRenderSettings {
    renderer: Renderer,
    max_width_cells: Option<u32>,
    render_scale: f64,
    theme: String,
    color_profile: String,
    scale_profile: String,
    scale_multiplier: u32,
}

impl RenderArgs {
    fn resolve_settings(
        &self,
        config: &crate::config::HitmapConfig,
    ) -> Result<ResolvedRenderSettings, String> {
        let render_config = config.render.as_ref();
        let renderer = if self.text_mode {
            Renderer::Text
        } else if let Some(renderer) = self.renderer {
            renderer
        } else if let Some(renderer) = render_config.and_then(|cfg| cfg.renderer.as_deref()) {
            parse_renderer_name(renderer)?
        } else {
            Renderer::Kitty
        };
        let max_width_cells = self
            .max_width_cells
            .or_else(|| render_config.and_then(|cfg| cfg.max_width_cells));
        let render_scale = self
            .render_scale
            .or_else(|| render_config.and_then(|cfg| cfg.render_scale))
            .unwrap_or(DEFAULT_RENDER_SCALE);
        validate_render_scale_number(render_scale)?;
        let theme = self
            .theme
            .clone()
            .or_else(|| render_config.and_then(|cfg| cfg.theme.clone()))
            .unwrap_or_else(|| DEFAULT_THEME.to_string());
        let theme = validate_theme_name(&theme)?;
        let color_profile = self
            .color_profile
            .clone()
            .or_else(|| render_config.and_then(|cfg| cfg.color_profile.clone()))
            .unwrap_or_else(|| DEFAULT_COLOR_PROFILE.to_string());
        let color_profile = crate::palette::validate_color_profile_name(&color_profile)?;
        let scale_profile = self
            .scale_profile
            .clone()
            .or_else(|| render_config.and_then(|cfg| cfg.scale_profile.clone()))
            .unwrap_or_else(|| DEFAULT_SCALE_PROFILE.to_string());
        let scale_profile = validate_scale_profile_name(&scale_profile)?;
        let scale_multiplier = self
            .scale_multiplier
            .or_else(|| render_config.and_then(|cfg| cfg.scale_multiplier))
            .unwrap_or(DEFAULT_SCALE_MULTIPLIER);
        if scale_multiplier == 0 {
            return Err("Scale multiplier must be greater than or equal to 1.".to_string());
        }

        Ok(ResolvedRenderSettings {
            renderer,
            max_width_cells,
            render_scale,
            theme,
            color_profile,
            scale_profile,
            scale_multiplier,
        })
    }
}

// ---------------------------------------------------------------------------
// Constants (match Python)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_font_from_system() -> Result<Font<'static>, String> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for name in [
        "Menlo Regular",
        "Menlo",
        "DejaVu Sans",
        "Helvetica Neue",
        "Arial",
        "Liberation Mono",
        "Courier New",
    ] {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(name)],
            ..Default::default()
        };
        if let Some(fid) = db.query(&query) {
            if let Some(data) = db.with_face_data(fid, |d, _| d.to_vec()) {
                let leaked = Box::leak(data.into_boxed_slice());
                return Font::try_from_bytes(leaked)
                    .ok_or_else(|| format!("Failed to parse font: {}", name));
            }
        }
    }
    Err("No suitable system font found".to_string())
}

fn draw_filled_rect(img: &mut RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 4]) {
    for y in y0..=y1.min(img.height() - 1) {
        for x in x0..=x1.min(img.width() - 1) {
            img.put_pixel(x, y, image::Rgba(c));
        }
    }
}

/// Draw text; returns advance width.
fn draw_text(
    img: &mut RgbaImage,
    font: &Font,
    text: &str,
    x: f32,
    y: f32,
    color: [u8; 4],
    scale: f32,
) -> f32 {
    let mut cx = x;
    let mut total = 0.0f32;
    for ch in text.chars() {
        let g = font.glyph(ch);
        if g.id().0 == 0 {
            continue;
        }
        let s = g.scaled(FontScale::uniform(scale));
        let adv = s.h_metrics().advance_width;
        let pos = s.positioned(rusttype::point(cx, y));
        if let Some(bb) = pos.pixel_bounding_box() {
            if (bb.max.x - bb.min.x) > 0 && (bb.max.y - bb.min.y) > 0 {
                pos.draw(|px, py, alpha| {
                    let ax = bb.min.x + px as i32;
                    let ay = bb.min.y + py as i32;
                    if ax >= 0 && ay >= 0 && (ax as u32) < img.width() && (ay as u32) < img.height()
                    {
                        let ex = img.get_pixel_mut(ax as u32, ay as u32);
                        let f = alpha * 255.0 / 255.0;
                        ex[0] = (ex[0] as f32 * (1.0 - f) + color[0] as f32 * f).clamp(0.0, 255.0)
                            as u8;
                        ex[1] = (ex[1] as f32 * (1.0 - f) + color[1] as f32 * f).clamp(0.0, 255.0)
                            as u8;
                        ex[2] = (ex[2] as f32 * (1.0 - f) + color[2] as f32 * f).clamp(0.0, 255.0)
                            as u8;
                        ex[3] = (ex[3] as f32 * (1.0 - f) + color[3] as f32 * f).clamp(0.0, 255.0)
                            as u8;
                    }
                });
            }
        }
        cx += adv;
        total += adv;
    }
    total
}

fn text_width(font: &Font, text: &str, scale: f32) -> f32 {
    let mut t = 0.0f32;
    for ch in text.chars() {
        let g = font.glyph(ch);
        if g.id().0 == 0 {
            continue;
        }
        t += g
            .scaled(FontScale::uniform(scale))
            .h_metrics()
            .advance_width;
    }
    t
}

fn text_vertical_bounds(font: &Font, text: &str, scale: f32) -> Option<(i32, i32)> {
    let mut cx = 0.0f32;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;

    for ch in text.chars() {
        let g = font.glyph(ch);
        if g.id().0 == 0 {
            continue;
        }
        let s = g.scaled(FontScale::uniform(scale));
        let adv = s.h_metrics().advance_width;
        let pos = s.positioned(rusttype::point(cx, 0.0));
        if let Some(bb) = pos.pixel_bounding_box() {
            min_y = min_y.min(bb.min.y);
            max_y = max_y.max(bb.max.y);
        }
        cx += adv;
    }

    if min_y <= max_y {
        Some((min_y, max_y))
    } else {
        None
    }
}

fn centered_text_baseline_y(
    font: &Font,
    text: &str,
    scale: f32,
    box_top: u32,
    box_height: u32,
) -> f32 {
    let box_center = box_top as f32 + box_height as f32 / 2.0;
    if let Some((min_y, max_y)) = text_vertical_bounds(font, text, scale) {
        let text_center = (min_y as f32 + max_y as f32) / 2.0;
        box_center - text_center
    } else {
        box_center
    }
}

fn text_width_px(font: &Font, text: &str, scale: f32) -> u32 {
    text_width(font, text, scale).ceil().max(0.0) as u32
}

fn legend_item_width(
    font: &Font,
    label: &str,
    scale: f32,
    legend_cell_size: u32,
    legend_item_padding: u32,
) -> u32 {
    legend_cell_size + legend_item_padding + text_width_px(font, label, scale)
}

fn legend_line_width(
    font: &Font,
    items: &[(String, &str)],
    scale: f32,
    legend_cell_size: u32,
    legend_item_padding: u32,
    legend_item_gap: u32,
) -> u32 {
    let mut total_width = 0;
    for (index, (label, _)) in items.iter().enumerate() {
        if index > 0 {
            total_width += legend_item_gap;
        }
        total_width += legend_item_width(font, label, scale, legend_cell_size, legend_item_padding);
    }
    total_width
}

fn build_legend_items_for_thresholds<'a>(
    palette: &'a Palette,
    thresholds: &[u64],
) -> Vec<(String, &'a str)> {
    let mut items = Vec::with_capacity(thresholds.len() + 1);
    items.push(("0".to_string(), palette.empty_cell.as_str()));
    for (index, threshold) in thresholds.iter().enumerate() {
        let label = if index == thresholds.len() - 1 {
            format!("{}+", threshold)
        } else {
            threshold.to_string()
        };
        items.push((label, palette.cells[index].as_str()));
    }
    items
}

fn resolve_effective_nonzero_level_count(
    font: &Font,
    palette: &Palette,
    thresholds: &[u64],
    scale: f32,
    legend_cell_size: u32,
    legend_item_padding: u32,
    legend_item_gap: u32,
    max_width: u32,
) -> usize {
    let max_levels = thresholds.len().min(palette.cells.len());
    if max_levels == 0 {
        return 0;
    }

    let mut level_count = max_levels;
    loop {
        let legend_items = build_legend_items_for_thresholds(palette, &thresholds[..level_count]);
        let width = legend_line_width(
            font,
            &legend_items,
            scale,
            legend_cell_size,
            legend_item_padding,
            legend_item_gap,
        );
        if width <= max_width || level_count == 1 {
            return level_count;
        }
        level_count -= 1;
    }
}

fn scaled_px(value: f32, rs: f64) -> u32 {
    (value * rs as f32).round().max(1.0) as u32
}

fn fallback_output_geometry() -> TerminalGeometry {
    TerminalGeometry {
        rows: 40,
        cols: 120,
        width_px: 1200,
        height_px: 800,
        cell_width_px: DEFAULT_CELL_WIDTH_PX,
        cell_height_px: DEFAULT_CELL_HEIGHT_PX,
    }
}

fn resolve_render_target_width_px(
    output_mode: bool,
    requested_width_cells: Option<u32>,
    geometry: &TerminalGeometry,
) -> u32 {
    if output_mode {
        return requested_width_cells
            .map(|cells| ((cells as f64) * geometry.cell_width_px.max(1.0)).round() as u32)
            .unwrap_or(u32::MAX / 4)
            .max(1);
    }

    let max_placement_cols = crate::terminal::resolve_width_cells(requested_width_cells, geometry);
    crate::terminal::resolve_target_width_px(max_placement_cols, geometry)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render_hitmap_png(
    sections: &[YearSection],
    palette: &Palette,
    scale_profile: &ScaleProfile,
    render_scale: f64,
    max_display_width_px: u32,
    _geometry: &TerminalGeometry,
) -> Result<RenderResult, String> {
    let font = load_font_from_system().map_err(|e| format!("Font load failed: {}", e))?;

    // Fixed base font size (Python: BASE_FONT_SIZE_PX = 11)
    const BASE_FONT_SIZE_PX: f64 = 11.0;
    let font_size: f32 = (BASE_FONT_SIZE_PX * render_scale) as f32;
    let font_size = font_size.max(8.0).min(58.0);
    let line_height = font_size as u32;

    // --- Scaled layout constants ---
    let outer_padding_x = scaled_px(16.0, render_scale);
    let outer_padding_y = scaled_px(14.0, render_scale);
    let row_label_width = max(
        text_width(&font, "Mon", font_size) as u32,
        text_width(&font, "Fri", font_size) as u32,
    );
    let row_label_gap = scaled_px(8.0, render_scale);
    let grid_gap = if max_display_width_px >= 720 {
        scaled_px(3.0, render_scale)
    } else {
        scaled_px(2.0, render_scale)
    };

    // Fixed cell/font regardless of terminal width
    let cell_size = (font_size * 1.0).round() as u32;
    let max_weeks = sections.iter().map(|s| s.week_count).max().unwrap_or(53);
    let cell_step = cell_size + grid_gap;

    // Image width from fixed reference (not terminal width)

    // --- Section layout heights ---
    let section_title_height = line_height + scaled_px(10.0, render_scale);
    let month_row_height = line_height + scaled_px(6.0, render_scale);
    let grid_height = 7 * cell_size + 6 * grid_gap;
    let section_gap = line_height + scaled_px(14.0, render_scale);

    // --- Grid width / image width ---
    let grid_left = outer_padding_x + row_label_width + row_label_gap;
    let grid_width = max_weeks as u32 * cell_size + max(0, max_weeks as u32 - 1) * grid_gap;

    // --- Image width: match Python's raster_width_px ---
    let max_raster_width_px = (max_display_width_px as f64 * render_scale).round() as u32;
    let grid_content_width =
        outer_padding_x + row_label_width + row_label_gap + grid_width + outer_padding_x;
    let image_width = max_raster_width_px.min(grid_content_width);
    let grid_right = image_width.saturating_sub(outer_padding_x);

    // --- Legend ---
    let legend_cell_size = (cell_size as f32)
        .min(scaled_px(14.0, render_scale) as f32)
        .max(scaled_px(8.0, render_scale) as f32) as u32;
    let legend_item_padding = scaled_px(4.0, render_scale);
    let legend_item_gap = scaled_px(12.0, render_scale);
    let legend_line_height = legend_cell_size.max(line_height) + scaled_px(6.0, render_scale);
    let legend_top_gap = line_height + scaled_px(6.0, render_scale);
    let available_legend_width = grid_right.saturating_sub(grid_left).max(1);
    let effective_nonzero_levels = resolve_effective_nonzero_level_count(
        &font,
        palette,
        &scale_profile.thresholds,
        font_size,
        legend_cell_size,
        legend_item_padding,
        legend_item_gap,
        available_legend_width,
    );
    let effective_thresholds: Vec<u64> = scale_profile
        .thresholds
        .iter()
        .copied()
        .take(effective_nonzero_levels)
        .collect();
    let legend_items = build_legend_items_for_thresholds(palette, &effective_thresholds);
    let legend_width = legend_line_width(
        &font,
        &legend_items,
        font_size,
        legend_cell_size,
        legend_item_padding,
        legend_item_gap,
    );

    // --- Allocate generously, trim after drawing ---
    let safe_height = outer_padding_y * 3
        + (sections.len() as u32) * (section_title_height + month_row_height + grid_height) * 2
        + (sections.len().max(1) as u32 - 1) * section_gap * 2
        + legend_top_gap
        + legend_line_height * 2;
    let mut image = RgbaImage::new(image_width, safe_height);

    // Colors
    let bg_color: [u8; 4] = {
        let (r, g, b) = hex_to_rgb(&palette.background);
        [r, g, b, 255]
    };
    let text_color: [u8; 4] = {
        let (r, g, b) = hex_to_rgb(&palette.text);
        [r, g, b, 255]
    };
    let subtle_color: [u8; 4] = {
        let (r, g, b) = hex_to_rgb(&palette.subtle_text);
        [r, g, b, 255]
    };
    let empty_color: [u8; 4] = {
        let r = hex_to_rgb(&palette.empty_cell);
        [r.0, r.1, r.2, 255]
    };

    for y in 0..safe_height {
        for x in 0..image_width {
            image.put_pixel(x, y, image::Rgba(bg_color));
        }
    }

    // --- Draw sections sequentially ---
    let mut current_y = outer_padding_y;

    for (si, section) in sections.iter().enumerate() {
        // Section title
        draw_text(
            &mut image,
            &font,
            &section.label,
            grid_left as f32,
            current_y as f32,
            text_color,
            font_size,
        );
        current_y += section_title_height;

        // Month labels
        let month_y = current_y;
        let mut last_right: f32 = -100.0;
        for (wk, ml) in &section.month_labels {
            let x = grid_left as f32 + (*wk as f32) * cell_step as f32;
            let w = text_width(&font, ml, font_size);
            if x <= last_right + 4.0 {
                continue;
            }
            if x + w < image_width as f32 {
                draw_text(
                    &mut image,
                    &font,
                    ml,
                    x,
                    month_y as f32,
                    subtle_color,
                    font_size,
                );
                last_right = x + w;
            }
        }
        current_y += month_row_height;

        // Day labels (Mon, Wed, Fri)
        for (ri, dl) in &[(1u8, "Mon"), (3u8, "Wed"), (5u8, "Fri")] {
            let label_box_top = current_y + (*ri as u32) * cell_step;
            let y = centered_text_baseline_y(&font, dl, font_size, label_box_top, cell_size);
            draw_text(
                &mut image,
                &font,
                dl,
                outer_padding_x as f32,
                y,
                subtle_color,
                font_size,
            );
        }

        // Grid
        let grid_y = current_y;
        for col in 0..section.week_count {
            for row in 0..7u32 {
                let cell_date =
                    section.visible_start + chrono::Duration::days((col * 7 + row) as i64);
                if cell_date < section.range_start || cell_date > section.range_end {
                    continue;
                }

                let x = grid_left + col * cell_size + col * grid_gap;
                let y = grid_y + row * cell_size + row * grid_gap;
                let count = section
                    .day_counts
                    .get(&(col, row as u8))
                    .copied()
                    .unwrap_or(0);
                let bucket = crate::scale::bucket_for_value(&effective_thresholds, count);
                let cc = if bucket == 0 {
                    empty_color
                } else {
                    let r = hex_to_rgb(&palette.cells[bucket - 1]);
                    [r.0, r.1, r.2, 255]
                };
                draw_filled_rect(&mut image, x, y, x + cell_size - 1, y + cell_size - 1, cc);
            }
        }
        current_y += grid_height;

        if si < sections.len() - 1 {
            current_y += section_gap;
        }
    }

    // --- Legend ---
    let legend_y = current_y + legend_top_gap;
    let mut legend_x = grid_right.saturating_sub(legend_width);
    if legend_x < grid_left {
        legend_x = grid_left;
    }
    let mut lx = legend_x;
    for (label, color) in &legend_items {
        let rgb = hex_to_rgb(color);
        draw_filled_rect(
            &mut image,
            lx,
            legend_y,
            lx + legend_cell_size - 1,
            legend_y + legend_cell_size - 1,
            [rgb.0, rgb.1, rgb.2, 255],
        );
        let tx = lx + legend_cell_size + legend_item_padding;
        let ty = centered_text_baseline_y(&font, label, font_size, legend_y, legend_cell_size);
        draw_text(
            &mut image,
            &font,
            label,
            tx as f32,
            ty,
            subtle_color,
            font_size,
        );
        lx += legend_item_width(
            &font,
            label,
            font_size,
            legend_cell_size,
            legend_item_padding,
        ) + legend_item_gap;
    }
    current_y = legend_y + legend_line_height;

    // --- Crop to actual content height ---
    let crop_h = (current_y + outer_padding_y).min(image.height());
    if crop_h < image.height() {
        let mut trimmed = RgbaImage::new(image_width, crop_h);
        for y in 0..crop_h {
            for x in 0..image_width {
                trimmed.put_pixel(x, y, *image.get_pixel(x, y));
            }
        }
        let _ = std::mem::replace(&mut image, trimmed);
    }
    let image_height = image.height();

    // --- Encode PNG ---
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    let mut png_bytes = Vec::new();
    {
        let enc = PngEncoder::new(&mut png_bytes);
        enc.write_image(
            &image.into_raw(),
            image_width,
            image_height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("PNG encode failed: {}", e))?;
    }

    Ok(RenderResult {
        png_bytes,
        display_width_px: image_width,
    })
}

// ---------------------------------------------------------------------------
// render_command
// ---------------------------------------------------------------------------

pub fn render_command(args: RenderArgs) -> Result<(), String> {
    let config = crate::config::load_config()?;
    let settings = args.resolve_settings(&config)?;

    if args.verbose {
        eprintln!("Render args: {:?}", args);
        eprintln!("Resolved render settings: {:?}", settings);
    }

    let repo_root = crate::git::resolve_repo_root(&args.repo_path).map_err(|e| e.to_string())?;
    let selection = crate::common::resolve_author_selection(
        args.all_authors,
        &args.author_name,
        &args.author_email,
        true,
    )?;
    let author_desc = crate::common::describe_author_selection(&selection);

    let from_date = args
        .from_date
        .as_ref()
        .map(|s| crate::parse_date(s))
        .transpose()?;
    let to_date = args
        .to_date
        .as_ref()
        .map(|s| crate::parse_date(s))
        .transpose()?;
    let period_window = crate::sections::resolve_period_window(
        from_date.as_ref(),
        to_date.as_ref(),
        Some(&args.last),
    )?;
    crate::git::ensure_author_selection_exists(&repo_root.path, &selection)
        .map_err(|e| e.to_string())?;
    let (day_counts, total_commits, _) = crate::git::collect_commit_day_counts(
        &repo_root.path,
        &selection,
        &period_window.start.and_utc(),
        &period_window.end.and_utc(),
    )
    .map_err(|e| e.to_string())?;
    let section_info = crate::sections::build_sections_for_period(&day_counts, &period_window);

    let scale_profile =
        scale_mod::parse_scale_profile(&settings.scale_profile, settings.scale_multiplier)?;
    let cell_count = if args.cell_count > 0 {
        args.cell_count
    } else {
        scale_profile.thresholds.len()
    };
    let effective_thresholds: Vec<u64> = scale_profile
        .thresholds
        .iter()
        .copied()
        .take(cell_count)
        .collect();
    let dark_mode = crate::common::resolve_dark_mode(&settings.theme);
    let resolved_color_profile =
        crate::palette::canonical_color_profile_name(&settings.color_profile, dark_mode);
    let renderer = settings.renderer;

    if args.verbose {
        eprintln!("Renderer: {:?}", renderer);
        eprintln!("Total commits: {}", total_commits);
        eprintln!("Author filter: {}", author_desc);
        eprintln!(
            "Sections: {} (total weeks: {})",
            section_info.len(),
            section_info.iter().map(|s| s.week_count).sum::<u32>()
        );
        for s in &section_info {
            eprintln!(
                "  {}  {}..{}  (visible: {}..{})",
                s.label,
                s.range_start.format("%Y-%m-%d"),
                s.range_end.format("%Y-%m-%d"),
                s.visible_start.format("%Y-%m-%d"),
                s.visible_end.format("%Y-%m-%d")
            );
        }
    }

    if matches!(renderer, Renderer::Text) {
        if args.output.is_some() {
            return Err("--output is only supported with the kitty renderer; redirect stdout for text output".to_string());
        }
        let rendered = crate::text::render_hitmap_text(
            &section_info,
            &effective_thresholds,
            &resolved_color_profile,
            dark_mode,
            settings.max_width_cells,
        )?;
        print!("{}\n", rendered);
        return Ok(());
    }

    let palette = crate::palette::load_palette(dark_mode, cell_count, &resolved_color_profile)?;
    let output_mode = args.output.is_some();
    let (tty_fd, geometry) = if output_mode {
        let geometry = crate::terminal::open_render_tty()
            .ok()
            .map(|fd| {
                let geometry = crate::terminal::get_terminal_geometry(fd);
                unsafe {
                    libc::close(fd);
                }
                geometry
            })
            .unwrap_or_else(fallback_output_geometry);
        (None, geometry)
    } else {
        let fd = crate::terminal::open_render_tty().map_err(|e| format!("{}", e))?;
        let probe = crate::terminal::probe_kitty_graphics(fd, 0.35)
            .map_err(|e| format!("Terminal probe failed: {}", e))?;
        if !probe.graphics_seen {
            return Err(
                "This terminal did not answer the Kitty graphics protocol query.\
                Run `hitmap render` in kitty, Ghostty, or another compatible terminal."
                    .to_string(),
            );
        }
        let g = crate::terminal::get_terminal_geometry(fd);
        (Some(fd), g)
    };

    let max_display =
        resolve_render_target_width_px(output_mode, settings.max_width_cells, &geometry);
    let result = render_hitmap_png(
        &section_info,
        &palette,
        &scale_profile,
        settings.render_scale,
        max_display,
        &geometry,
    )?;
    let placement_cols = if output_mode {
        None
    } else {
        let max_placement_cols =
            crate::terminal::resolve_width_cells(settings.max_width_cells, &geometry);
        Some(crate::terminal::resolve_placement_cols(
            result.display_width_px,
            max_placement_cols,
            &geometry,
        ))
    };

    if args.verbose {
        eprintln!("PNG size: {} bytes", result.png_bytes.len());
    }

    if let Some(p) = &args.output {
        std::fs::write(p, &result.png_bytes).map_err(|e| format!("Write failed: {}", e))?;
    } else if let Some(fd) = tty_fd {
        crate::terminal::display_png_via_kitty(
            fd,
            &result.png_bytes,
            placement_cols.expect("inline kitty rendering requires placement columns"),
        )
        .map_err(|e| format!("Display failed: {}", e))?;
    } else {
        return Err("No output path and no TTY".to_string());
    }

    if let Some(fd) = tty_fd {
        unsafe {
            libc::close(fd);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_COLOR_PROFILE, DEFAULT_RENDER_SCALE, DEFAULT_RENDERER, DEFAULT_SCALE_MULTIPLIER,
        DEFAULT_SCALE_PROFILE, DEFAULT_THEME, RenderArgs, Renderer, render_hitmap_png,
        resolve_render_target_width_px,
    };
    use crate::{
        config::{HitmapConfig, RenderConfig},
        palette::load_palette,
        scale::ScaleProfile,
        sections::YearSection,
        terminal::TerminalGeometry,
    };
    use chrono::{Duration, NaiveDate};
    use image::load_from_memory;
    use std::collections::HashMap;

    fn sample_geometry() -> TerminalGeometry {
        TerminalGeometry {
            rows: 40,
            cols: 120,
            width_px: 1200,
            height_px: 800,
            cell_width_px: 10.0,
            cell_height_px: 20.0,
        }
    }

    fn long_section(week_count: u32) -> YearSection {
        let visible_start = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let visible_end = visible_start + Duration::days((week_count as i64 * 7) - 1);
        YearSection {
            label: "2023-2024".to_string(),
            range_start: visible_start,
            range_end: visible_end,
            visible_start,
            visible_end,
            week_count,
            month_labels: Vec::new(),
            day_counts: HashMap::new(),
        }
    }

    fn sample_scale_profile() -> ScaleProfile {
        ScaleProfile {
            thresholds: vec![1, 2, 3, 5, 8],
        }
    }

    fn sample_render_args() -> RenderArgs {
        RenderArgs {
            repo_path: ".".to_string(),
            all_authors: false,
            author_name: Vec::new(),
            author_email: Vec::new(),
            max_width_cells: None,
            render_scale: None,
            output: None,
            renderer: None,
            text_mode: false,
            theme: None,
            cell_count: 0,
            color_profile: None,
            scale_profile: None,
            scale_multiplier: None,
            from_date: None,
            to_date: None,
            last: "1y".to_string(),
            quiet: false,
            verbose: false,
        }
    }

    #[test]
    fn resolve_settings_uses_config_when_cli_is_missing() {
        let args = sample_render_args();
        let config = HitmapConfig {
            render: Some(RenderConfig {
                renderer: Some("text".to_string()),
                theme: Some("dark".to_string()),
                color_profile: Some("ocean".to_string()),
                scale_profile: Some("linear-10-plus".to_string()),
                scale_multiplier: Some(3),
                render_scale: Some(3.0),
                max_width_cells: Some(88),
            }),
            authors: None,
            doctor: None,
        };

        let resolved = args.resolve_settings(&config).unwrap();
        assert_eq!(resolved.renderer, Renderer::Text);
        assert_eq!(resolved.theme, "dark");
        assert_eq!(resolved.color_profile, "ocean");
        assert_eq!(resolved.scale_profile, "linear-10-plus");
        assert_eq!(resolved.scale_multiplier, 3);
        assert_eq!(resolved.render_scale, 3.0);
        assert_eq!(resolved.max_width_cells, Some(88));
    }

    #[test]
    fn resolve_settings_prefers_cli_over_config_and_falls_back_to_defaults() {
        let mut args = sample_render_args();
        args.renderer = Some(Renderer::Text);
        args.theme = Some("light".to_string());
        args.color_profile = Some("fire".to_string());
        args.scale_profile = Some("linear-5-plus".to_string());
        args.scale_multiplier = Some(2);
        args.render_scale = Some(4.0);
        args.max_width_cells = Some(99);

        let config = HitmapConfig {
            render: Some(RenderConfig {
                renderer: Some("kitty".to_string()),
                theme: Some("dark".to_string()),
                color_profile: Some("ocean".to_string()),
                scale_profile: Some("linear-10-plus".to_string()),
                scale_multiplier: Some(3),
                render_scale: Some(3.0),
                max_width_cells: Some(88),
            }),
            authors: None,
            doctor: None,
        };

        let resolved = args.resolve_settings(&config).unwrap();
        assert_eq!(resolved.renderer, Renderer::Text);
        assert_eq!(resolved.theme, "light");
        assert_eq!(resolved.color_profile, "fire");
        assert_eq!(resolved.scale_profile, "linear-5-plus");
        assert_eq!(resolved.scale_multiplier, 2);
        assert_eq!(resolved.render_scale, 4.0);
        assert_eq!(resolved.max_width_cells, Some(99));

        let defaults = sample_render_args()
            .resolve_settings(&HitmapConfig::default())
            .unwrap();
        assert_eq!(defaults.renderer, DEFAULT_RENDERER);
        assert_eq!(defaults.theme, DEFAULT_THEME);
        assert_eq!(defaults.color_profile, DEFAULT_COLOR_PROFILE);
        assert_eq!(defaults.scale_profile, DEFAULT_SCALE_PROFILE);
        assert_eq!(defaults.scale_multiplier, DEFAULT_SCALE_MULTIPLIER);
        assert_eq!(defaults.render_scale, DEFAULT_RENDER_SCALE);
        assert_eq!(defaults.max_width_cells, None);
    }

    #[test]
    fn output_png_defaults_to_unconstrained_content_width() {
        let width = resolve_render_target_width_px(true, None, &sample_geometry());
        assert_eq!(width, u32::MAX / 4);
    }

    #[test]
    fn output_png_honors_explicit_width_without_terminal_cap() {
        let width = resolve_render_target_width_px(true, Some(200), &sample_geometry());
        assert_eq!(width, 2000);
    }

    #[test]
    fn inline_render_still_uses_terminal_width_defaults() {
        let width = resolve_render_target_width_px(false, None, &sample_geometry());
        assert_eq!(width, 1080);
    }

    #[test]
    fn long_output_png_is_not_clipped_to_inline_terminal_width() {
        let geometry = sample_geometry();
        let palette = load_palette(false, 5, "github").unwrap();
        let scale_profile = sample_scale_profile();
        let sections = vec![long_section(105)];

        let inline_result = render_hitmap_png(
            &sections,
            &palette,
            &scale_profile,
            2.0,
            resolve_render_target_width_px(false, None, &geometry),
            &geometry,
        )
        .unwrap();
        let output_result = render_hitmap_png(
            &sections,
            &palette,
            &scale_profile,
            2.0,
            resolve_render_target_width_px(true, None, &geometry),
            &geometry,
        )
        .unwrap();

        let inline_png = load_from_memory(&inline_result.png_bytes).unwrap();
        let output_png = load_from_memory(&output_result.png_bytes).unwrap();

        assert_eq!(inline_png.width(), inline_result.display_width_px);
        assert_eq!(output_png.width(), output_result.display_width_px);
        assert!(
            output_png.width() > inline_png.width(),
            "output PNG width {} should exceed inline width {} for long windows",
            output_png.width(),
            inline_png.width()
        );
    }

    #[test]
    fn output_png_still_honors_explicit_width_cap() {
        let geometry = sample_geometry();
        let palette = load_palette(false, 5, "github").unwrap();
        let scale_profile = sample_scale_profile();
        let sections = vec![long_section(105)];

        let uncapped = render_hitmap_png(
            &sections,
            &palette,
            &scale_profile,
            2.0,
            resolve_render_target_width_px(true, None, &geometry),
            &geometry,
        )
        .unwrap();
        let capped = render_hitmap_png(
            &sections,
            &palette,
            &scale_profile,
            2.0,
            resolve_render_target_width_px(true, Some(100), &geometry),
            &geometry,
        )
        .unwrap();

        let capped_png = load_from_memory(&capped.png_bytes).unwrap();
        assert_eq!(capped_png.width(), capped.display_width_px);
        assert!(capped_png.width() < uncapped.display_width_px);
        assert_eq!(capped_png.width(), 2000);
    }
}
