//! Authors command: grouping, sorting, searching, and output formatting.

use clap::Args;
use comfy_table::{
    Cell, Color, Table, TableComponent, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED,
};
use serde::Serialize;

use crate::common::AuthorSummary;
use crate::git::{collect_author_summaries, resolve_repo_root};

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct AuthorsArgs {
    #[arg(
        default_value = ".",
        value_name = "REPO_PATH",
        help = "Repository path to inspect"
    )]
    pub repo_path: String,

    #[arg(
        short,
        long,
        value_enum,
        default_value = "commits",
        help = "Sort field"
    )]
    pub sort: AuthorSortBy,

    #[arg(
        short = 'O',
        long,
        value_enum,
        help = "Sort order (default: desc for commits, asc otherwise)"
    )]
    pub order: Option<SortOrder>,

    #[arg(
        short,
        long,
        value_enum,
        default_value = "pair",
        help = "Identity grouping mode"
    )]
    pub group_by: GroupBy,

    #[arg(long, value_name = "TERM", help = "Case-insensitive substring filter")]
    pub search: Option<String>,

    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u32).range(1..),
        help = "Limit the number of displayed rows"
    )]
    pub limit: Option<u32>,

    #[arg(
        short,
        long,
        visible_alias = "format",
        value_enum,
        help = "Output format (default: config value or table)"
    )]
    pub output_format: Option<OutputFormat>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum AuthorSortBy {
    Commits,
    Name,
    Email,
}

impl std::fmt::Display for AuthorSortBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthorSortBy::Commits => write!(f, "commits"),
            AuthorSortBy::Name => write!(f, "name"),
            AuthorSortBy::Email => write!(f, "email"),
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum GroupBy {
    Pair,
    Name,
    Email,
}

impl std::fmt::Display for GroupBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupBy::Pair => write!(f, "pair"),
            GroupBy::Name => write!(f, "name"),
            GroupBy::Email => write!(f, "email"),
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum OutputFormat {
    Table,
    Json,
    Tsv,
}

pub const DEFAULT_OUTPUT_FORMAT: OutputFormat = OutputFormat::Table;

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Table => write!(f, "table"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Tsv => write!(f, "tsv"),
        }
    }
}

pub fn parse_output_format_name(value: &str) -> Result<OutputFormat, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "table" => Ok(OutputFormat::Table),
        "json" => Ok(OutputFormat::Json),
        "tsv" => Ok(OutputFormat::Tsv),
        _ => Err("Authors output format must be one of: table, json, tsv".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Resolve the effective sort order.
fn resolved_sort_order(sort_by: &str, order: Option<&SortOrder>) -> String {
    match order {
        Some(SortOrder::Asc) => "asc".to_string(),
        Some(SortOrder::Desc) => "desc".to_string(),
        None => match sort_by {
            "commits" | "Commits" => "desc".to_string(),
            _ => "asc".to_string(),
        },
    }
}

/// Summarize a list of values into a comma-separated string.
fn summarize_values(values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    values.join(", ")
}

/// Check if an author summary matches a search term.
fn summary_matches_search(summary: &AuthorSummary, search_term: Option<&str>) -> bool {
    match search_term {
        None => true,
        Some(term) => {
            let term_lower = term.to_lowercase();
            summary.primary_name.to_lowercase().contains(&term_lower)
                || summary.primary_email.to_lowercase().contains(&term_lower)
                || summary
                    .names
                    .iter()
                    .any(|n| n.to_lowercase().contains(&term_lower))
                || summary
                    .emails
                    .iter()
                    .any(|e| e.to_lowercase().contains(&term_lower))
        }
    }
}

/// Serialize author summaries as JSON.
fn author_summaries_to_json(
    summaries: &[AuthorSummary],
    sort_by: &str,
    order: Option<&SortOrder>,
    group_by: &str,
    repo_path: &str,
) -> String {
    let entries: Vec<AuthorEntry> = summaries
        .iter()
        .map(|s| AuthorEntry {
            commits: s.commits,
            primary_name: s.primary_name.clone(),
            primary_email: s.primary_email.clone(),
            names: s.names.clone(),
            emails: s.emails.clone(),
        })
        .collect();

    let output = AuthorsJsonOutput {
        repo: repo_path.to_string(),
        sort: sort_by.to_string(),
        order: resolved_sort_order(sort_by, order),
        group_by: group_by.to_string(),
        entries,
    };

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

#[derive(Serialize)]
struct AuthorEntry {
    commits: u64,
    primary_name: String,
    primary_email: String,
    names: Vec<String>,
    emails: Vec<String>,
}

#[derive(Serialize)]
struct AuthorsJsonOutput {
    repo: String,
    sort: String,
    order: String,
    group_by: String,
    entries: Vec<AuthorEntry>,
}

// ---------------------------------------------------------------------------
// Command implementation
// ---------------------------------------------------------------------------

/// Execute the authors command.
pub fn authors_command(args: AuthorsArgs) -> Result<(), String> {
    let config = crate::config::load_config()?;
    let output_format = if let Some(format) = args.output_format {
        format
    } else if let Some(value) = config
        .authors
        .as_ref()
        .and_then(|cfg| cfg.output_format.as_deref())
    {
        parse_output_format_name(value)?
    } else {
        DEFAULT_OUTPUT_FORMAT
    };

    // Resolve repository path
    let repo_path = resolve_repo_root(&args.repo_path)
        .map(|r| r.path)
        .map_err(|e| format!("Invalid repository: {}", e))?;

    // Collect author summaries
    let summaries = collect_author_summaries(&repo_path, &args.group_by.to_string())
        .map_err(|e| format!("Failed to collect author summaries: {}", e))?;

    // Filter by search term
    let filtered: Vec<AuthorSummary> = summaries
        .into_iter()
        .filter(|s| summary_matches_search(&s, args.search.as_deref()))
        .collect();

    // Sort
    let mut sorted = filtered;
    let order_str = resolved_sort_order(&args.sort.to_string(), args.order.as_ref());
    match args.sort {
        AuthorSortBy::Commits => {
            if order_str == "desc" {
                sorted.sort_by(|a, b| b.commits.cmp(&a.commits));
            } else {
                sorted.sort_by(|a, b| a.commits.cmp(&b.commits));
            }
        }
        AuthorSortBy::Name => {
            if order_str == "desc" {
                sorted.sort_by(|a, b| b.primary_name.cmp(&a.primary_name));
            } else {
                sorted.sort_by(|a, b| a.primary_name.cmp(&b.primary_name));
            }
        }
        AuthorSortBy::Email => {
            if order_str == "desc" {
                sorted.sort_by(|a, b| b.primary_email.cmp(&a.primary_email));
            } else {
                sorted.sort_by(|a, b| a.primary_email.cmp(&b.primary_email));
            }
        }
    }

    // Apply limit
    let limited: Vec<AuthorSummary> = match args.limit {
        Some(limit) => sorted.into_iter().take(limit as usize).collect(),
        None => sorted,
    };

    // Output
    match output_format {
        OutputFormat::Table => render_table(&limited, &args),
        OutputFormat::Json => {
            let json = author_summaries_to_json(
                &limited,
                &args.sort.to_string(),
                args.order.as_ref(),
                &args.group_by.to_string(),
                &repo_path.to_string_lossy(),
            );
            println!("{}", json);
            Ok(())
        }
        OutputFormat::Tsv => render_tsv(&limited, &args),
    }
}

/// Render the authors as a Rich-style table.
fn render_table(summaries: &[AuthorSummary], _args: &AuthorsArgs) -> Result<(), String> {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.apply_modifier(UTF8_ROUND_CORNERS);
    table.set_style(TableComponent::VerticalLines, '│');
    table.set_header(vec![
        Cell::new("Commits"),
        Cell::new("Author Name"),
        Cell::new("Author Email"),
    ]);

    for summary in summaries {
        table.add_row(vec![
            Cell::new(summary.commits.to_string())
                .fg(Color::Cyan)
                .add_attribute(comfy_table::Attribute::Bold),
            Cell::new(&summary.primary_name),
            Cell::new(&summary.primary_email).fg(Color::Magenta),
        ]);
    }

    println!("{}", table);
    Ok(())
}

/// Render the authors as TSV.
fn render_tsv(summaries: &[AuthorSummary], args: &AuthorsArgs) -> Result<(), String> {
    // Print header
    println!("repo\tsort\torder\tgroup_by\tcommits\tprimary_name\tprimary_email\tnames\temails");

    for summary in summaries {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            args.repo_path,
            args.sort.to_string(),
            resolved_sort_order(&args.sort.to_string(), args.order.as_ref()),
            args.group_by.to_string(),
            summary.commits,
            summary.primary_name,
            summary.primary_email,
            summarize_values(&summary.names),
            summarize_values(&summary.emails),
        );
    }

    Ok(())
}
