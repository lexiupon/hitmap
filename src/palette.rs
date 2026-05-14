//! Color profiles, palettes, and perceptual color interpolation.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Color profile definitions
// ---------------------------------------------------------------------------

/// A 5-stop positive color scale: low → med-low → med-high → high → max.
pub type ColorStops = [&'static str; 5];

/// Theme-specific palette data for a color profile.
#[derive(Debug, Clone, Copy)]
pub struct ThemeProfile {
    pub background: &'static str,
    pub text: &'static str,
    pub subtle_text: &'static str,
    pub empty_cell: &'static str,
    pub cells: ColorStops,
}

/// A color profile with light and dark theme variants.
#[derive(Debug, Clone, Copy)]
pub struct ColorProfile {
    pub light: ThemeProfile,
    pub dark: ThemeProfile,
}

/// Supported color profile names shown in CLI help and validation errors.
pub const COLOR_PROFILE_NAMES: &[&str] = &[
    "github",
    "aurora",
    "ocean",
    "fire",
    "catppuccin-latte",
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "catppuccin-mocha",
];

/// Human-readable color profile list for help text.
pub const COLOR_PROFILE_NAMES_HELP: &str = "github, aurora, ocean, fire, catppuccin-latte, catppuccin-frappe, \
     catppuccin-macchiato, catppuccin-mocha";

const DEFAULT_LIGHT_BACKGROUND: &str = "#ffffff";
const DEFAULT_LIGHT_TEXT: &str = "#24292f";
const DEFAULT_LIGHT_SUBTLE_TEXT: &str = "#57606a";
const DEFAULT_DARK_TEXT: &str = "#c9d1d9";
const DEFAULT_DARK_SUBTLE_TEXT: &str = "#8b949e";

/// Normalize a color profile name for internal lookup.
///
/// The legacy `catppuccin` name is preserved as a compatibility alias and maps
/// to the original light/dark pairing: Latte on light themes, Mocha on dark themes.
pub fn canonical_color_profile_name(profile_name: &str, dark_mode: bool) -> String {
    let normalized = profile_name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "catppuccin" => {
            if dark_mode {
                "catppuccin-mocha".to_string()
            } else {
                "catppuccin-latte".to_string()
            }
        }
        _ => normalized,
    }
}

/// Validate a color profile name from CLI input.
pub fn validate_color_profile_name(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized == "catppuccin" || COLOR_PROFILE_NAMES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(format!("Use one of: {}", COLOR_PROFILE_NAMES_HELP))
    }
}

/// All color profiles.
pub fn get_color_profiles() -> HashMap<&'static str, ColorProfile> {
    let mut map = HashMap::new();

    // GitHub — classic green, high contrast
    map.insert(
        "github",
        ColorProfile {
            light: ThemeProfile {
                background: DEFAULT_LIGHT_BACKGROUND,
                text: DEFAULT_LIGHT_TEXT,
                subtle_text: DEFAULT_LIGHT_SUBTLE_TEXT,
                empty_cell: "#ebedf0",
                cells: ["#9be9a8", "#40c463", "#30a14e", "#216e39", "#0f5323"],
            },
            dark: ThemeProfile {
                background: "#0d1117",
                text: DEFAULT_DARK_TEXT,
                subtle_text: DEFAULT_DARK_SUBTLE_TEXT,
                empty_cell: "#161b22",
                cells: ["#0e4429", "#006d32", "#26a641", "#39d353", "#52d989"],
            },
        },
    );

    // Aurora — pale yellow → warm yellow → light green-yellow → deep green
    map.insert(
        "aurora",
        ColorProfile {
            light: ThemeProfile {
                background: DEFAULT_LIGHT_BACKGROUND,
                text: DEFAULT_LIGHT_TEXT,
                subtle_text: DEFAULT_LIGHT_SUBTLE_TEXT,
                empty_cell: "#fffbe6",
                cells: ["#ffe97a", "#f5d432", "#a8c826", "#5da818", "#2d7a0e"],
            },
            dark: ThemeProfile {
                background: "#1c1f28",
                text: DEFAULT_DARK_TEXT,
                subtle_text: DEFAULT_DARK_SUBTLE_TEXT,
                empty_cell: "#242a26",
                cells: ["#3d4a1e", "#9a8c1a", "#6db828", "#3d9015", "#1a600a"],
            },
        },
    );

    // Ocean — subtle teal to deep blue
    map.insert(
        "ocean",
        ColorProfile {
            light: ThemeProfile {
                background: DEFAULT_LIGHT_BACKGROUND,
                text: DEFAULT_LIGHT_TEXT,
                subtle_text: DEFAULT_LIGHT_SUBTLE_TEXT,
                empty_cell: "#f1fafe",
                cells: ["#a8dadc", "#66b2c9", "#3d8ba5", "#1d5e7a", "#0c3447"],
            },
            dark: ThemeProfile {
                background: "#121820",
                text: DEFAULT_DARK_TEXT,
                subtle_text: DEFAULT_DARK_SUBTLE_TEXT,
                empty_cell: "#14202b",
                cells: ["#1b3a4b", "#2c6f8a", "#4899b8", "#5bb8d4", "#7dd4e8"],
            },
        },
    );

    // Fire — warm red to deep crimson
    map.insert(
        "fire",
        ColorProfile {
            light: ThemeProfile {
                background: DEFAULT_LIGHT_BACKGROUND,
                text: DEFAULT_LIGHT_TEXT,
                subtle_text: DEFAULT_LIGHT_SUBTLE_TEXT,
                empty_cell: "#fde8e4",
                cells: ["#f59e8a", "#e05535", "#c43520", "#9a1a0e", "#6b0c04"],
            },
            dark: ThemeProfile {
                background: "#1c1412",
                text: DEFAULT_DARK_TEXT,
                subtle_text: DEFAULT_DARK_SUBTLE_TEXT,
                empty_cell: "#301a14",
                cells: ["#6b2e1a", "#c44a2a", "#e86535", "#ff8c42", "#ffc07a"],
            },
        },
    );

    // Catppuccin Latte — official light flavor.
    map.insert(
        "catppuccin-latte",
        ColorProfile {
            light: ThemeProfile {
                background: "#eff1f5",
                text: "#4c4f69",
                subtle_text: "#6c6f85",
                empty_cell: "#ccd0da",
                cells: ["#40a02b", "#df8e1d", "#fe640b", "#ea76cb", "#8839ef"],
            },
            dark: ThemeProfile {
                background: "#303446",
                text: "#c6d0f5",
                subtle_text: "#a5adce",
                empty_cell: "#414559",
                cells: ["#40a02b", "#df8e1d", "#fe640b", "#ea76cb", "#8839ef"],
            },
        },
    );

    // Catppuccin Frappé — official dark flavor.
    map.insert(
        "catppuccin-frappe",
        ColorProfile {
            light: ThemeProfile {
                background: "#eff1f5",
                text: "#4c4f69",
                subtle_text: "#6c6f85",
                empty_cell: "#ccd0da",
                cells: ["#a6d189", "#e5c890", "#ef9f76", "#f4b8e4", "#ca9ee6"],
            },
            dark: ThemeProfile {
                background: "#303446",
                text: "#c6d0f5",
                subtle_text: "#a5adce",
                empty_cell: "#414559",
                cells: ["#a6d189", "#e5c890", "#ef9f76", "#f4b8e4", "#ca9ee6"],
            },
        },
    );

    // Catppuccin Macchiato — official dark flavor.
    map.insert(
        "catppuccin-macchiato",
        ColorProfile {
            light: ThemeProfile {
                background: "#eff1f5",
                text: "#4c4f69",
                subtle_text: "#6c6f85",
                empty_cell: "#ccd0da",
                cells: ["#a6da95", "#eed49f", "#f5a97f", "#f5bde6", "#c6a0f6"],
            },
            dark: ThemeProfile {
                background: "#24273a",
                text: "#cad3f5",
                subtle_text: "#a5adcb",
                empty_cell: "#363a4f",
                cells: ["#a6da95", "#eed49f", "#f5a97f", "#f5bde6", "#c6a0f6"],
            },
        },
    );

    // Catppuccin Mocha — official dark flavor.
    map.insert(
        "catppuccin-mocha",
        ColorProfile {
            light: ThemeProfile {
                background: "#eff1f5",
                text: "#4c4f69",
                subtle_text: "#6c6f85",
                empty_cell: "#ccd0da",
                cells: ["#a6e3a1", "#f9e2af", "#fab387", "#f5c2e7", "#cba6f7"],
            },
            dark: ThemeProfile {
                background: "#1e1e2e",
                text: "#cdd6f4",
                subtle_text: "#a6adc8",
                empty_cell: "#313244",
                cells: ["#a6e3a1", "#f9e2af", "#fab387", "#f5c2e7", "#cba6f7"],
            },
        },
    );

    map
}

// ---------------------------------------------------------------------------
// Palette data structures
// ---------------------------------------------------------------------------

/// Render palette with background, text, and cell colors.
#[derive(Debug, Clone)]
pub struct Palette {
    pub background: String,
    pub text: String,
    pub subtle_text: String,
    pub empty_cell: String,
    pub cells: Vec<String>,
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Convert a #RRGGBB color to an RGB tuple.
pub fn hex_to_rgb(color: &str) -> (u8, u8, u8) {
    let color = color.trim_start_matches('#');
    if color.len() < 6 {
        return (0, 0, 0);
    }
    (
        u8::from_str_radix(&color[0..2], 16).unwrap_or(0),
        u8::from_str_radix(&color[2..4], 16).unwrap_or(0),
        u8::from_str_radix(&color[4..6], 16).unwrap_or(0),
    )
}

/// Convert an RGB tuple to #RRGGBB.
#[allow(dead_code)]
pub fn rgb_to_hex(rgb: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}

/// Approximate CIELAB L* luminance from an RGB tuple.
#[allow(dead_code)]
fn rgb_to_lab_l(rgb: (u8, u8, u8)) -> f64 {
    let (r, g, b) = (
        rgb.0 as f64 / 255.0,
        rgb.1 as f64 / 255.0,
        rgb.2 as f64 / 255.0,
    );
    let r = if r > 0.04045 { r.powf(2.2) } else { r / 12.92 };
    let g = if g > 0.04045 { g.powf(2.2) } else { g / 12.92 };
    let b = if b > 0.04045 { b.powf(2.2) } else { b / 12.92 };
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Approximate RGB from a perceptual luminance value.
#[allow(dead_code)]
fn lab_l_to_rgb(lum: f64) -> u8 {
    let lum = lum.max(0.0).min(1.0);
    let c = if lum > 0.04045 {
        lum.powf(1.0 / 2.2)
    } else {
        lum * 12.92
    };
    (c * 255.0).max(0.0).min(255.0) as u8
}

/// Interpolate between two hex colors with perceptual luminance awareness.
#[allow(dead_code)]
pub fn interpolate_color_perceptual(start_color: &str, end_color: &str, fraction: f64) -> String {
    let start_rgb = hex_to_rgb(start_color);
    let end_rgb = hex_to_rgb(end_color);

    let start_l = rgb_to_lab_l(start_rgb);
    let end_l = rgb_to_lab_l(end_rgb);

    let blended_l = start_l + (end_l - start_l) * fraction;

    let start_max = start_rgb.0.max(start_rgb.1).max(start_rgb.2) as f64;
    let end_max = end_rgb.0.max(end_rgb.1).max(end_rgb.2) as f64;
    let ratio = if end_max > 0.0 {
        start_max / end_max
    } else {
        1.0
    };

    let mut result = Vec::new();
    for (s_ch, e_ch) in [start_rgb.0, start_rgb.1, start_rgb.2]
        .iter()
        .zip([end_rgb.0, end_rgb.1, end_rgb.2].iter())
    {
        let perceptual = *s_ch as f64 + (*e_ch as f64 - *s_ch as f64) * fraction;
        let adjusted = perceptual * (blended_l / start_l.max(0.001)) * ratio;
        result.push(adjusted.max(0.0).min(255.0) as u8);
    }
    rgb_to_hex((result[0], result[1], result[2]))
}

/// Interpolate between two hex colors (simple per-channel).
#[allow(dead_code)]
pub fn interpolate_color(start_color: &str, end_color: &str, fraction: f64) -> String {
    let start_rgb = hex_to_rgb(start_color);
    let end_rgb = hex_to_rgb(end_color);
    let blended = (
        (start_rgb.0 as f64 + (end_rgb.0 as f64 - start_rgb.0 as f64) * fraction).round() as u8,
        (start_rgb.1 as f64 + (end_rgb.1 as f64 - start_rgb.1 as f64) * fraction).round() as u8,
        (start_rgb.2 as f64 + (end_rgb.2 as f64 - start_rgb.2 as f64) * fraction).round() as u8,
    );
    rgb_to_hex(blended)
}

/// Generate a color scale of the requested size from multi-stop anchors.
/// Uses perceptual interpolation for smoother, more uniform transitions.
/// The first stop is the least intense (low activity), the last stop is the most intense.
#[allow(dead_code)]
pub fn generate_color_scale(stops: &[&str], count: usize) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![stops[0].to_string()];
    }

    let segment_count = stops.len() - 1;
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let position = i as f64 / (count - 1) as f64;
        let segment_position = position * segment_count as f64;
        let left_index = (segment_position as usize).min(segment_count - 1);
        let right_index = (left_index + 1).min(stops.len() - 1);
        let fraction = segment_position - left_index as f64;
        result.push(interpolate_color_perceptual(
            stops[left_index],
            stops[right_index],
            fraction,
        ));
    }
    result
}

/// Resolve a color profile name to its theme-specific palette.
pub fn resolve_color_profile(profile_name: &str, dark_mode: bool) -> Result<ThemeProfile, String> {
    let canonical_name = canonical_color_profile_name(profile_name, dark_mode);
    let profiles = get_color_profiles();
    let profile = profiles
        .get(canonical_name.as_str())
        .copied()
        .ok_or_else(|| {
            format!(
                "Unknown color profile: {:?}. Valid profiles: {}",
                profile_name, COLOR_PROFILE_NAMES_HELP
            )
        })?;

    Ok(if dark_mode {
        profile.dark
    } else {
        profile.light
    })
}

/// Load the render palette.
pub fn load_palette(
    dark_mode: bool,
    cell_count: usize,
    color_profile: &str,
) -> Result<Palette, String> {
    let profile = resolve_color_profile(color_profile, dark_mode)?;
    let mut cells: Vec<String> = profile.cells.iter().map(|s| s.to_string()).collect();

    // Pad cells if needed
    while cells.len() < cell_count {
        cells.push(cells.last().cloned().unwrap_or_default());
    }
    cells.truncate(cell_count);

    Ok(Palette {
        background: profile.background.to_string(),
        text: profile.text.to_string(),
        subtle_text: profile.subtle_text.to_string(),
        empty_cell: profile.empty_cell.to_string(),
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_rgb() {
        assert_eq!(hex_to_rgb("#000000"), (0, 0, 0));
        assert_eq!(hex_to_rgb("#ffffff"), (255, 255, 255));
        assert_eq!(hex_to_rgb("#ff0000"), (255, 0, 0));
        assert_eq!(hex_to_rgb("#00ff00"), (0, 255, 0));
        assert_eq!(hex_to_rgb("#0000ff"), (0, 0, 255));
        assert_eq!(hex_to_rgb("#123456"), (0x12, 0x34, 0x56));
    }

    #[test]
    fn test_hex_to_rgb_invalid() {
        assert_eq!(hex_to_rgb("#000"), (0, 0, 0));
        assert_eq!(hex_to_rgb("invalid"), (0, 0, 0));
        assert_eq!(hex_to_rgb(""), (0, 0, 0));
    }

    #[test]
    fn test_canonical_color_profile_name() {
        assert_eq!(
            canonical_color_profile_name("catppuccin", false),
            "catppuccin-latte"
        );
        assert_eq!(
            canonical_color_profile_name("catppuccin", true),
            "catppuccin-mocha"
        );
        assert_eq!(
            canonical_color_profile_name("CaTpPuCcIn-MoChA", true),
            "catppuccin-mocha"
        );
    }

    #[test]
    fn test_validate_color_profile_name() {
        assert_eq!(validate_color_profile_name("github").unwrap(), "github");
        assert_eq!(
            validate_color_profile_name("catppuccin").unwrap(),
            "catppuccin"
        );
        assert!(validate_color_profile_name("invalid").is_err());
    }

    #[test]
    fn test_resolve_color_profile() {
        let profile = resolve_color_profile("github", false).unwrap();
        assert_eq!(profile.background, "#ffffff");
        assert_eq!(profile.empty_cell, "#ebedf0");
        assert_eq!(profile.cells.len(), 5);

        let profile = resolve_color_profile("github", true).unwrap();
        assert_eq!(profile.background, "#0d1117");
        assert_eq!(profile.empty_cell, "#161b22");
        assert_eq!(profile.cells.len(), 5);
    }

    #[test]
    fn test_resolve_color_profile_uses_theme_specific_backgrounds() {
        assert_eq!(
            resolve_color_profile("aurora", true).unwrap().background,
            "#1c1f28"
        );
        assert_eq!(
            resolve_color_profile("ocean", true).unwrap().background,
            "#121820"
        );
        assert_eq!(
            resolve_color_profile("fire", true).unwrap().background,
            "#1c1412"
        );
        assert_eq!(
            resolve_color_profile("catppuccin-mocha", true)
                .unwrap()
                .background,
            "#1e1e2e"
        );
    }

    #[test]
    fn test_resolve_color_profile_invalid() {
        assert!(resolve_color_profile("invalid", false).is_err());
    }

    #[test]
    fn test_load_palette() {
        let palette = load_palette(false, 5, "github").unwrap();
        assert_eq!(palette.background, "#ffffff");
        assert_eq!(palette.cells.len(), 5);

        let palette = load_palette(true, 5, "github").unwrap();
        assert_eq!(palette.background, "#0d1117");
        assert_eq!(palette.cells.len(), 5);
    }

    #[test]
    fn test_load_palette_uses_profile_specific_surfaces() {
        let palette = load_palette(false, 5, "catppuccin-latte").unwrap();
        assert_eq!(palette.background, "#eff1f5");
        assert_eq!(palette.text, "#4c4f69");
        assert_eq!(palette.empty_cell, "#ccd0da");

        let palette = load_palette(true, 5, "catppuccin-frappe").unwrap();
        assert_eq!(palette.background, "#303446");
        assert_eq!(palette.text, "#c6d0f5");
        assert_eq!(palette.empty_cell, "#414559");
    }

    #[test]
    fn test_load_palette_cell_count() {
        let palette = load_palette(false, 10, "github").unwrap();
        assert_eq!(palette.cells.len(), 10);
    }

    #[test]
    fn test_get_color_profiles() {
        let profiles = get_color_profiles();
        assert!(profiles.contains_key("github"));
        assert!(profiles.contains_key("aurora"));
        assert!(profiles.contains_key("ocean"));
        assert!(profiles.contains_key("fire"));
        assert!(profiles.contains_key("catppuccin-latte"));
        assert!(profiles.contains_key("catppuccin-frappe"));
        assert!(profiles.contains_key("catppuccin-macchiato"));
        assert!(profiles.contains_key("catppuccin-mocha"));
        assert!(!profiles.contains_key("catppuccin"));
    }
}
