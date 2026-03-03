# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] - 2026-03-03

### Breaking Changes

- **Renamed project from `ralph-cli` to `ralph-tui`.**
  The binary name has changed. If you installed a previous build, you must reinstall:

  ```sh
  just install
  # or
  cargo install --path .
  ```

  The new binary is named `ralph-tui`. Any scripts, aliases, or PATH entries that
  reference `ralph-cli` must be updated to `ralph-tui`.

### Changed

- Cargo package name updated from `ralph-cli` to `ralph-tui`.
- Binary name updated from `ralph-cli` to `ralph-tui`.
- Package description updated to reflect TUI focus.
- README, CLAUDE.md, and Justfile updated throughout to reference the new name.

---

## [0.0.1] - Initial release

- Initial scaffold: ratatui TUI with workflows tab, runner tab, and live subprocess streaming.
