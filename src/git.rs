//! Git repository access and log querying.
//!
//! Uses subprocess `git log` to query commit dates, author names, and emails.

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, NaiveDate, Utc};

use crate::common::*;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum GitError {
    InvalidRepo(String),
    Io(std::io::Error),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::InvalidRepo(path) => write!(f, "Invalid repository path: {}", path),
            GitError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for GitError {}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        GitError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single commit entry from git log.
#[derive(Debug, Clone)]
pub struct CommitEntry {
    pub date: NaiveDate,
    pub author_name: String,
    pub author_email: String,
}

/// Resolved git repository root path.
#[derive(Debug, Clone)]
pub struct RepoRoot {
    pub path: PathBuf,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Validate and resolve the target path to a git working tree root.
pub fn resolve_repo_root(repo_path: &str) -> Result<RepoRoot, GitError> {
    let path = PathBuf::from(crate::expand_tilde(repo_path));

    // Resolve the path without requiring canonicalize (macOS sandbox can fail it)
    let absolute = if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir()
            .map_err(|e| GitError::InvalidRepo(format!("Failed to get current directory: {}", e)))?
            .join(&path)
    };

    // Walk up from the given path looking for .git
    let mut current = absolute;
    loop {
        if current.join(".git").is_dir() || current.join(".git").exists() {
            return Ok(RepoRoot { path: current });
        }
        if !current.pop() {
            return Err(GitError::InvalidRepo(path.to_string_lossy().to_string()));
        }
    }
}

/// Validate and resolve the target path to a git working tree root.
pub fn iter_commit_entries(
    repo_path: &Path,
    start_date: Option<&NaiveDate>,
    end_date: Option<&NaiveDate>,
) -> Result<Vec<CommitEntry>, GitError> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_path);
    cmd.args([
        "log",
        "--date=short",
        &format!("--pretty=format:%ad{}%an{}%ae", '\x1f', '\x1f'),
    ]);

    if let Some(sd) = start_date {
        cmd.arg("--since");
        cmd.arg(&sd.format("%d %b %Y").to_string());
    }
    if let Some(ed) = end_date {
        cmd.arg("--before");
        let ed_plus_one = *ed + chrono::Duration::days(1);
        cmd.arg(&ed_plus_one.format("%d %b %Y").to_string());
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let _stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::InvalidRepo(
            repo_path.to_string_lossy().to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() != 3 {
            continue;
        }

        let date = NaiveDate::parse_from_str(parts[0].trim(), "%Y-%m-%d")
            .ok()
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());

        entries.push(CommitEntry {
            date,
            author_name: parts[1].trim().to_string(),
            author_email: parts[2].trim().to_string(),
        });
    }

    Ok(entries)
}

/// Ensure the author selection matches at least one commit in the repository.
/// Returns early (ok) if `all_authors` is true.
pub fn ensure_author_selection_exists(
    repo_path: &Path,
    selection: &AuthorSelection,
) -> Result<(), GitError> {
    if selection.all_authors {
        return Ok(());
    }

    let entries = iter_commit_entries(repo_path, None, None)?;
    for entry in &entries {
        if author_matches(selection, &entry.author_name, &entry.author_email) {
            return Ok(());
        }
    }

    Err(GitError::InvalidRepo(
        "No author matched the requested filters. Use `hitmap authors REPO --search ...` to inspect available identities".to_string(),
    ))
}

/// Collect per-day commit counts for the selected authors and date range.
pub fn collect_commit_day_counts(
    repo_path: &Path,
    selection: &AuthorSelection,
    start_date: &DateTime<Utc>,
    end_date: &DateTime<Utc>,
) -> Result<
    (
        std::collections::HashMap<String, u64>,
        u64,
        Vec<(String, String)>,
    ),
    GitError,
> {
    let start = start_date.date_naive();
    let end = end_date.date_naive();

    let mut day_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut total_commits: u64 = 0;
    let mut matched_identities: Vec<(String, String)> = Vec::new();

    // Collect all entries first (git log may be large)
    let entries = iter_commit_entries(repo_path, Some(&start), Some(&end))?;

    for entry in &entries {
        if !author_matches(selection, &entry.author_name, &entry.author_email) {
            continue;
        }

        let key = entry.date.format("%Y-%m-%d").to_string();
        *day_counts.entry(key).or_insert(0) += 1;
        total_commits += 1;
        matched_identities.push((entry.author_name.clone(), entry.author_email.clone()));
    }

    Ok((day_counts, total_commits, matched_identities))
}

/// Collect git author identities grouped by pair, name, or email.
pub fn collect_author_summaries(
    repo_path: &Path,
    group_by: &str,
) -> Result<Vec<AuthorSummary>, GitError> {
    // Get all unique entries
    let entries = iter_commit_entries(repo_path, None, None)?;

    // Group by the specified field
    let mut buckets: Vec<(String, String, u64)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for entry in &entries {
        let key = match group_by {
            "pair" => format!(
                "{}\x1f{}",
                normalize_text(&entry.author_name),
                normalize_text(&entry.author_email)
            ),
            "name" => normalize_text(&entry.author_name),
            "email" => normalize_text(&entry.author_email),
            _ => continue,
        };

        if let Some(bucket_idx) = seen.iter().position(|s| s == &key) {
            buckets[bucket_idx].2 += 1;
        } else {
            seen.push(key.clone());
            buckets.push((key.clone(), entry.author_name.clone(), 1));
        }
    }

    // Expand buckets into AuthorSummary structs
    let mut summaries = Vec::new();
    for (key, primary_name, commits) in buckets {
        let mut names: Vec<String> = Vec::new();
        let mut emails: Vec<String> = Vec::new();

        for entry in &entries {
            let entry_key = match group_by {
                "pair" => format!(
                    "{}\x1f{}",
                    normalize_text(&entry.author_name),
                    normalize_text(&entry.author_email)
                ),
                "name" => normalize_text(&entry.author_name),
                "email" => normalize_text(&entry.author_email),
                _ => continue,
            };

            if entry_key == key {
                if !names.contains(&entry.author_name) {
                    names.push(entry.author_name.clone());
                }
                if !emails.contains(&entry.author_email) {
                    emails.push(entry.author_email.clone());
                }
            }
        }

        names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        emails.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

        summaries.push(AuthorSummary {
            commits,
            primary_name,
            primary_email: entries
                .iter()
                .find(|e| {
                    let ek = match group_by {
                        "pair" => format!(
                            "{}\x1f{}",
                            normalize_text(&e.author_name),
                            normalize_text(&e.author_email)
                        ),
                        "name" => normalize_text(&e.author_name),
                        "email" => normalize_text(&e.author_email),
                        _ => return false,
                    };
                    ek == key
                })
                .map(|e| e.author_email.clone())
                .unwrap_or_default(),
            names,
            emails,
        });
    }

    Ok(summaries)
}
