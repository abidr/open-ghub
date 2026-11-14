//! On-board profile memory (HID++ 0x8100).
//!
//! Layout (matches G HUB's onboard_profiles model and libratbag's HID++
//! implementation): memory is split into sectors. Sector 0x0000 is the
//! profile directory (4-byte entries: big-endian sector address, enabled
//! flag, reserved; 0xFFFF ends the list). User profiles live in sectors
//! 0x0001.., factory (ROM) profiles in 0x0101... Every user sector ends with
//! a big-endian CRC-CCITT (seed 0xFFFF) of the bytes before it.
//!
//! Reads have no side effects. Writes ([`Device::write_slot_settings`],
//! [`Device::restore_sectors`]) only touch user profile sectors and the
//! directory, change only the bytes Open GHub manages, recompute the CRC and
//! verify by reading back. Factory (ROM) sectors are never written.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::device::{feature::ONBOARD_PROFILES, Device};
use crate::hidpp::{Error, Result, Transport};

const DIRECTORY_SECTOR: u16 = 0x0000;
const ROM_BASE: u16 = 0x0100;
const DIRECTORY_END: u16 = 0xFFFF;
/// Committing a sector to flash can take longer than an ordinary request.
const COMMIT_TIMEOUT: Duration = Duration::from_secs(5);
/// Report rates the on-board format can store, as a period in milliseconds.
const STORABLE_RATES_MS: [u8; 4] = [1, 2, 4, 8];

/// Function numbers of feature 0x8100.
mod func {
    pub const GET_INFO: u8 = 0;
    pub const SET_ACTIVE_PROFILE: u8 = 3;
    pub const GET_ACTIVE_PROFILE: u8 = 4;
    pub const READ: u8 = 5;
    pub const WRITE_START: u8 = 6;
    pub const WRITE_DATA: u8 = 7;
    pub const WRITE_END: u8 = 8;
    pub const GET_ACTIVE_DPI_INDEX: u8 = 0xB;
}

/// Offsets inside a profile sector of the button tables: 16 four-byte
/// bindings, then the 16 G-Shift bindings.
const BUTTONS_AT: usize = 0x20;
const GSHIFT_AT: usize = BUTTONS_AT + 16 * 4;
const MAX_BUTTONS: usize = 16;

/// A button binding to store. This is the inverse of [`decode_binding`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BindingSpec {
    /// The button does nothing.
    None,
    /// The button is disabled (not even reported to the PC).
    Disabled,
    /// A standard mouse button, 1 = left, 2 = right, 3 = middle, 4 = back, 5 = forward.
    Mouse { button: u8 },
    /// A keyboard key: HID modifier bitmask plus HID keyboard usage. A key of
    /// 0 sends the modifiers alone.
    Key { modifiers: u8, key: u8 },
    /// A HID consumer-control usage (media keys).
    Media { usage: u16 },
    /// A mouse-side function: DPI, profile, G-Shift or wheel tilt. See `SPECIALS`.
    Special { id: u8 },
}

impl BindingSpec {
    /// The 4 bytes stored in the profile.
    pub fn encode(&self) -> std::result::Result<[u8; 4], String> {
        match *self {
            BindingSpec::None => Ok([0x80, 0x00, 0x00, 0x00]),
            BindingSpec::Disabled => Ok([0xFF; 4]),
            BindingSpec::Mouse { button } => {
                if !(1..=16).contains(&button) {
                    return Err(format!("mouse button {button} is out of range (1-16)"));
                }
                let [hi, lo] = (1u16 << (button - 1)).to_be_bytes();
                Ok([0x80, 0x01, hi, lo])
            }
            BindingSpec::Key { modifiers, key } => {
                if key >= 0xE0 {
                    return Err(format!("key 0x{key:02x} is a modifier; set it in the modifier mask instead"));
                }
                if key == 0 && modifiers == 0 {
                    return Err("a key binding needs a key or at least one modifier".into());
                }
                Ok([0x80, 0x02, modifiers, key])
            }
            BindingSpec::Media { usage } => {
                if usage == 0 {
                    return Err("a media binding needs a usage code".into());
                }
                let [hi, lo] = usage.to_be_bytes();
                Ok([0x80, 0x03, hi, lo])
            }
            BindingSpec::Special { id } => {
                if id == 0 || usize::from(id) >= SPECIALS.len() {
                    return Err(format!("special function {id} doesn't exist"));
                }
                Ok([0x90, id, 0x00, 0x00])
            }
        }
    }
}

/// Replaces one button of a slot, in the default layer or the G-Shift layer.
#[derive(Clone, Debug, Deserialize)]
pub struct ButtonChange {
    /// 0-based button index, as in the profile's button table.
    pub index: u8,
    #[serde(default)]
    pub gshift: bool,
    pub binding: BindingSpec,
}

/// What to change in one user slot. DPI levels, starting level and report
/// rate go together: give all three or none. Buttons are independent of them.
#[derive(Clone, Debug, Deserialize)]
pub struct SlotSettings {
    pub slot: u8,
    pub dpi_levels: Option<Vec<u16>>,
    pub default_index: Option<u8>,
    pub report_rate_hz: Option<u32>,
    #[serde(default)]
    pub buttons: Vec<ButtonChange>,
}

/// Result of a verified write.
#[derive(Clone, Debug, Serialize)]
pub struct WriteReport {
    pub slot: u8,
    pub sector: u16,
    /// Bytes that differ from what was stored before.
    pub bytes_changed: usize,
    /// True when nothing changed, so nothing was written.
    pub unchanged: bool,
}

/// Puts `settings` into a copy of a profile sector. Only the report rate,
/// starting and shift DPI indices, the five DPI values and the button
/// bindings that were asked for change; everything else (other buttons,
/// lighting, power settings, the name) is kept byte for byte. The CRC is
/// recomputed.
pub fn patch_profile_sector(original: &[u8], settings: &SlotSettings) -> std::result::Result<Vec<u8>, String> {
    if original.len() < GSHIFT_AT + MAX_BUTTONS * 4 {
        return Err("sector too small for a profile".into());
    }
    let mut data = original.to_vec();

    match (&settings.dpi_levels, settings.default_index, settings.report_rate_hz) {
        (None, None, None) => {}
        (Some(levels), Some(default_index), Some(rate_hz)) => {
            if levels.is_empty() || levels.len() > 5 {
                return Err("between 1 and 5 DPI levels are required".into());
            }
            if levels.iter().any(|&d| d == 0 || d == 0xFFFF) {
                return Err("DPI levels must be between 1 and 65534".into());
            }
            if usize::from(default_index) >= levels.len() {
                return Err("the starting level must be one of the DPI levels".into());
            }
            let period = (1000 / rate_hz.max(1)).min(255) as u8;
            if !STORABLE_RATES_MS.contains(&period) || 1000 / u32::from(period) != rate_hz {
                return Err(format!("{rate_hz} Hz can't be stored on the mouse; use 125, 250, 500 or 1000 Hz"));
            }
            data[0] = period;
            data[1] = default_index;
            // Keep the shift level valid for the new number of levels.
            if usize::from(data[2]) >= levels.len() {
                data[2] = 0;
            }
            for i in 0..5 {
                let value = levels.get(i).copied().unwrap_or(0);
                data[3 + 2 * i..5 + 2 * i].copy_from_slice(&value.to_le_bytes());
            }
        }
        _ => return Err("DPI levels, starting level and report rate must be given together".into()),
    }

    for change in &settings.buttons {
        if usize::from(change.index) >= MAX_BUTTONS {
            return Err(format!("button {} doesn't exist", u16::from(change.index) + 1));
        }
        let base = if change.gshift { GSHIFT_AT } else { BUTTONS_AT };
        let at = base + 4 * usize::from(change.index);
        data[at..at + 4].copy_from_slice(&change.binding.encode()?);
    }

    let n = data.len();
    let crc = crc_ccitt(&data[..n - 2]);
    data[n - 2..].copy_from_slice(&crc.to_be_bytes());
    Ok(data)
}

pub fn crc_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        let temp = (crc >> 8) ^ u16::from(byte);
        crc <<= 8;
        let mut quick = temp ^ (temp >> 4);
        crc ^= quick;
        quick <<= 5;
        crc ^= quick;
        quick <<= 7;
        crc ^= quick;
    }
    crc
}

fn sector_crc_ok(data: &[u8]) -> bool {
    data.len() >= 2 && crc_ccitt(&data[..data.len() - 2]) == u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]])
}

#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub memory_model: u8,
    pub profile_format: u8,
    pub macro_format: u8,
    pub profile_count: u8,
    pub rom_profile_count: u8,
    pub button_count: u8,
    pub sector_count: u8,
    pub sector_size: u16,
    pub mechanical_layout: u8,
    pub various_info: u8,
}

/// One 4-byte button binding, decoded for display.
#[derive(Clone, Debug, Serialize)]
pub struct ButtonBinding {
    pub raw: [u8; 4],
    pub kind: &'static str,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Profile {
    /// 1-based user slot, or ROM slot when `rom` is true.
    pub slot: u8,
    pub sector: u16,
    pub rom: bool,
    pub enabled: bool,
    /// None for ROM profiles, which carry no CRC.
    pub crc_valid: Option<bool>,
    pub name: Option<String>,
    pub report_rate_hz: u32,
    /// Index into `dpi_levels` the profile starts on.
    pub default_dpi_index: u8,
    /// DPI-shift (sniper) index.
    pub shift_dpi_index: u8,
    pub dpi_levels: Vec<u16>,
    pub angle_snapping: u8,
    pub powersave_timeout: u16,
    pub poweroff_timeout: u16,
    pub buttons: Vec<ButtonBinding>,
    pub gshift_buttons: Vec<ButtonBinding>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub info: Info,
    pub directory_crc_valid: bool,
    /// Active on-board profile slot (1-based); 0 when none is active,
    /// e.g. while the mouse is in software (host) mode.
    pub active_profile: u8,
    pub active_dpi_index: Option<u8>,
    pub profiles: Vec<Profile>,
    pub rom_profiles: Vec<Profile>,
}

/// Raw sectors, for a byte-exact backup file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawSector {
    pub sector: u16,
    /// None for ROM sectors, which carry no CRC.
    pub crc_valid: Option<bool>,
    /// Hex-encoded sector bytes.
    pub hex: String,
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

const SPECIALS: [&str; 18] = [
    "invalid",
    "Wheel left",
    "Wheel right",
    "DPI up",
    "DPI down",
    "DPI cycle",
    "DPI default",
    "DPI shift",
    "Profile up",
    "Profile down",
    "Profile cycle",
    "G-Shift",
    "Battery indicator",
    "Enable profile",
    "Performance switch",
    "Host",
    "Scroll down",
    "Scroll up",
];

pub fn decode_binding(raw: [u8; 4]) -> ButtonBinding {
    let (kind, description) = match raw[0] {
        0x80 => match raw[1] {
            0x00 => ("none", "No action".to_string()),
            0x01 => {
                let mask = u16::from_be_bytes([raw[2], raw[3]]);
                let button = if mask == 0 { 0 } else { mask.trailing_zeros() + 1 };
                ("mouse", format!("Mouse button {button}"))
            }
            0x02 => ("keyboard", format!("Key 0x{:02x} (modifiers 0x{:02x})", raw[3], raw[2])),
            0x03 => ("consumer", format!("Media key 0x{:04x}", u16::from_be_bytes([raw[2], raw[3]]))),
            other => ("unknown", format!("HID subtype 0x{other:02x}")),
        },
        0x90 => {
            let name = SPECIALS.get(usize::from(raw[1])).copied().unwrap_or("invalid");
            ("special", name.to_string())
        }
        0x00 => ("macro", format!("Macro at page {} offset {}", raw[1], raw[3])),
        0xFF => ("disabled", "Disabled".to_string()),
        other => ("unknown", format!("Type 0x{other:02x}")),
    };
    ButtonBinding { raw, kind, description }
}

/// Profile name: all 0xFF means "use the default name". Names written as
/// UTF-16LE (every other byte zero) and as plain UTF-8 are both seen.
fn decode_name(raw: &[u8]) -> Option<String> {
    if raw.iter().all(|&b| b == 0xFF) {
        return None;
    }
    let utf16 = raw.chunks(2).take_while(|c| c != &[0, 0]).all(|c| c[1] == 0);
    let text = if utf16 {
        let units: Vec<u16> = raw
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0 && u != 0xFFFF)
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        let end = raw.iter().position(|&b| b == 0 || b == 0xFF).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    };
    Some(text.trim().to_string()).filter(|s| !s.is_empty())
}

fn parse_profile(slot: u8, sector: u16, rom: bool, enabled: bool, data: &[u8], button_count: usize) -> Profile {
    let le16 = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
    let binding = |i: usize| decode_binding([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    // Packed layout: rate, default/shift DPI index, 5 x DPI (LE), RGB color,
    // power mode, angle snapping, 10 reserved, powersave/poweroff timeouts,
    // then 16 buttons, 16 G-Shift buttons, a 48-byte name, LEDs, CRC.
    let buttons_at = BUTTONS_AT;
    let gshift_at = GSHIFT_AT;
    let name_at = gshift_at + 16 * 4;
    let name = decode_name(&data[name_at..name_at + 48]);
    let count = button_count.min(16);
    Profile {
        slot,
        sector,
        rom,
        enabled,
        crc_valid: (!rom).then(|| sector_crc_ok(data)),
        name,
        report_rate_hz: 1000 / u32::from(data[0].max(1)),
        default_dpi_index: data[1],
        shift_dpi_index: data[2],
        dpi_levels: (0..5).map(|i| le16(3 + 2 * i)).filter(|&d| d != 0 && d != 0xFFFF).collect(),
        angle_snapping: data[0x11],
        powersave_timeout: le16(0x1C),
        poweroff_timeout: le16(0x1E),
        buttons: (0..count).map(|i| binding(buttons_at + 4 * i)).collect(),
        gshift_buttons: (0..count).map(|i| binding(gshift_at + 4 * i)).collect(),
    }
}

fn parse_hex(hex: &str) -> std::result::Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 || !hex.is_ascii() {
        return Err("malformed hex in backup".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Sectors from a backup that may be written back: the directory and user
/// profile sectors whose stored CRC is valid. Factory (ROM) sectors are skipped.
pub fn restorable_sectors(raw: &[RawSector]) -> std::result::Result<Vec<(u16, Vec<u8>)>, String> {
    let mut out = Vec::new();
    for s in raw {
        if s.sector >= ROM_BASE || s.crc_valid != Some(true) {
            continue;
        }
        let data = parse_hex(&s.hex)?;
        if !sector_crc_ok(&data) {
            return Err(format!("backup sector 0x{:04x} fails its checksum", s.sector));
        }
        out.push((s.sector, data));
    }
    if out.is_empty() {
        return Err("the backup contains no restorable sectors".into());
    }
    Ok(out)
}

impl Device {
    pub fn onboard_info(&self, t: &Transport) -> Result<Info> {
        let r = self.call(t, ONBOARD_PROFILES, func::GET_INFO, &[])?;
        Ok(Info {
            memory_model: r[0],
            profile_format: r[1],
            macro_format: r[2],
            profile_count: r[3],
            rom_profile_count: r[4],
            button_count: r[5],
            sector_count: r[6],
            sector_size: u16::from_be_bytes([r[7], r[8]]),
            mechanical_layout: r[9],
            various_info: r[10],
        })
    }

    /// Reads a whole sector, 16 bytes at a time. The last read is aligned to
    /// `size - 16`, since the firmware rejects reads past the sector end.
    pub fn read_sector(&self, t: &Transport, sector: u16, size: u16) -> Result<Vec<u8>> {
        let size = usize::from(size);
        let mut data = vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            let at = if size - offset < 16 { size - 16 } else { offset };
            let [sh, sl] = sector.to_be_bytes();
            let [oh, ol] = (at as u16).to_be_bytes();
            let r = self.call(t, ONBOARD_PROFILES, func::READ, &[sh, sl, oh, ol])?;
            data[at..at + 16].copy_from_slice(&r[..16]);
            offset = at + 16;
        }
        Ok(data)
    }

    /// After switching to on-board mode the mouse can have no profile
    /// selected (it reports 0) and then runs nothing from memory. In that
    /// case this selects the first enabled user profile. Returns the active
    /// slot, or 0 when there is none to select.
    pub fn ensure_active_profile(&self, t: &Transport) -> Result<u8> {
        let active = self.call(t, ONBOARD_PROFILES, func::GET_ACTIVE_PROFILE, &[])?[1];
        if active != 0 {
            return Ok(active);
        }
        let info = self.onboard_info(t)?;
        let directory = self.read_sector(t, DIRECTORY_SECTOR, info.sector_size)?;
        if !sector_crc_ok(&directory) {
            return Ok(0);
        }
        for i in 0..usize::from(info.profile_count) {
            let addr = u16::from_be_bytes([directory[4 * i], directory[4 * i + 1]]);
            if addr == DIRECTORY_END {
                break;
            }
            if directory[4 * i + 2] != 0 {
                let slot = i as u8 + 1;
                self.call(t, ONBOARD_PROFILES, func::SET_ACTIVE_PROFILE, &[0, slot])?;
                return Ok(slot);
            }
        }
        Ok(0)
    }

    /// Reads and decodes the directory, user profiles and ROM profiles.
    pub fn onboard_snapshot(&self, t: &Transport) -> Result<(Snapshot, Vec<RawSector>)> {
        let info = self.onboard_info(t)?;
        let size = info.sector_size;
        let buttons = usize::from(info.button_count);
        let mut raw = Vec::new();

        let directory = self.read_sector(t, DIRECTORY_SECTOR, size)?;
        let directory_crc_valid = sector_crc_ok(&directory);
        raw.push(RawSector { sector: DIRECTORY_SECTOR, crc_valid: Some(directory_crc_valid), hex: hex(&directory) });

        let mut entries = Vec::new();
        if directory_crc_valid {
            for i in 0..usize::from(info.profile_count) {
                let addr = u16::from_be_bytes([directory[4 * i], directory[4 * i + 1]]);
                if addr == DIRECTORY_END {
                    break;
                }
                entries.push((i as u8 + 1, addr, directory[4 * i + 2] != 0));
            }
        }

        let mut profiles = Vec::new();
        for (slot, sector, enabled) in entries {
            let data = self.read_sector(t, sector, size)?;
            raw.push(RawSector { sector, crc_valid: Some(sector_crc_ok(&data)), hex: hex(&data) });
            profiles.push(parse_profile(slot, sector, false, enabled, &data, buttons));
        }

        let mut rom_profiles = Vec::new();
        for i in 1..=info.rom_profile_count {
            let sector = ROM_BASE + u16::from(i);
            let data = self.read_sector(t, sector, size)?;
            raw.push(RawSector { sector, crc_valid: None, hex: hex(&data) });
            rom_profiles.push(parse_profile(i, sector, true, true, &data, buttons));
        }

        let active = self.call(t, ONBOARD_PROFILES, func::GET_ACTIVE_PROFILE, &[])?;
        let active_profile = active[1];
        let active_dpi_index = self.call(t, ONBOARD_PROFILES, func::GET_ACTIVE_DPI_INDEX, &[]).ok().map(|r| r[0]);

        Ok((Snapshot { info, directory_crc_valid, active_profile, active_dpi_index, profiles, rom_profiles }, raw))
    }

    /// Writes one whole sector: start (sector, offset 0, length), 16-byte
    /// chunks, then end, which commits it to flash.
    fn write_sector(&self, t: &Transport, sector: u16, data: &[u8]) -> Result<()> {
        if sector >= ROM_BASE {
            return Err(Error::Other("refusing to write a factory (ROM) sector".into()));
        }
        let [sh, sl] = sector.to_be_bytes();
        let [ch, cl] = (data.len() as u16).to_be_bytes();
        self.call(t, ONBOARD_PROFILES, func::WRITE_START, &[sh, sl, 0, 0, ch, cl])?;
        for chunk in data.chunks(16) {
            let mut buf = [0xFFu8; 16];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.call(t, ONBOARD_PROFILES, func::WRITE_DATA, &buf)?;
        }
        self.call_with_timeout(t, ONBOARD_PROFILES, func::WRITE_END, &[], COMMIT_TIMEOUT)?;
        Ok(())
    }

    /// Writes `data` to `sector` and reads it back. On a mismatch the
    /// original bytes are written back and an error explains what happened.
    fn write_sector_verified(&self, t: &Transport, sector: u16, data: &[u8], original: &[u8]) -> Result<()> {
        let size = data.len() as u16;
        let outcome = self.write_sector(t, sector, data).and_then(|()| self.read_sector(t, sector, size));
        let reason = match outcome {
            Ok(readback) if readback == data => return Ok(()),
            Ok(_) => "the mouse stored different bytes than were written".to_string(),
            Err(e) => e.to_string(),
        };
        let rolled_back = self
            .write_sector(t, sector, original)
            .and_then(|()| self.read_sector(t, sector, size))
            .is_ok_and(|back| back == original);
        Err(Error::Other(if rolled_back {
            format!("write to sector 0x{sector:04x} failed ({reason}); the original was put back")
        } else {
            format!("write to sector 0x{sector:04x} failed ({reason}) and the original couldn't be confirmed; restore from the backup")
        }))
    }

    /// Changes one user slot's DPI levels, starting level, report rate and/or button bindings.
    pub fn write_slot_settings(&self, t: &Transport, settings: &SlotSettings) -> Result<WriteReport> {
        let info = self.onboard_info(t)?;
        if settings.slot == 0 || settings.slot > info.profile_count {
            return Err(Error::Other(format!("slot {} doesn't exist (1–{})", settings.slot, info.profile_count)));
        }
        if let Some(b) = settings.buttons.iter().find(|b| b.index >= info.button_count) {
            return Err(Error::Other(format!(
                "this mouse has {} buttons; button {} doesn't exist",
                info.button_count,
                u16::from(b.index) + 1
            )));
        }
        let directory = self.read_sector(t, DIRECTORY_SECTOR, info.sector_size)?;
        if !sector_crc_ok(&directory) {
            return Err(Error::Other("the profile directory fails its checksum; not writing".into()));
        }
        let i = usize::from(settings.slot - 1) * 4;
        let sector = u16::from_be_bytes([directory[i], directory[i + 1]]);
        if sector == DIRECTORY_END || sector == DIRECTORY_SECTOR || sector >= ROM_BASE {
            return Err(Error::Other(format!("slot {} has no user profile sector", settings.slot)));
        }
        let original = self.read_sector(t, sector, info.sector_size)?;
        if !sector_crc_ok(&original) {
            return Err(Error::Other(format!("slot {} fails its checksum; not writing", settings.slot)));
        }
        let patched = patch_profile_sector(&original, settings).map_err(Error::Other)?;
        let bytes_changed = patched.iter().zip(&original).filter(|(a, b)| a != b).count();
        if bytes_changed == 0 {
            return Ok(WriteReport { slot: settings.slot, sector, bytes_changed, unchanged: true });
        }
        self.write_sector_verified(t, sector, &patched, &original)?;
        // If the mouse is running this slot from memory, reload it.
        let active = self.call(t, ONBOARD_PROFILES, func::GET_ACTIVE_PROFILE, &[])?;
        if active[1] == settings.slot {
            let _ = self.call(t, ONBOARD_PROFILES, func::SET_ACTIVE_PROFILE, &[0, settings.slot]);
        }
        Ok(WriteReport { slot: settings.slot, sector, bytes_changed, unchanged: false })
    }

    /// Writes backed-up sectors (directory and user profiles only) back to
    /// the mouse, skipping those already identical. Returns how many changed.
    pub fn restore_sectors(&self, t: &Transport, sectors: &[(u16, Vec<u8>)]) -> Result<usize> {
        let info = self.onboard_info(t)?;
        if let Some((sector, data)) = sectors.iter().find(|(_, d)| d.len() != usize::from(info.sector_size)) {
            return Err(Error::Other(format!(
                "backup sector 0x{sector:04x} is {} bytes but this mouse uses {}",
                data.len(),
                info.sector_size
            )));
        }
        let mut written = 0;
        for (sector, data) in sectors {
            let current = self.read_sector(t, *sector, info.sector_size)?;
            if &current == data {
                continue;
            }
            self.write_sector_verified(t, *sector, data, &current)?;
            written += 1;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_ccitt_false_check_value() {
        // CRC-16/CCITT-FALSE (poly 0x1021, seed 0xFFFF) of "123456789" is 0x29B1.
        assert_eq!(crc_ccitt(b"123456789"), 0x29B1);
    }

    fn dpi_settings(levels: Vec<u16>, index: u8, hz: u32) -> SlotSettings {
        SlotSettings { slot: 1, dpi_levels: Some(levels), default_index: Some(index), report_rate_hz: Some(hz), buttons: Vec::new() }
    }

    fn button_settings(buttons: Vec<ButtonChange>) -> SlotSettings {
        SlotSettings { slot: 1, dpi_levels: None, default_index: None, report_rate_hz: None, buttons }
    }

    fn sample_sector() -> Vec<u8> {
        let mut s = vec![0xFFu8; 255];
        s[0] = 1;
        s[1] = 0;
        s[2] = 0;
        for (i, dpi) in [1500u16, 400, 800, 0, 0].iter().enumerate() {
            s[3 + 2 * i..5 + 2 * i].copy_from_slice(&dpi.to_le_bytes());
        }
        let crc = crc_ccitt(&s[..253]);
        s[253..].copy_from_slice(&crc.to_be_bytes());
        s
    }

    #[test]
    fn patch_changes_only_managed_bytes_and_fixes_crc() {
        let original = sample_sector();
        let settings = dpi_settings(vec![800, 1600, 3200], 1, 500);
        let patched = patch_profile_sector(&original, &settings).unwrap();
        assert!(sector_crc_ok(&patched));
        assert_eq!(patched[0], 2); // 500 Hz = 2 ms
        assert_eq!(patched[1], 1);
        assert_eq!(u16::from_le_bytes([patched[3], patched[4]]), 800);
        assert_eq!(u16::from_le_bytes([patched[7], patched[8]]), 3200);
        assert_eq!(u16::from_le_bytes([patched[9], patched[10]]), 0);
        // Everything after the DPI values (color, power, buttons, name) is untouched.
        assert_eq!(&patched[0x0D..253], &original[0x0D..253]);
    }

    #[test]
    fn patch_rejects_unstorable_values() {
        let original = sample_sector();
        let rate = dpi_settings(vec![800], 0, 8000);
        assert!(patch_profile_sector(&original, &rate).is_err());
        let index = dpi_settings(vec![800], 1, 1000);
        assert!(patch_profile_sector(&original, &index).is_err());
        let count = dpi_settings(vec![1; 6], 0, 1000);
        assert!(patch_profile_sector(&original, &count).is_err());
    }

    #[test]
    fn binding_encode_round_trips_through_decode() {
        let cases = [
            (BindingSpec::None, "none"),
            (BindingSpec::Disabled, "disabled"),
            (BindingSpec::Mouse { button: 4 }, "mouse"),
            (BindingSpec::Key { modifiers: 0x01, key: 0x06 }, "keyboard"),
            (BindingSpec::Key { modifiers: 0x09, key: 0 }, "keyboard"),
            (BindingSpec::Media { usage: 0x00CD }, "consumer"),
            (BindingSpec::Special { id: 5 }, "special"),
        ];
        for (spec, kind) in cases {
            assert_eq!(decode_binding(spec.encode().unwrap()).kind, kind, "{spec:?}");
        }
        assert_eq!(decode_binding(BindingSpec::Mouse { button: 5 }.encode().unwrap()).description, "Mouse button 5");
        assert_eq!(decode_binding(BindingSpec::Mouse { button: 16 }.encode().unwrap()).description, "Mouse button 16");
        assert_eq!(decode_binding(BindingSpec::Special { id: 5 }.encode().unwrap()).description, "DPI cycle");
    }

    #[test]
    fn encodes_ctrl_c_and_ctrl_v() {
        // Left Ctrl = modifier bit 0; HID usage C = 0x06, V = 0x19.
        assert_eq!(BindingSpec::Key { modifiers: 0x01, key: 0x06 }.encode().unwrap(), [0x80, 0x02, 0x01, 0x06]);
        assert_eq!(BindingSpec::Key { modifiers: 0x01, key: 0x19 }.encode().unwrap(), [0x80, 0x02, 0x01, 0x19]);
    }

    #[test]
    fn rejects_unstorable_bindings() {
        assert!(BindingSpec::Mouse { button: 0 }.encode().is_err());
        assert!(BindingSpec::Mouse { button: 17 }.encode().is_err());
        assert!(BindingSpec::Key { modifiers: 0, key: 0 }.encode().is_err());
        assert!(BindingSpec::Key { modifiers: 0, key: 0xE0 }.encode().is_err());
        assert!(BindingSpec::Media { usage: 0 }.encode().is_err());
        assert!(BindingSpec::Special { id: 0 }.encode().is_err());
        assert!(BindingSpec::Special { id: 18 }.encode().is_err());
    }

    #[test]
    fn button_patch_changes_only_that_binding_and_fixes_crc() {
        let original = sample_sector();
        let change = ButtonChange { index: 3, gshift: false, binding: BindingSpec::Key { modifiers: 0x01, key: 0x06 } };
        let patched = patch_profile_sector(&original, &button_settings(vec![change])).unwrap();
        assert!(sector_crc_ok(&patched));
        let at = BUTTONS_AT + 4 * 3;
        assert_eq!(&patched[at..at + 4], &[0x80, 0x02, 0x01, 0x06]);
        // Nothing else moved: DPI block, other buttons, G-Shift table, name.
        assert_eq!(&patched[..at], &original[..at]);
        assert_eq!(&patched[at + 4..253], &original[at + 4..253]);
    }

    #[test]
    fn gshift_patch_targets_the_gshift_table() {
        let original = sample_sector();
        let change = ButtonChange { index: 0, gshift: true, binding: BindingSpec::Media { usage: 0x00E2 } };
        let patched = patch_profile_sector(&original, &button_settings(vec![change])).unwrap();
        assert_eq!(&patched[GSHIFT_AT..GSHIFT_AT + 4], &[0x80, 0x03, 0x00, 0xE2]);
        assert_eq!(&patched[BUTTONS_AT..BUTTONS_AT + 4], &original[BUTTONS_AT..BUTTONS_AT + 4]);
    }

    #[test]
    fn button_patch_rejects_bad_index_and_partial_dpi_group() {
        let original = sample_sector();
        let change = ButtonChange { index: 16, gshift: false, binding: BindingSpec::None };
        assert!(patch_profile_sector(&original, &button_settings(vec![change])).is_err());
        let partial = SlotSettings { slot: 1, dpi_levels: Some(vec![800]), default_index: None, report_rate_hz: None, buttons: Vec::new() };
        assert!(patch_profile_sector(&original, &partial).is_err());
    }

    #[test]
    fn binding_spec_parses_from_the_front_end_json() {
        let spec: BindingSpec = serde_json::from_str(r#"{"type":"key","modifiers":1,"key":6}"#).unwrap();
        assert_eq!(spec, BindingSpec::Key { modifiers: 1, key: 6 });
        let change: ButtonChange = serde_json::from_str(r#"{"index":3,"binding":{"type":"none"}}"#).unwrap();
        assert!(!change.gshift);
    }

    #[test]
    fn restore_skips_rom_and_invalid_sectors() {
        let good = sample_sector();
        let hexed: String = good.iter().map(|b| format!("{b:02x}")).collect();
        let raw = vec![
            RawSector { sector: 0x0001, crc_valid: Some(true), hex: hexed.clone() },
            RawSector { sector: 0x0101, crc_valid: None, hex: hexed.clone() },
            RawSector { sector: 0x0002, crc_valid: Some(false), hex: hexed },
        ];
        let sectors = restorable_sectors(&raw).unwrap();
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0].0, 0x0001);
    }

    #[test]
    fn decodes_button_bindings() {
        assert_eq!(decode_binding([0x80, 0x01, 0x00, 0x01]).description, "Mouse button 1");
        assert_eq!(decode_binding([0x80, 0x01, 0x00, 0x10]).description, "Mouse button 5");
        assert_eq!(decode_binding([0x90, 0x05, 0x00, 0x00]).description, "DPI cycle");
        assert_eq!(decode_binding([0xFF, 0xFF, 0xFF, 0xFF]).kind, "disabled");
    }
}
