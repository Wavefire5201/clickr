# Changelog

All notable changes to clickr are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-09-17

### Security
- Refuse to start when setuid, setgid, or carrying file capabilities, not only when root.
- Mark the process non-dumpable so other processes of the same user cannot read
  buffered keyboard events from its memory, and no core dump can contain them.
- Release workflow: the build job no longer holds a repository write token, all
  actions are pinned to commit SHAs, and release tarballs carry build provenance
  attestations.
- Weekly `cargo audit` workflow.
- CI workflow: rustfmt, clippy with warnings denied, build, and tests on every push. Dependabot for actions and crates.
- `rust-version = "1.88"` declared for packagers (let chains).
- Device names from hardware are stripped of control characters before printing.
- Lockfile refreshed; drops a quick-xml version with two RUSTSEC advisories.

### Fixed
- Ctrl+C quits the TUI. SIGTERM and SIGHUP shut down cleanly, restoring the terminal.
- Hotkey listener threads are reaped after a device disappears, and a device that
  cannot be opened is retried with back-off instead of every ten seconds forever.
- A panic in the clicker thread stops the app instead of leaving the TUI showing
  "CLICKING" with nothing behind it.
- A panic no longer poisons the settings lock for every other thread.
- Speed gauge label is readable on dark terminal themes.
- Key hints and section titles are brighter.

### Changed
- README rewritten: install instructions first, badges, demo GIF.
- Cargo metadata for crates.io (keywords, categories, readme).

## [0.1.0] - 2026-09-11

First tagged release. Same code as the `clickr-git` AUR package at the time, plus:

### Added
- `--version` and `--help` flags.
- Release workflow producing static musl binaries for x86_64 and aarch64 with sha256 files.

[Unreleased]: https://github.com/Wavefire5201/clickr/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/Wavefire5201/clickr/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Wavefire5201/clickr/releases/tag/v0.1.0
