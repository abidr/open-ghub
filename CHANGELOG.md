# Changelog

All notable changes to Open GHub are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- An Assignments page (left rail) for on-board slots: give any button a keyboard shortcut
  (for example Ctrl+C), a media key, a mouse button or a mouse function such as
  DPI cycle or G-Shift, in the default layer or the G-Shift layer. The
  bindings are stored in the mouse, so they work with Open GHub and G HUB
  closed. Only the buttons you change are written, with the same backup and
  read-back verification as DPI edits.
- A warning on the Assignments page when Windows has the receiver's keyboard
  interface disabled, which silently drops every keystroke binding.
- `--onboard-bind`, `--onboard-mode`, `--button-state`, `--listen` and
  `--blocked-keyboards` command-line options.

### Fixed

- Buttons that G HUB had captured (remapped to "nothing" through feature
  0x8110 so it could run Commands itself) stayed dead after G HUB quit, even in
  on-board mode. Switching to on-board memory now restores the normal button
  table.

## [0.1.2] - 2026-10-20

### Added

- On-board memory view: the profiles stored in the mouse, with DPI levels,
  report rate, button bindings and the factory profile.
- Editing an on-board slot's DPI levels, starting level and report rate. Each
  write is backed up first and verified by reading it back.
- Backing up on-board memory to a file and restoring it.
- A privacy notice shown in the installer.

### Changed

- Releases are now built and published by GitHub Actions, ready for code
  signing through the SignPath Foundation.

## [0.1.1] - 2026-10-20

### Added

- Automatic updates from GitHub Releases, with an option to install them
  automatically.
- The active profile is applied again when a mouse reconnects or wakes up.

## [0.1.0] - 2026-10-20

### Added

- First release: DPI, report rate, battery and on-board/software mode control
  over HID++ 2.0.
- Per-application profiles with auto-switching, and import from G HUB.
- Tray mode, start with Windows and low-battery notifications.

[Unreleased]: https://github.com/abidr/open-ghub/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/abidr/open-ghub/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/abidr/open-ghub/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/abidr/open-ghub/releases/tag/v0.1.0
