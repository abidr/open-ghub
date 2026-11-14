# Architecture

Open GHub is a [Tauri 2](https://tauri.app) app: a Rust core that talks to the
mouse, and a React front end shown in a WebView2 window.

```
G HUB:      Electron UI ──named pipe──► lghub_agent.exe ──HID++──► mouse
Open GHub:  React UI ───Tauri invoke──► Rust core (src-tauri) ──HID++──► mouse
```

## Source layout

### Rust core (`src-tauri/src`)

| File | Responsibility |
| --- | --- |
| `hidpp.rs` | HID++ transport: short (`0x10`) and long (`0x11`) reports, one background reader per HID collection, request/response matching on software id `0xB` |
| `manager.rs` | Device enumeration, pairing short and long collections, receiver slot probing and rescans |
| `device.rs` | Feature discovery, plus DPI, report rate, battery and on-board/host mode |
| `device_db.rs` | Product id → marketing name table |
| `onboard.rs` | On-board memory (`0x8100`): read, patch, write with verification, backup and restore |
| `events.rs` | Event loop: HID++ notifications, device arrival and removal, battery warnings |
| `foreground.rs` | Foreground-window hook used by profile auto-switching |
| `keyboard_iface.rs` | Detects a disabled Windows keyboard interface on the receiver, which drops keystroke bindings |
| `profiles.rs` | Profile store (`profiles.json`), app matching, G HUB import |
| `tray.rs` | Tray icon and menu, notifications, creating the window on demand |
| `settings.rs` | App settings (`settings.json`) |
| `updater.rs` | Update checks and installation |
| `main.rs` | Tauri commands, app setup and command-line diagnostics |

### Front end (`src`)

- `lib/api.ts`: typed wrappers around the Tauri commands, and shared types
- `components/`: header, home (device cards), device page, DPI, settings and on-board panels, profiles
- `components/ui/`: [shadcn/ui](https://ui.shadcn.com) components
- `index.css`: Tailwind theme with G HUB's colour palette

## HID++ features used

| Setting | HID++ feature | Notes |
| --- | --- | --- |
| Device discovery | Root `0x0000`, FeatureSet `0x0001` | Direct USB devices and receiver slots 1–6 |
| Product ids | DeviceInformation `0x0003` | Used to look up the name in `device_db.rs` |
| Name and type | `0x0005` | |
| Battery | `0x1004`, `0x1000`, `0x1001` | Whichever the device has |
| DPI | `0x2201` AdjustableDPI, `0x2202` ExtendedAdjustableDPI | Separate X/Y and lift-off distance on `0x2202` mice |
| Report rate | `0x8060`, `0x8061` | Up to 8000 Hz on supported mice |
| On-board profiles | `0x8100` OnboardProfiles | Mode switching, profile memory read and write |

## Event model

Open GHub doesn't poll:

- Profile auto-switching uses a Windows foreground event hook (`SetWinEventHook`).
- Battery, DPI and profile changes come from the mouse's HID++ notifications.
- A mouse waking up or reconnecting is reported by the receiver's connection
  notification. Open GHub turns these on in receiver register `0x00`.
- Plugging devices in and out comes from Windows device notifications.
- A safety battery check runs every 15 minutes.

The window is destroyed when it's closed and created again when it's opened,
so no WebView2 processes run while Open GHub sits in the tray. WebView2 runs
with `--disable-gpu`, which avoids a GPU process of about 100 MB.

## On-board memory layout

The layout is the same as libratbag's implementation and G HUB's on-board
model:

- Sector `0x0000` is the profile directory.
- User profiles are in sectors `0x0001`…, and factory (ROM) profiles in `0x0101`….
- Every user sector ends with a big-endian CRC-16/CCITT (seed `0xFFFF`). ROM
  sectors have no CRC.
- The mouse reports its sector size and profile count.

Within a profile: report rate at byte 0, default and shift DPI index at 1–2,
five little-endian DPI values from 3, buttons from `0x20` (16 × 4 bytes),
G-Shift buttons from `0x60`, and the name from `0xA0`.

Each button binding is 4 bytes (`BindingSpec` in `onboard.rs` is the encoder,
`decode_binding` the decoder):

| Bytes | Meaning |
| --- | --- |
| `80 00 00 00` | No action |
| `80 01 <mask hi> <mask lo>` | Mouse button (bit n-1 of a big-endian mask) |
| `80 02 <modifiers> <key>` | Keyboard: HID modifier bits, HID key usage (Ctrl+C is `80 02 01 06`) |
| `80 03 <usage hi> <usage lo>` | Consumer control (media keys) |
| `90 <id> 00 00` | Mouse function (DPI, profile, G-Shift, wheel tilt) |
| `FF FF FF FF` | Disabled |

### G HUB's button capture (0x8110)

To run software Commands, G HUB remaps the buttons it handles to "nothing"
through Mouse Button Spy (`0x8110`, fn3 getRemapping / fn4 setRemapping,
one byte per physical button) and catches the raw presses itself. The table
stays in the mouse after G HUB quits, and those buttons then do nothing in
either mode, because the on-board bindings never see them. Switching to
on-board memory in Open GHub stops spying and writes the identity table
(1, 2, 3, ...), unless G HUB's agent is running.

A slot write reads the sector, patches only the bytes being changed (report
rate and DPI, and/or the requested button bindings),
recomputes the CRC, writes, reads back and compares. On a mismatch the
original sector is written back. Writes to ROM sectors are refused, and so are
writes while G HUB's agent is running.

## Command-line diagnostics

```
open-ghub.exe --probe                     print devices and settings as JSON (read-only)
open-ghub.exe --ghub-profiles             print what a G HUB import would produce (read-only)
open-ghub.exe --onboard-dump [file]       dump on-board memory (read-only)
open-ghub.exe --onboard-write <slot> <dpi,dpi,..> <start-index> <hz> <backup.json>
open-ghub.exe --button-state               print mode, active slot and the 0x8110 button remap table (read-only)
open-ghub.exe --onboard-mode <onboard|host>  switch mode; going on-board also resets the 0x8110 table
open-ghub.exe --onboard-bind <slot> <button> <default|gshift> '<binding json>' <backup.json>
open-ghub.exe --onboard-restore <backup.json>
```

Set `OPEN_GHUB_LOG=1` to print every event to the console.

## Data locations

| Path | Contents |
| --- | --- |
| `%APPDATA%\dev.local.openghub\profiles.json` | Profiles |
| `%APPDATA%\dev.local.openghub\settings.json` | App settings |
| `%APPDATA%\dev.local.openghub\onboard-backups\` | Automatic on-board memory backups |
