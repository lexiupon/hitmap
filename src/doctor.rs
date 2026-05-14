//! Doctor command: terminal diagnostics and capability checks.

use clap::Args;
use comfy_table::{
    Cell, Color, Row, Table, TableComponent, modifiers::UTF8_ROUND_CORNERS,
    presets::UTF8_FULL_CONDENSED,
};
use serde::Serialize;

use crate::terminal;
use std::io::IsTerminal;
use std::os::fd::AsRawFd;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct DoctorArgs {
    #[arg(
        short,
        long,
        visible_alias = "format",
        value_enum,
        help = "Output format (default: config value or table)"
    )]
    pub output_format: Option<DoctorFormat>,
    #[arg(long, help = "Include raw terminal probe details")]
    pub debug_probe: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum DoctorFormat {
    Table,
    Json,
}

impl std::fmt::Display for DoctorFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DoctorFormat::Table => write!(f, "table"),
            DoctorFormat::Json => write!(f, "json"),
        }
    }
}

pub const DEFAULT_DOCTOR_FORMAT: DoctorFormat = DoctorFormat::Table;

pub fn parse_doctor_format_name(value: &str) -> Result<DoctorFormat, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "table" => Ok(DoctorFormat::Table),
        "json" => Ok(DoctorFormat::Json),
        _ => Err("Doctor output format must be one of: table, json".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Doctor check result.
#[derive(Serialize)]
struct DoctorCheck {
    name: String,
    status: String,
    detail: String,
}

/// Doctor output.
#[derive(Serialize)]
struct DoctorOutput {
    checks: Vec<DoctorCheck>,
}

// ---------------------------------------------------------------------------
// Command implementation
// ---------------------------------------------------------------------------

/// Execute the doctor command.
pub fn doctor_command(args: DoctorArgs) -> Result<(), String> {
    let config = crate::config::load_config()?;
    let output_format = if let Some(format) = args.output_format {
        format
    } else if let Some(value) = config
        .doctor
        .as_ref()
        .and_then(|cfg| cfg.output_format.as_deref())
    {
        parse_doctor_format_name(value)?
    } else {
        DEFAULT_DOCTOR_FORMAT
    };

    let mut checks = Vec::new();

    // Check 1: stdin is a TTY
    let stdin_is_tty = std::io::stdin().is_terminal();
    checks.push(DoctorCheck {
        name: "stdin tty".to_string(),
        status: if stdin_is_tty {
            "ok".to_string()
        } else {
            "fail".to_string()
        },
        detail: if stdin_is_tty {
            "interactive".to_string()
        } else {
            "piped or redirected".to_string()
        },
    });

    // Check 2: stdout is a TTY
    let stdout_is_tty = std::io::stdout().is_terminal();
    checks.push(DoctorCheck {
        name: "stdout tty".to_string(),
        status: if stdout_is_tty {
            "ok".to_string()
        } else {
            "fail".to_string()
        },
        detail: if stdout_is_tty {
            "interactive".to_string()
        } else {
            "piped or redirected".to_string()
        },
    });

    // Check: terminal program
    let terminal_program = std::env::var("TERM_PROGRAM")
        .or_else(|_| std::env::var("TERM"))
        .unwrap_or_else(|_| "unknown".to_string());
    checks.push(DoctorCheck {
        name: "terminal program".to_string(),
        status: "ok".to_string(),
        detail: terminal_program,
    });

    // Check: theme guess from terminal background query or environment hints
    let theme_dark = crate::common::guess_dark_theme();
    checks.push(DoctorCheck {
        name: "theme guess".to_string(),
        status: "ok".to_string(),
        detail: if theme_dark {
            "dark".to_string()
        } else {
            "light".to_string()
        },
    });

    let tty_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty");

    // Check: controlling tty
    let tty_available = tty_file.is_ok();
    checks.push(DoctorCheck {
        name: "controlling tty".to_string(),
        status: if tty_available {
            "ok".to_string()
        } else {
            "fail".to_string()
        },
        detail: if tty_available {
            "/dev/tty available".to_string()
        } else {
            "/dev/tty not available".to_string()
        },
    });

    // Check 3: Kitty graphics support (only if stdout is TTY)
    let mut kitty_probe_detail: Option<String> = None;
    let mut kitty_probe_status = "fail".to_string();
    let kitty_support = match tty_file.as_ref() {
        Ok(tty_file) => {
            let fd = tty_file.as_raw_fd();
            match terminal::probe_kitty_graphics(fd, 0.35) {
                Ok(result) => {
                    if !result.graphics_seen && !result.device_attributes_seen {
                        kitty_probe_status = "warn".to_string();
                    } else if result.graphics_seen {
                        kitty_probe_status = "ok".to_string();
                    }
                    if args.debug_probe {
                        kitty_probe_detail = Some(format!(
                            "graphics_reply={} da_reply={} {}\ntrace:\n{}",
                            result.graphics_seen,
                            result.device_attributes_seen,
                            terminal::format_probe_response_preview(&result.response_bytes),
                            terminal::format_probe_trace(&result.trace),
                        ));
                    } else if !result.graphics_seen && !result.device_attributes_seen {
                        kitty_probe_detail = Some(
                            "probe was inconclusive: terminal returned no reply bytes".to_string(),
                        );
                    } else if !result.graphics_seen && result.device_attributes_seen {
                        kitty_probe_detail = Some(
                            "terminal answered device attributes but not the Kitty graphics query"
                                .to_string(),
                        );
                    }
                    result.graphics_seen
                }
                Err(err) => {
                    kitty_probe_status = "warn".to_string();
                    if args.debug_probe {
                        kitty_probe_detail = Some(format!("probe error: {}", err));
                    } else {
                        kitty_probe_detail =
                            Some(format!("probe failed before a terminal reply: {}", err));
                    }
                    false
                }
            }
        }
        Err(err) => {
            kitty_probe_status = "warn".to_string();
            if args.debug_probe {
                kitty_probe_detail = Some(format!("unable to open /dev/tty: {}", err));
            } else {
                kitty_probe_detail = Some(format!("unable to open /dev/tty for probing: {}", err));
            }
            false
        }
    };

    checks.push(DoctorCheck {
        name: "kitty graphics".to_string(),
        status: if stdout_is_tty {
            kitty_probe_status
        } else if kitty_support {
            "ok".to_string()
        } else {
            "fail".to_string()
        },
        detail: if kitty_support {
            kitty_probe_detail.unwrap_or_else(|| "probe succeeded".to_string())
        } else {
            kitty_probe_detail.unwrap_or_else(|| "not supported".to_string())
        },
    });

    // Check 4: Terminal geometry
    let terminal_geometry = match tty_file.as_ref() {
        Ok(tty_file) => {
            let fd = tty_file.as_raw_fd();
            terminal::get_terminal_geometry(fd)
        }
        Err(_) => terminal::TerminalGeometry {
            rows: 24,
            cols: 80,
            width_px: 0,
            height_px: 0,
            cell_width_px: terminal::DEFAULT_CELL_WIDTH_PX,
            cell_height_px: terminal::DEFAULT_CELL_HEIGHT_PX,
        },
    };

    checks.push(DoctorCheck {
        name: "terminal geometry".to_string(),
        status: "ok".to_string(),
        detail: format!(
            "{} cols \u{00d7} {} rows \u{00b7} {}px \u{00d7} {}px",
            terminal_geometry.cols,
            terminal_geometry.rows,
            (terminal_geometry.cell_width_px * terminal_geometry.cols as f64).round() as u32,
            (terminal_geometry.cell_height_px * terminal_geometry.rows as f64).round() as u32,
        ),
    });

    // Check: cell size
    checks.push(DoctorCheck {
        name: "cell size".to_string(),
        status: "ok".to_string(),
        detail: format!(
            "{:.1}px \u{00d7} {:.1}px",
            terminal_geometry.cell_width_px, terminal_geometry.cell_height_px
        ),
    });

    // Output
    match output_format {
        DoctorFormat::Table => render_table(&checks),
        DoctorFormat::Json => {
            let output = DoctorOutput { checks };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
            Ok(())
        }
    }
}

/// Render the doctor checks as a table.
fn render_table(checks: &[DoctorCheck]) -> Result<(), String> {
    println!();
    println!("{:^50}", "Doctor");
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.apply_modifier(UTF8_ROUND_CORNERS);
    table.set_style(TableComponent::VerticalLines, '│');
    table.set_header(vec![
        Cell::new("Check"),
        Cell::new("Status"),
        Cell::new("Detail"),
    ]);

    for check in checks {
        let status_cell = match check.status.as_str() {
            "ok" => Cell::new("✓ ok").fg(Color::Green),
            "warn" => Cell::new("⚠ warn").fg(Color::Yellow),
            "fail" => Cell::new("✗ fail").fg(Color::Red),
            _ => Cell::new(&check.status),
        };

        table.add_row(Row::from(vec![
            Cell::new(&check.name),
            status_cell,
            Cell::new(&check.detail),
        ]));
    }

    println!("{}", table);
    Ok(())
}
