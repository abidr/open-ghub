<div align="center">

<img src="app-icon.png" alt="Open GHub" width="96" height="96">

# Open GHub

**A lightweight, open-source alternative to Logitech G HUB for configuring Logitech gaming mice on Windows.**

[![Latest release](https://img.shields.io/github/v/release/abidr/open-ghub?label=release)](https://github.com/abidr/open-ghub/releases/latest)
[![CI](https://github.com/abidr/open-ghub/actions/workflows/ci.yml/badge.svg)](https://github.com/abidr/open-ghub/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)
![Platform: Windows](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078d4)

[Download](https://github.com/abidr/open-ghub/releases/latest) ·
[Features](#features) ·
[FAQ](#faq) ·
[Contributing](CONTRIBUTING.md) ·
[Changelog](CHANGELOG.md)

</div>

---

Open GHub talks to your mouse directly over Logitech's HID++ 2.0 protocol, the
same protocol G HUB uses. It needs no G HUB installation and no background
service, and it runs in about 27 MB of RAM from the system tray.

<p align="center">
  <img src="docs/screenshots/dpi.png" alt="Open GHub DPI settings for a PRO X SUPERLIGHT" width="820">
</p>

> [!NOTE]
> Open GHub is an independent project. It is not affiliated with, endorsed by
> or supported by Logitech. Logitech, Logitech G, G HUB, LIGHTSPEED and product
> names are trademarks of Logitech, used here only to identify compatible
> hardware.

## Features

- **DPI levels and report rate:** up to 5 DPI levels with separate X/Y on supported mice, and report rates up to 8000 Hz.
- **Per-application profiles:** profiles switch automatically when a linked game or app comes to the foreground, with a Desktop fallback.
- **On-board memory:** view and edit the profiles stored inside the mouse, which keep working with Open GHub closed. Every write is backed up first and verified by reading it back.
- **Button assignments:** give buttons keyboard shortcuts (Ctrl+C, Win+Tab and so on), media keys or mouse functions, with a G-Shift layer. They are stored in the mouse, so they work without G HUB or Open GHub running.
- **Battery:** shows the level in the app and tray, with low-battery notifications at 15% and 5%.
- **Import from G HUB:** converts your existing G HUB profiles in one click (optional).
- **Lightweight:** event-driven with no polling. In the tray no WebView runs, so idle CPU is effectively zero.
- **Automatic updates:** signed updates from GitHub Releases. You can turn update checks off.
- **No account, no telemetry:** see the [privacy policy](#privacy-policy).

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/devices.png" alt="Devices page"><p align="center"><b>Devices</b></p></td>
    <td width="50%"><img src="docs/screenshots/profiles.png" alt="Profiles page with auto-switching"><p align="center"><b>Profiles and auto-switching</b></p></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/onboard-memory.png" alt="On-board memory slots"><p align="center"><b>On-board memory</b></p></td>
    <td width="50%"><img src="docs/screenshots/onboard-slot-editor.png" alt="Editing an on-board slot"><p align="center"><b>Editing an on-board slot</b></p></td>
  </tr>
</table>

## Supported devices

Open GHub works with Logitech G mice that speak HID++ 2.0, connected by USB
cable or through a LIGHTSPEED or Unifying receiver. Its built-in table names
about 110 Logitech G products.

| Status | Devices |
| --- | --- |
| Tested | PRO X SUPERLIGHT |
| Expected to work | Other HID++ 2.0 Logitech G mice (G502 X, G Pro Wireless, G305, G703, G903 and others) |
| Not supported yet | Keyboards, headsets and lighting control |

Has your mouse been tested? Please
[report it](https://github.com/abidr/open-ghub/issues/new?template=device_report.yml),
whether it works or not.

## Installation

1. Download `Open-GHub_<version>_x64-setup.exe` from the
   [latest release](https://github.com/abidr/open-ghub/releases/latest).
2. Run the installer. It installs for the current user and doesn't need
   administrator rights.
3. Start **Open GHub** from the Start menu.

**Requirements:** Windows 10 or 11 (64-bit) and the Microsoft Edge WebView2
runtime, which comes preinstalled on Windows 11.

> [!TIP]
> Quit G HUB while you use Open GHub (right-click its tray icon, then
> Quit). Otherwise G HUB may reapply its own settings to the mouse.

## Usage

### Tray and background mode

Closing the window keeps Open GHub running in the system tray, so profile
switching and battery warnings keep working. The tray menu shows battery
levels and lets you switch profiles, toggle auto-switching, check for updates,
start with Windows, or quit. When **Start with Windows** is on, Open GHub
starts minimized to the tray.

### Profiles

Each profile stores DPI levels, the active level and a report rate, and can be
linked to an application. That's either a specific `.exe` or a folder; the
longest match wins. With **Auto-switch** on, the matching profile is applied
whenever its app comes to the foreground. **Desktop** is used when nothing
else matches. Profiles live in software. To keep settings with Open GHub
closed, use on-board memory.

### On-board memory

Go to **Device settings → On-board memory** to read the profiles stored in the
mouse. Click the pencil on a slot to change its DPI levels, starting level and
report rate. Only those bytes change; buttons, lighting and the profile name
stay exactly as they were.

Open **Assignments** in the left rail to change what the buttons do. Pick a slot and a button,
then pick a command (for example Copy or Open Task View), press a key
combination on the Keys tab, or choose a media key or mouse function on the
Media and System tabs. The **G-Shift** switch edits the second layer, used
while a button assigned to G-Shift is held. Only the buttons you change are
written. Assignments run inside the mouse, so they need on-board memory mode.
Switch to it from Open GHub rather than G HUB: G HUB leaves the buttons it
handled captured, and Open GHub's switch releases them. They
can't be macros or application-specific actions, which need software.

If buttons bound to mouse clicks work but keyboard shortcuts do nothing, check
that Windows has the receiver's keyboard interface enabled: in Device Manager,
under **Keyboards**, a "HID Keyboard Device" with a warning icon (code 22,
disabled) blocks every keystroke the mouse sends. G HUB doesn't need that
interface because it types shortcuts in software, so it can stay disabled
unnoticed. The Assignments page warns when this is the case and shows the
command to fix it. Enable it (right-click → Enable device), or from an administrator
prompt: `pnputil /enable-device "<instance id>"`.

Before every write, Open GHub saves a backup to
`%APPDATA%\dev.local.openghub\onboard-backups\`. You can restore any backup
from the same screen. The factory profile is never written.

### Updates

Open GHub checks for a new release 30 seconds after it starts and then every
6 hours. When one is available it shows a notification, a tray item and an
Install button in **App settings** (gear icon). You can also turn on automatic
installation there. Every update is signed, and Open GHub checks the signature
before installing.

## FAQ

<details>
<summary><b>Do I need G HUB installed?</b></summary>

No. Open GHub is standalone. G HUB is only used if you choose **Import from G
HUB**, which reads its settings database read-only.
</details>

<details>
<summary><b>Windows SmartScreen warns about the installer</b></summary>

Releases are being moved to code signing through the SignPath Foundation (see
[Code signing policy](#code-signing-policy)). Until a release is signed, click
**More info → Run anyway**. You can always check that the installer came from
this repository's Releases page.
</details>

<details>
<summary><b>My DPI changes are lost when the mouse turns off</b></summary>

In software mode, settings last until the mouse loses power; Open GHub
reapplies your active profile when the mouse reconnects. To keep settings with
Open GHub closed, write them to the mouse's on-board memory and switch the
mouse to on-board mode.
</details>

<details>
<summary><b>The mouse shows the wrong name</b></summary>

Firmware often reports an internal name. Open GHub corrects it using its
product table in [`device_db.rs`](src-tauri/src/device_db.rs). If yours is
missing, open a [device report](https://github.com/abidr/open-ghub/issues/new?template=device_report.yml)
or send a pull request.
</details>

<details>
<summary><b>How do I collect logs for a bug report?</b></summary>

Quit Open GHub from the tray, then run it from PowerShell with logging on. Use
it until the problem happens, then quit from the tray again:

```powershell
$env:OPEN_GHUB_LOG = "1"
& "$env:LOCALAPPDATA\Open GHub\open-ghub.exe" 2> "$env:TEMP\open-ghub.log"
```

To save the detected devices and their settings as JSON without changing
anything, run:

```powershell
& "$env:LOCALAPPDATA\Open GHub\open-ghub.exe" --probe > "$env:TEMP\open-ghub-probe.json"
```

Attach both files to your issue.
</details>

## Building from source

**Prerequisites:** [Rust](https://rustup.rs) stable (MSVC), Visual Studio
Build Tools with the "Desktop development with C++" workload, and Node.js 20
or later.

```sh
git clone https://github.com/abidr/open-ghub.git
cd open-ghub
npm install
npm run app:dev                       # run with hot reload
npx tauri build --no-bundle           # release build of the executable
cd src-tauri && cargo test            # unit tests
```

> [!IMPORTANT]
> Build the app with the Tauri CLI (`npx tauri ...`), not plain `cargo build`.
> A plain cargo build expects the Vite dev server and shows "localhost refused
> to connect".

For more detail, see [Architecture](docs/architecture.md) for how the code is
laid out and how it maps to HID++, and [Releasing](docs/releasing.md) for the
release and signing pipeline.

## Contributing

Contributions are welcome: bug reports, device reports, documentation and
code. Please read the [contributing guide](CONTRIBUTING.md) and follow the
[code of conduct](CODE_OF_CONDUCT.md). To report a security issue, see the
[security policy](SECURITY.md).

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io),
certificate by [SignPath Foundation](https://signpath.org).

Windows releases are built from this repository's source by GitHub Actions and
signed through SignPath. Only the [release workflow](.github/workflows/release.yml)
can submit signing requests, and every release-signing request is approved by
hand. The SignPath artifact configurations are in
[`.signpath/artifact-configurations`](.signpath/artifact-configurations).

**Team roles**

- Committers and reviewers: [Abidur Rahman](https://github.com/abidr)
- Approvers: [Abidur Rahman](https://github.com/abidr)

All team members use multi-factor authentication on GitHub and SignPath.

## Privacy policy

This program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or
operating it, with one exception: the update check. About 30 seconds after
start and then every 6 hours, Open GHub downloads `latest.json` from this
repository's GitHub releases, and when you install an update it downloads the
installer from the same release. GitHub receives the usual request information
(such as your IP address) under the
[GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).
Turn the update check off in **App settings** (gear icon) under
**Check for updates automatically**. The installer shows this notice before
installing. Open GHub has no accounts, telemetry, analytics or crash reporting.

## Acknowledgements

- [Solaar](https://github.com/pwr-Solaar/Solaar) and
  [libratbag](https://github.com/libratbag/libratbag), whose open HID++ work
  documents much of the protocol.
- [Tauri](https://tauri.app), [shadcn/ui](https://ui.shadcn.com) and
  [hidapi](https://github.com/ruabmbua/hidapi-rs).

## License

Copyright (C) 2026 Abidur Rahman

Open GHub is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY. See [LICENSE](LICENSE) for details.
