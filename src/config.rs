//! XDG-based configuration loading and the `hitmap config` command.

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// Print the resolved config file path
    Path,
    /// Create a commented config template if no config exists yet
    Init,
    /// Print config contents
    Show {
        /// Print merged values including built-in defaults
        #[arg(long)]
        effective: bool,
    },
    /// Open the config file in $VISUAL, $EDITOR, or vi
    Edit,
    /// Set a config key to a validated value
    Set {
        /// Dot-separated config key, for example render.theme
        key: String,
        /// Value to store for the selected key
        value: String,
    },
    /// Remove a config key from the config file
    Unset {
        /// Dot-separated config key, for example render.theme
        key: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct HitmapConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<AuthorsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctor: Option<DoctorConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RenderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_multiplier: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width_cells: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AuthorsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DoctorConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
}

impl HitmapConfig {
    pub fn is_empty(&self) -> bool {
        self.render.is_none() && self.authors.is_none() && self.doctor.is_none()
    }

    pub fn validate_semantics(&self) -> Result<(), String> {
        if let Some(render) = &self.render {
            if let Some(value) = &render.renderer {
                crate::render::parse_renderer_name(value)
                    .map_err(|error| format!("render.renderer: {}", error))?;
            }
            if let Some(value) = &render.theme {
                crate::render::validate_theme_name(value)
                    .map_err(|error| format!("render.theme: {}", error))?;
            }
            if let Some(value) = &render.color_profile {
                crate::palette::validate_color_profile_name(value)
                    .map_err(|error| format!("render.color_profile: {}", error))?;
            }
            if let Some(value) = &render.scale_profile {
                crate::render::validate_scale_profile_name(value)
                    .map_err(|error| format!("render.scale_profile: {}", error))?;
            }
            if let Some(value) = render.scale_multiplier
                && value == 0
            {
                return Err(
                    "render.scale_multiplier must be greater than or equal to 1".to_string()
                );
            }
            if let Some(value) = render.render_scale {
                crate::render::validate_render_scale_number(value)
                    .map_err(|error| format!("render.render_scale: {}", error))?;
            }
            if let Some(value) = render.max_width_cells
                && value == 0
            {
                return Err("render.max_width_cells must be greater than or equal to 1".to_string());
            }
        }

        if let Some(authors) = &self.authors
            && let Some(value) = &authors.output_format
        {
            crate::authors::parse_output_format_name(value)
                .map_err(|error| format!("authors.output_format: {}", error))?;
        }

        if let Some(doctor) = &self.doctor
            && let Some(value) = &doctor.output_format
        {
            crate::doctor::parse_doctor_format_name(value)
                .map_err(|error| format!("doctor.output_format: {}", error))?;
        }

        Ok(())
    }
}

impl RenderConfig {
    fn is_empty(&self) -> bool {
        self.renderer.is_none()
            && self.theme.is_none()
            && self.color_profile.is_none()
            && self.scale_profile.is_none()
            && self.scale_multiplier.is_none()
            && self.render_scale.is_none()
            && self.max_width_cells.is_none()
    }
}

impl AuthorsConfig {
    fn is_empty(&self) -> bool {
        self.output_format.is_none()
    }
}

impl DoctorConfig {
    fn is_empty(&self) -> bool {
        self.output_format.is_none()
    }
}

enum ConfigKey {
    RenderRenderer,
    RenderTheme,
    RenderColorProfile,
    RenderScaleProfile,
    RenderScaleMultiplier,
    RenderRenderScale,
    RenderMaxWidthCells,
    AuthorsOutputFormat,
    DoctorOutputFormat,
}

impl ConfigKey {
    fn parse(key: &str) -> Result<Self, String> {
        match key.trim() {
            "render.renderer" => Ok(Self::RenderRenderer),
            "render.theme" => Ok(Self::RenderTheme),
            "render.color_profile" => Ok(Self::RenderColorProfile),
            "render.scale_profile" => Ok(Self::RenderScaleProfile),
            "render.scale_multiplier" => Ok(Self::RenderScaleMultiplier),
            "render.render_scale" => Ok(Self::RenderRenderScale),
            "render.max_width_cells" => Ok(Self::RenderMaxWidthCells),
            "authors.output_format" => Ok(Self::AuthorsOutputFormat),
            "doctor.output_format" => Ok(Self::DoctorOutputFormat),
            _ => Err(format!(
                "Unsupported config key: {}. Supported keys: render.renderer, render.theme, render.color_profile, render.scale_profile, render.scale_multiplier, render.render_scale, render.max_width_cells, authors.output_format, doctor.output_format",
                key
            )),
        }
    }

    fn set_value(&self, config: &mut HitmapConfig, raw_value: &str) -> Result<(), String> {
        match self {
            Self::RenderRenderer => {
                let renderer = crate::render::parse_renderer_name(raw_value)?;
                config.render.get_or_insert_with(Default::default).renderer =
                    Some(match renderer {
                        crate::render::Renderer::Kitty => "kitty".to_string(),
                        crate::render::Renderer::Text => "text".to_string(),
                    });
            }
            Self::RenderTheme => {
                config.render.get_or_insert_with(Default::default).theme =
                    Some(crate::render::validate_theme_name(raw_value)?);
            }
            Self::RenderColorProfile => {
                config
                    .render
                    .get_or_insert_with(Default::default)
                    .color_profile = Some(crate::palette::validate_color_profile_name(raw_value)?);
            }
            Self::RenderScaleProfile => {
                config
                    .render
                    .get_or_insert_with(Default::default)
                    .scale_profile = Some(crate::render::validate_scale_profile_name(raw_value)?);
            }
            Self::RenderScaleMultiplier => {
                let value = raw_value.trim().parse::<u32>().map_err(|_| {
                    "render.scale_multiplier must be an integer greater than or equal to 1"
                        .to_string()
                })?;
                if value == 0 {
                    return Err(
                        "render.scale_multiplier must be greater than or equal to 1".to_string()
                    );
                }
                config
                    .render
                    .get_or_insert_with(Default::default)
                    .scale_multiplier = Some(value);
            }
            Self::RenderRenderScale => {
                let value = raw_value.trim().parse::<f64>().map_err(|_| {
                    "render.render_scale must be a number greater than or equal to 1.0".to_string()
                })?;
                crate::render::validate_render_scale_number(value)?;
                config
                    .render
                    .get_or_insert_with(Default::default)
                    .render_scale = Some(value);
            }
            Self::RenderMaxWidthCells => {
                let value = raw_value.trim().parse::<u32>().map_err(|_| {
                    "render.max_width_cells must be an integer greater than or equal to 1"
                        .to_string()
                })?;
                if value == 0 {
                    return Err(
                        "render.max_width_cells must be greater than or equal to 1".to_string()
                    );
                }
                config
                    .render
                    .get_or_insert_with(Default::default)
                    .max_width_cells = Some(value);
            }
            Self::AuthorsOutputFormat => {
                let value = crate::authors::parse_output_format_name(raw_value)?;
                config
                    .authors
                    .get_or_insert_with(Default::default)
                    .output_format = Some(match value {
                    crate::authors::OutputFormat::Table => "table".to_string(),
                    crate::authors::OutputFormat::Json => "json".to_string(),
                    crate::authors::OutputFormat::Tsv => "tsv".to_string(),
                });
            }
            Self::DoctorOutputFormat => {
                let value = crate::doctor::parse_doctor_format_name(raw_value)?;
                config
                    .doctor
                    .get_or_insert_with(Default::default)
                    .output_format = Some(match value {
                    crate::doctor::DoctorFormat::Table => "table".to_string(),
                    crate::doctor::DoctorFormat::Json => "json".to_string(),
                });
            }
        }

        Ok(())
    }

    fn unset(&self, config: &mut HitmapConfig) {
        match self {
            Self::RenderRenderer => {
                if let Some(render) = &mut config.render {
                    render.renderer = None;
                }
            }
            Self::RenderTheme => {
                if let Some(render) = &mut config.render {
                    render.theme = None;
                }
            }
            Self::RenderColorProfile => {
                if let Some(render) = &mut config.render {
                    render.color_profile = None;
                }
            }
            Self::RenderScaleProfile => {
                if let Some(render) = &mut config.render {
                    render.scale_profile = None;
                }
            }
            Self::RenderScaleMultiplier => {
                if let Some(render) = &mut config.render {
                    render.scale_multiplier = None;
                }
            }
            Self::RenderRenderScale => {
                if let Some(render) = &mut config.render {
                    render.render_scale = None;
                }
            }
            Self::RenderMaxWidthCells => {
                if let Some(render) = &mut config.render {
                    render.max_width_cells = None;
                }
            }
            Self::AuthorsOutputFormat => {
                if let Some(authors) = &mut config.authors {
                    authors.output_format = None;
                }
            }
            Self::DoctorOutputFormat => {
                if let Some(doctor) = &mut config.doctor {
                    doctor.output_format = None;
                }
            }
        }

        if config.render.as_ref().is_some_and(RenderConfig::is_empty) {
            config.render = None;
        }
        if config.authors.as_ref().is_some_and(AuthorsConfig::is_empty) {
            config.authors = None;
        }
        if config.doctor.as_ref().is_some_and(DoctorConfig::is_empty) {
            config.doctor = None;
        }
    }
}

pub fn resolve_config_path() -> Result<PathBuf, String> {
    resolve_config_path_from_env(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn resolve_config_path_from_env(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf, String> {
    if let Some(xdg_root) = xdg_config_home.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(xdg_root).join("hitmap").join("hitmap.toml"));
    }

    if let Some(home) = home.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home)
            .join(".config")
            .join("hitmap")
            .join("hitmap.toml"));
    }

    Err("Unable to resolve hitmap config path: neither XDG_CONFIG_HOME nor HOME is set".to_string())
}

pub fn load_config() -> Result<HitmapConfig, String> {
    let path = resolve_config_path()?;
    load_config_from_path(&path)
}

pub fn load_config_from_path(path: &Path) -> Result<HitmapConfig, String> {
    let config = load_config_from_path_unvalidated(path)?;
    config
        .validate_semantics()
        .map_err(|error| format!("Invalid config {}: {}", path.display(), error))?;
    Ok(config)
}

fn load_config_from_path_unvalidated(path: &Path) -> Result<HitmapConfig, String> {
    if !path.exists() {
        return Ok(HitmapConfig::default());
    }

    let raw = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read config {}: {}", path.display(), error))?;
    if raw.trim().is_empty() {
        return Ok(HitmapConfig::default());
    }

    toml::from_str(&raw)
        .map_err(|error| format!("Failed to parse config {}: {}", path.display(), error))
}

fn ensure_config_parent_dir(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| {
        format!(
            "Unable to determine parent directory for config path {}",
            path.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create config directory {}: {}",
            parent.display(),
            error
        )
    })
}

fn write_config_to_path(path: &Path, config: &HitmapConfig) -> Result<(), String> {
    if config.is_empty() {
        if path.exists() {
            fs::remove_file(path).map_err(|error| {
                format!("Failed to remove config {}: {}", path.display(), error)
            })?;
        }
        return Ok(());
    }

    let serialized = toml::to_string_pretty(config)
        .map_err(|error| format!("Failed to serialize config: {}", error))?;
    ensure_config_parent_dir(path)?;

    let mut content = serialized;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    fs::write(path, content)
        .map_err(|error| format!("Failed to write config {}: {}", path.display(), error))
}

fn print_toml_value<T: Serialize>(value: &T) -> Result<(), String> {
    let mut content = toml::to_string_pretty(value)
        .map_err(|error| format!("Failed to serialize config: {}", error))?;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    print!("{}", content);
    Ok(())
}

fn effective_config(config: &HitmapConfig) -> Result<HitmapConfig, String> {
    let render = Some(RenderConfig {
        renderer: Some(
            config
                .render
                .as_ref()
                .and_then(|cfg| cfg.renderer.as_deref())
                .map(crate::render::parse_renderer_name)
                .transpose()?
                .unwrap_or(crate::render::DEFAULT_RENDERER)
                .to_string(),
        ),
        theme: Some(
            config
                .render
                .as_ref()
                .and_then(|cfg| cfg.theme.as_deref())
                .map(crate::render::validate_theme_name)
                .transpose()?
                .unwrap_or_else(|| crate::render::DEFAULT_THEME.to_string()),
        ),
        color_profile: Some(
            config
                .render
                .as_ref()
                .and_then(|cfg| cfg.color_profile.as_deref())
                .map(crate::palette::validate_color_profile_name)
                .transpose()?
                .unwrap_or_else(|| crate::render::DEFAULT_COLOR_PROFILE.to_string()),
        ),
        scale_profile: Some(
            config
                .render
                .as_ref()
                .and_then(|cfg| cfg.scale_profile.as_deref())
                .map(crate::render::validate_scale_profile_name)
                .transpose()?
                .unwrap_or_else(|| crate::render::DEFAULT_SCALE_PROFILE.to_string()),
        ),
        scale_multiplier: Some(
            config
                .render
                .as_ref()
                .and_then(|cfg| cfg.scale_multiplier)
                .unwrap_or(crate::render::DEFAULT_SCALE_MULTIPLIER),
        ),
        render_scale: Some(
            config
                .render
                .as_ref()
                .and_then(|cfg| cfg.render_scale)
                .unwrap_or(crate::render::DEFAULT_RENDER_SCALE),
        ),
        max_width_cells: config.render.as_ref().and_then(|cfg| cfg.max_width_cells),
    });

    let authors = Some(AuthorsConfig {
        output_format: Some(
            config
                .authors
                .as_ref()
                .and_then(|cfg| cfg.output_format.as_deref())
                .map(crate::authors::parse_output_format_name)
                .transpose()?
                .unwrap_or(crate::authors::DEFAULT_OUTPUT_FORMAT)
                .to_string(),
        ),
    });

    let doctor = Some(DoctorConfig {
        output_format: Some(
            config
                .doctor
                .as_ref()
                .and_then(|cfg| cfg.output_format.as_deref())
                .map(crate::doctor::parse_doctor_format_name)
                .transpose()?
                .unwrap_or(crate::doctor::DEFAULT_DOCTOR_FORMAT)
                .to_string(),
        ),
    });

    Ok(HitmapConfig {
        render,
        authors,
        doctor,
    })
}

fn resolve_editor_command() -> String {
    std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "vi".to_string())
}

fn edit_file_with_editor_command(path: &Path, editor_command: &str) -> Result<(), String> {
    let status = Command::new("sh")
        .arg("-c")
        .arg("eval \"$HITMAP_EDITOR_COMMAND \\\"${HITMAP_EDIT_PATH}\\\"\"")
        .env("HITMAP_EDITOR_COMMAND", editor_command)
        .env("HITMAP_EDIT_PATH", path)
        .status()
        .map_err(|error| {
            format!(
                "Failed to launch editor {:?} for {}: {}",
                editor_command,
                path.display(),
                error
            )
        })?;

    if !status.success() {
        return Err(format!(
            "Editor {:?} exited with status {}",
            editor_command, status
        ));
    }

    Ok(())
}

fn config_template() -> String {
    r#"# hitmap configuration
#
# Location precedence:
#   $XDG_CONFIG_HOME/hitmap/hitmap.toml
#   ~/.config/hitmap/hitmap.toml
#
# Runtime precedence:
#   CLI flags > config file > built-in defaults
#
# Uncomment only the options you want to override.

[render]
# renderer = "kitty"             # options: kitty, text
# theme = "auto"                 # options: auto, light, dark
# color_profile = "github"       # options: github, aurora, ocean, fire, catppuccin-latte, catppuccin-frappe, catppuccin-macchiato, catppuccin-mocha
# scale_profile = "fibonacci-21-plus"  # examples: linear-5-plus, linear-10-plus, fibonacci-8-plus, fibonacci-21-plus
# scale_multiplier = 1            # integer >= 1
# render_scale = 2.0              # float >= 1.0
# max_width_cells = 120           # integer >= 1

[authors]
# output_format = "table"        # options: table, json, tsv

[doctor]
# output_format = "table"        # options: table, json
"#
    .to_string()
}

fn ensure_config_file_exists(path: &Path) -> Result<bool, String> {
    ensure_config_parent_dir(path)?;
    let should_write_template = if !path.exists() {
        true
    } else {
        fs::read_to_string(path)
            .map(|content| content.trim().is_empty())
            .map_err(|error| format!("Failed to read config {}: {}", path.display(), error))?
    };

    if should_write_template {
        fs::write(path, config_template())
            .map_err(|error| format!("Failed to create config {}: {}", path.display(), error))?;
        return Ok(true);
    }

    Ok(false)
}

pub fn config_command(args: ConfigArgs) -> Result<(), String> {
    let path = resolve_config_path()?;

    match args.command {
        ConfigCommand::Path => {
            println!("{}", path.display());
            Ok(())
        }
        ConfigCommand::Init => {
            if ensure_config_file_exists(&path)? {
                println!("Initialized {}", path.display());
            } else {
                println!("Config already exists at {}", path.display());
            }
            Ok(())
        }
        ConfigCommand::Show { effective } => {
            if effective {
                let config = load_config()?;
                return print_toml_value(&effective_config(&config)?);
            }

            if !path.exists() {
                println!("# No config file found at {}", path.display());
                return Ok(());
            }

            let raw = fs::read_to_string(&path)
                .map_err(|error| format!("Failed to read config {}: {}", path.display(), error))?;
            if raw.ends_with('\n') {
                print!("{}", raw);
            } else {
                println!("{}", raw);
            }
            Ok(())
        }
        ConfigCommand::Edit => {
            ensure_config_file_exists(&path)?;
            let editor_command = resolve_editor_command();
            edit_file_with_editor_command(&path, &editor_command)?;
            load_config_from_path(&path)?;
            println!("Edited {}", path.display());
            Ok(())
        }
        ConfigCommand::Set { key, value } => {
            let mut config = load_config_from_path_unvalidated(&path)?;
            let parsed_key = ConfigKey::parse(&key)?;
            parsed_key.set_value(&mut config, &value)?;
            write_config_to_path(&path, &config)?;
            println!("Updated {} in {}", key, path.display());
            Ok(())
        }
        ConfigCommand::Unset { key } => {
            let mut config = load_config_from_path_unvalidated(&path)?;
            let parsed_key = ConfigKey::parse(&key)?;
            parsed_key.unset(&mut config);
            write_config_to_path(&path, &config)?;
            println!("Removed {} from {}", key, path.display());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("hitmap-tests-{}-{}", std::process::id(), unique))
            .join(name)
    }

    #[test]
    fn resolves_xdg_path_when_available() {
        let path = resolve_config_path_from_env(Some("/tmp/xdg".into()), Some("/tmp/home".into()))
            .unwrap();
        assert_eq!(path, PathBuf::from("/tmp/xdg/hitmap/hitmap.toml"));
    }

    #[test]
    fn falls_back_to_home_config_path() {
        let path = resolve_config_path_from_env(None, Some("/tmp/home".into())).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/home/.config/hitmap/hitmap.toml"));
    }

    #[test]
    fn set_and_unset_round_trip_through_toml_file() {
        let path = temp_path("config.toml");
        let mut config = HitmapConfig::default();

        ConfigKey::parse("render.theme")
            .unwrap()
            .set_value(&mut config, "dark")
            .unwrap();
        ConfigKey::parse("authors.output_format")
            .unwrap()
            .set_value(&mut config, "json")
            .unwrap();
        write_config_to_path(&path, &config).unwrap();

        let loaded = load_config_from_path(&path).unwrap();
        assert_eq!(loaded.render.unwrap().theme.as_deref(), Some("dark"));
        assert_eq!(
            loaded.authors.unwrap().output_format.as_deref(),
            Some("json")
        );

        let mut loaded = load_config_from_path_unvalidated(&path).unwrap();
        ConfigKey::parse("render.theme").unwrap().unset(&mut loaded);
        ConfigKey::parse("authors.output_format")
            .unwrap()
            .unset(&mut loaded);
        write_config_to_path(&path, &loaded).unwrap();
        assert!(!path.exists(), "empty config file should be removed");
    }

    #[test]
    fn load_config_rejects_invalid_semantic_values() {
        let path = temp_path("invalid.toml");
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::write(&path, "[render]\ntheme = \"blue\"\n").unwrap();

        let error = load_config_from_path(&path).unwrap_err();
        assert!(error.contains("Invalid config"));
        assert!(error.contains("theme"));
    }

    #[test]
    fn effective_config_merges_defaults_and_saved_values() {
        let config = HitmapConfig {
            render: Some(RenderConfig {
                theme: Some("dark".to_string()),
                color_profile: Some("ocean".to_string()),
                ..Default::default()
            }),
            authors: Some(AuthorsConfig {
                output_format: Some("json".to_string()),
            }),
            doctor: None,
        };

        let effective = effective_config(&config).unwrap();
        let render = effective.render.unwrap();
        assert_eq!(render.renderer.as_deref(), Some("kitty"));
        assert_eq!(render.theme.as_deref(), Some("dark"));
        assert_eq!(render.color_profile.as_deref(), Some("ocean"));
        assert_eq!(render.scale_profile.as_deref(), Some("fibonacci-21-plus"));
        assert_eq!(render.scale_multiplier, Some(1));
        assert_eq!(render.render_scale, Some(2.0));
        assert_eq!(render.max_width_cells, None);
        assert_eq!(
            effective.authors.unwrap().output_format.as_deref(),
            Some("json")
        );
        assert_eq!(
            effective.doctor.unwrap().output_format.as_deref(),
            Some("table")
        );
    }

    #[test]
    fn ensure_config_file_exists_writes_commented_template() {
        let path = temp_path("template.toml");
        let created = ensure_config_file_exists(&path).unwrap();
        assert!(created);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# hitmap configuration"));
        assert!(content.contains("# renderer = \"kitty\""));
        assert!(content.contains("# output_format = \"table\""));
        assert!(content.contains("color_profile = \"github\""));
    }

    #[test]
    fn ensure_config_file_exists_does_not_overwrite_existing_config() {
        let path = temp_path("existing.toml");
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::write(&path, "[render]\ntheme = \"dark\"\n").unwrap();

        let created = ensure_config_file_exists(&path).unwrap();
        assert!(!created);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[render]\ntheme = \"dark\"\n"
        );
    }

    #[test]
    fn edit_file_with_shell_editor_command_updates_file() {
        let path = temp_path("edit.toml");
        ensure_config_file_exists(&path).unwrap();
        edit_file_with_editor_command(&path, "printf '[render]\ntheme = \"dark\"\n' >").unwrap();
        let loaded = load_config_from_path(&path).unwrap();
        assert_eq!(loaded.render.unwrap().theme.as_deref(), Some("dark"));
    }
}
