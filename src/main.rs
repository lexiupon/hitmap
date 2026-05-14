mod authors;
mod common;
mod config;
mod doctor;
mod git;
mod palette;
mod render;
mod scale;
mod sections;
mod terminal;
mod text;

use clap::{Parser, error::ErrorKind};
use std::ffi::OsString;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ROOT_PASSTHROUGH_FLAGS: &[&str] = &["-h", "--help", "-V", "--version", "--"];

/// Expand a leading `~` or `~/` to the user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{}", home.to_string_lossy(), rest);
        }
    }
    if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return home.to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// Parse a date string (YYYY-MM-DD) into a DateTime<Utc>.
pub fn parse_date(s: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|nd| nd.and_hms_opt(0, 0, 0).unwrap().and_utc())
        .map_err(|e| format!("Invalid date '{}': {}", s, e))
}

#[derive(Parser)]
#[command(
    name = "hitmap",
    version = VERSION,
    about = "Render git commit hitmaps and inspect author identities",
    long_about = "Render git commit hitmaps and inspect author identities.\n\nWhen no subcommand is given, hitmap defaults to `render`.",
    after_help = "Tip: `hitmap [REPO_PATH]` is equivalent to `hitmap render [REPO_PATH]`.\nUse `hitmap render --help` to see render-specific options like color profiles."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Render a git commit hitmap in the terminal
    Render(render::RenderArgs),
    /// List author identities reachable from the current HEAD history
    Authors(authors::AuthorsArgs),
    /// Check terminal support for Kitty image rendering
    Doctor(doctor::DoctorArgs),
    /// Inspect and update persisted defaults
    Config(config::ConfigArgs),
}

#[derive(Debug, Clone, Copy)]
enum HelpTopic {
    Root,
    Render,
    Authors,
    Doctor,
    Config,
}

struct ParsePlan {
    normalized_args: Vec<OsString>,
    help_topic_on_error: HelpTopic,
}

fn build_parse_plan(args: Vec<OsString>) -> ParsePlan {
    if args.len() <= 1 {
        return ParsePlan {
            normalized_args: vec!["hitmap".into(), "render".into()],
            help_topic_on_error: HelpTopic::Render,
        };
    }

    let first = &args[1];
    let first_str = first.to_string_lossy();
    let is_root_passthrough_flag = ROOT_PASSTHROUGH_FLAGS
        .iter()
        .any(|flag| flag == &first_str.as_ref());

    if is_root_passthrough_flag {
        return ParsePlan {
            normalized_args: args,
            help_topic_on_error: HelpTopic::Root,
        };
    }

    let help_topic_on_error = match first_str.as_ref() {
        "render" => HelpTopic::Render,
        "authors" => HelpTopic::Authors,
        "doctor" => HelpTopic::Doctor,
        "config" => HelpTopic::Config,
        "help" | "hitmap" => HelpTopic::Root,
        _ => HelpTopic::Render,
    };

    let normalized_args = match first_str.as_ref() {
        "render" | "authors" | "doctor" | "config" | "help" | "hitmap" => args,
        _ if first_str.starts_with("--") => {
            let mut new_args: Vec<OsString> = vec!["hitmap".into(), "render".into()];
            new_args.extend(args.into_iter().skip(1));
            new_args
        }
        _ => {
            let mut new_args: Vec<OsString> = vec!["hitmap".into(), "render".into()];
            new_args.extend(args.into_iter().skip(1));
            new_args
        }
    };

    ParsePlan {
        normalized_args,
        help_topic_on_error,
    }
}

fn help_hint_for(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::Root => "Run `hitmap --help` for full usage.",
        HelpTopic::Render => "Run `hitmap render --help` for full render usage.",
        HelpTopic::Authors => "Run `hitmap authors --help` for full authors usage.",
        HelpTopic::Doctor => "Run `hitmap doctor --help` for full doctor usage.",
        HelpTopic::Config => "Run `hitmap config --help` for full config usage.",
    }
}

fn strip_clap_help_hint(rendered: &str) -> String {
    const CLAP_GENERIC_HINTS: &[&str] = &[
        "For more information, try '--help'.",
        "For more information try '--help'.",
    ];

    let mut lines: Vec<&str> = rendered.lines().collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if lines
        .last()
        .is_some_and(|line| CLAP_GENERIC_HINTS.contains(&line.trim()))
    {
        lines.pop();
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
    }
    lines.join("\n")
}

fn handle_parse_error(err: clap::Error, topic: HelpTopic) -> ! {
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        err.exit();
    }

    let exit_code = err.exit_code();
    eprintln!("{}", strip_clap_help_hint(&err.to_string()));
    eprintln!("Hint: {}", help_hint_for(topic));
    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os_args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn rewrites_root_render_flags_to_render_subcommand() {
        let plan = build_parse_plan(os_args(&["hitmap", "--color-profile", "fire"]));
        let normalized: Vec<String> = plan
            .normalized_args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            normalized,
            vec!["hitmap", "render", "--color-profile", "fire"]
        );
        assert!(matches!(plan.help_topic_on_error, HelpTopic::Render));
    }

    #[test]
    fn keeps_help_subcommand_intact() {
        let plan = build_parse_plan(os_args(&["hitmap", "help", "render"]));
        let normalized: Vec<String> = plan
            .normalized_args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();

        assert_eq!(normalized, vec!["hitmap", "help", "render"]);
        assert!(matches!(plan.help_topic_on_error, HelpTopic::Root));
    }

    #[test]
    fn keeps_config_subcommand_intact() {
        let plan = build_parse_plan(os_args(&["hitmap", "config", "show"]));
        let normalized: Vec<String> = plan
            .normalized_args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();

        assert_eq!(normalized, vec!["hitmap", "config", "show"]);
        assert!(matches!(plan.help_topic_on_error, HelpTopic::Config));
    }

    #[test]
    fn strips_generic_clap_help_hint() {
        let rendered = "error: bad flag\n\nFor more information, try '--help'.\n";
        assert_eq!(strip_clap_help_hint(rendered), "error: bad flag");
    }
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().collect();
    let parse_plan = build_parse_plan(args);
    let cli = match Cli::try_parse_from(parse_plan.normalized_args) {
        Ok(cli) => cli,
        Err(err) => handle_parse_error(err, parse_plan.help_topic_on_error),
    };

    let result = match cli.command {
        Some(Commands::Render(args)) => render::render_command(args),
        Some(Commands::Authors(args)) => authors::authors_command(args),
        Some(Commands::Doctor(args)) => doctor::doctor_command(args),
        Some(Commands::Config(args)) => config::config_command(args),
        None => Ok(()),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
