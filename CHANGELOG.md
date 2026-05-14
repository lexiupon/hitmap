# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.1] - 2026-05-14

Bug-fix release for the first public Rust version of `hitmap`.

### Fixed

- Corrected the CLI-reported version so `hitmap --version` now matches the package version from `Cargo.toml`.
- Removed the legacy Python `hitmap` script from the repository to avoid versioning confusion between old and current implementations.

## [0.1.0] - 2026-05-14

Initial public release of `hitmap`.

### Added

- GitHub-style git contribution heatmaps for any local repository.
- Dual rendering modes:
  - Kitty Graphics Protocol inline image output for high-fidelity terminal rendering.
  - Portable Unicode text rendering for terminals without inline image support.
- PNG export support for sharing, screenshots, and documentation.
- Flexible date filtering with rolling windows like `--last 90d` and exact ranges via `--from` / `--to`.
- Author filtering by exact name or email, plus an `authors` command to inspect repository identities.
- Multiple built-in color profiles including GitHub, Aurora, Ocean, Fire, and Catppuccin variants.
- Configurable intensity scaling with linear and Fibonacci-style threshold profiles.
- Persistent user configuration via XDG config files and `hitmap config` subcommands.
- Terminal diagnostics with `hitmap doctor` to validate rendering support and environment setup.
- Sensible CLI defaults so running `hitmap` renders the current repository without extra setup.
