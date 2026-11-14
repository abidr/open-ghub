//! HID++ 2.0 device model: feature discovery and the settings we expose.
//!
//! Function numbers follow the public HID++ 2.0 feature layouts, which match
//! the method names found in G HUB's agent (`Feature2201AdjustableDPI::SetSensorDPI`, ...).

use std::collections::BTreeMap;
use std::ffi::CString;
use std::time::Duration;

use serde::Serialize;

use crate::hidpp::{Error, Result, Transport};

pub mod feature {
    pub const ROOT: u16 = 0x0000;
    pub const FEATURE_SET: u16 = 0x0001;
    pub const DEVICE_INFO: u16 = 0x0003;
    pub const DEVICE_NAME: u16 = 0x0005;
    pub const BATTERY_STATUS: u16 = 0x1000;
    pub const BATTERY_VOLTAGE: u16 = 0x1001;
    pub const UNIFIED_BATTERY: u16 = 0x1004;
    pub const ADJUSTABLE_DPI: u16 = 0x2201;
    pub const EXTENDED_DPI: u16 = 0x2202;
    pub const REPORT_RATE: u16 = 0x8060;
    pub const EXTENDED_REPORT_RATE: u16 = 0x8061;
    pub const ONBOARD_PROFILES: u16 = 0x8100;
    pub const MOUSE_BUTTON_SPY: u16 = 0x8110;
}
use feature::*;

/// The 16-byte 0x8110 remapping table that leaves `count` buttons alone.
pub fn identity_remap(count: usize) -> [u8; 16] {
    let mut table = [0u8; 16];
    for (i, slot) in table.iter_mut().enumerate().take(count.min(16)) {
        *slot = i as u8 + 1;
    }
    table
}

const TIMEOUT: Duration = Duration::from_millis(1500);
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// Sensor index used for every DPI call; gaming mice have a single sensor.
const SENSOR: u8 = 0;

pub fn feature_name(id: u16) -> &'static str {
    match id {
        0x0000 => "Root",
        0x0001 => "Feature set",
        0x0002 => "Feature info",
        0x0003 => "Firmware info",
        0x0005 => "Device name & type",
        0x0007 => "Friendly name",
        0x0020 => "Config change",
        0x0021 => "Unique identifier",
        0x00C2 => "DFU control",
        0x00D0 => "DFU",
        0x1000 => "Battery status",
        0x1001 => "Battery voltage",
        0x1004 => "Unified battery",
        0x1802 => "Device reset",
        0x1806 => "Device properties",
        0x1814 => "Change host",
        0x1815 => "Hosts info",
        0x1B04 => "Special keys & buttons",
        0x1D4B => "Wireless status",
        0x1E00 => "Enable hidden features",
        0x1F20 => "ADC measurement",
        0x2110 => "SmartShift",
        0x2121 => "Hi-res wheel",
        0x2150 => "Thumbwheel",
        0x2201 => "Adjustable DPI",
        0x2202 => "Extended adjustable DPI",
        0x2205 => "Pointer motion scaling",
        0x2230 => "Angle snapping",
        0x2240 => "Surface tuning",
        0x2400 => "Hybrid tracking",
        0x8060 => "Report rate",
        0x8061 => "Extended report rate",
        0x8070 => "Color LED effects",
        0x8071 => "RGB effects",
        0x8080 | 0x8081 => "Per-key lighting",
        0x8090 => "Mode status",
        0x8100 => "Onboard profiles",
        0x8110 => "Mouse button spy",
        0x8111 => "Latency monitoring",
        _ => "",
    }
}

fn device_kind(code: u8) -> &'static str {
    match code {
        0 => "Keyboard",
        1 => "Remote control",
        2 => "Numpad",
        3 => "Mouse",
        4 => "Trackpad",
        5 => "Trackball",
        6 => "Presenter",
        7 => "Receiver",
        8 => "Headset",
        9 => "Webcam",
        10 => "Steering wheel",
        11 => "Joystick",
        12 => "Gamepad",
        13 => "Dock",
        14 => "Speaker",
        15 => "Microphone",
        _ => "Device",
    }
}

fn be16(hi: u8, lo: u8) -> u16 {
    u16::from_be_bytes([hi, lo])
}

#[derive(Clone, Serialize)]
pub struct FeatureInfo {
    pub id: u16,
    pub index: u8,
    pub version: u8,
    pub flags: u8,
    pub name: &'static str,
}

#[derive(Clone)]
pub struct Device {
    pub id: String,
    pub path: CString,
    pub index: u8,
    pub product_id: u16,
    pub product: String,
    /// Display name: the marketing name from G HUB's device database when
    /// available, otherwise the name the firmware reports.
    pub name: String,
    /// Name reported by the firmware (feature 0x0005), e.g. "PRO X Wireless".
    pub firmware_name: String,
    /// Product IDs per transport from feature 0x0003, e.g. 0x4093 over Lightspeed.
    pub model_ids: Vec<u16>,
    pub kind: &'static str,
    pub protocol: String,
    pub features: BTreeMap<u16, FeatureInfo>,
}

#[derive(Serialize)]
pub struct Battery {
    pub percent: Option<u8>,
    pub level: Option<&'static str>,
    pub status: &'static str,
    pub voltage_mv: Option<u16>,
}

#[derive(Serialize)]
pub struct Dpi {
    pub feature: u16,
    pub x: u16,
    pub y: Option<u16>,
    pub default: Option<u16>,
    pub supported: Vec<u16>,
    pub supports_y: bool,
    pub lod: Option<u8>,
    pub supports_lod: bool,
}

#[derive(Serialize)]
pub struct Choice {
    pub value: u8,
    pub label: String,
    pub hz: u32,
}

#[derive(Serialize)]
pub struct ReportRate {
    pub feature: u16,
    pub current: u8,
    pub choices: Vec<Choice>,
}

#[derive(Serialize)]
pub struct DeviceState {
    pub battery: Option<Battery>,
    pub dpi: Option<Dpi>,
    pub report_rate: Option<ReportRate>,
    /// "onboard" or "host"
    pub onboard_mode: Option<&'static str>,
    pub errors: Vec<String>,
}

/// Decodes a HID++ DPI list: big-endian u16 values terminated by 0, where a
/// value of the form `0b111x_xxxx_xxxx_xxxx` is a step size between the
/// previous value and the value that follows it.
pub fn parse_dpi_list(bytes: &[u8]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let value = be16(bytes[i], bytes[i + 1]);
        if value == 0 {
            break;
        }
        if value >> 13 == 0b111 {
            let step = u32::from(value & 0x1FFF);
            if i + 3 >= bytes.len() {
                break;
            }
            let last = u32::from(be16(bytes[i + 2], bytes[i + 3]));
            if let (Some(&prev), true) = (out.last(), step > 0) {
                let mut dpi = u32::from(prev) + step;
                while dpi <= last {
                    out.push(dpi as u16);
                    dpi += step;
                }
            }
            i += 4;
        } else {
            out.push(value);
            i += 2;
        }
    }
    out
}

/// Parses 0x0003 getDeviceInfo: entity count, unit id (4), transport bits
/// (2, low byte used), then one big-endian product id per supported
/// transport in the order Bluetooth, BLE, Lightspeed/eQuad, USB.
pub fn parse_model_ids(r: &[u8]) -> Vec<u16> {
    if r.len() < 13 {
        return Vec::new();
    }
    let transports = r[6];
    let mut ids = Vec::new();
    let mut offset = 7;
    for flag in [0x01u8, 0x02, 0x04, 0x08] {
        if transports & flag != 0 && offset + 1 < 13 {
            let pid = be16(r[offset], r[offset + 1]);
            if pid != 0 {
                ids.push(pid);
            }
            offset += 2;
        }
    }
    ids
}

impl Device {
    /// Pings `index` on `transport`, then discovers the device's features.
    pub fn probe(
        transport: &Transport,
        id: String,
        path: CString,
        index: u8,
        product_id: u16,
        product: String,
    ) -> Result<Device> {
        // Root function 1: getProtocolVersion, with a ping byte echoed back.
        let ping = transport.request(index, 0, 1, &[0, 0, 0xA5], PROBE_TIMEOUT)?;
        if ping[0] < 2 {
            return Err(Error::Other("device only speaks HID++ 1.0".into()));
        }

        let mut device = Device {
            id,
            path,
            index,
            product_id,
            name: product.clone(),
            firmware_name: String::new(),
            model_ids: Vec::new(),
            product,
            kind: "Device",
            protocol: format!("{}.{}", ping[0], ping[1]),
            features: BTreeMap::new(),
        };
        device.features.insert(
            ROOT,
            FeatureInfo { id: ROOT, index: 0, version: 0, flags: 0, name: feature_name(ROOT) },
        );
        device.discover_features(transport)?;

        if device.has(DEVICE_NAME) {
            if let Ok(name) = device.read_name(transport) {
                if !name.is_empty() {
                    device.firmware_name = name.clone();
                    device.name = name;
                }
            }
            if let Ok(r) = device.call(transport, DEVICE_NAME, 2, &[]) {
                device.kind = device_kind(r[0]);
            }
        }
        if device.has(DEVICE_INFO) {
            if let Ok(r) = device.call(transport, DEVICE_INFO, 0, &[]) {
                device.model_ids = parse_model_ids(&r);
            }
        }
        // Wired devices also identify by their USB product id.
        let mut candidates = device.model_ids.clone();
        if index == crate::hidpp::DIRECT {
            candidates.push(product_id);
        }
        if let Some(name) = candidates.iter().find_map(|pid| crate::device_db::lookup(*pid).map(|(name, _)| name.to_string())) {
            device.name = name;
        }
        Ok(device)
    }

    fn discover_features(&mut self, t: &Transport) -> Result<()> {
        // Root function 0: getFeature(featureId) -> index, type, version.
        let r = t.request(self.index, 0, 0, &FEATURE_SET.to_be_bytes(), TIMEOUT)?;
        let set_index = r[0];
        if set_index == 0 {
            return Ok(());
        }
        let count = t.request(self.index, set_index, 0, &[], TIMEOUT)?[0];
        for i in 1..=count {
            let r = t.request(self.index, set_index, 1, &[i], TIMEOUT)?;
            let id = be16(r[0], r[1]);
            self.features.insert(
                id,
                FeatureInfo { id, index: i, flags: r[2], version: r[3], name: feature_name(id) },
            );
        }
        Ok(())
    }

    fn read_name(&self, t: &Transport) -> Result<String> {
        let len = usize::from(self.call(t, DEVICE_NAME, 0, &[])?[0]);
        let mut bytes = Vec::with_capacity(len);
        while bytes.len() < len {
            let chunk = self.call(t, DEVICE_NAME, 1, &[bytes.len() as u8])?;
            let take = (len - bytes.len()).min(chunk.len());
            bytes.extend_from_slice(&chunk[..take]);
        }
        let name = String::from_utf8_lossy(&bytes);
        Ok(name.trim_end_matches('\0').trim().to_string())
    }

    pub fn has(&self, feature: u16) -> bool {
        self.features.contains_key(&feature)
    }

    pub(crate) fn call(&self, t: &Transport, feature: u16, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        self.call_with_timeout(t, feature, function, params, TIMEOUT)
    }

    /// For slow operations such as committing a flash write.
    pub(crate) fn call_with_timeout(
        &self,
        t: &Transport,
        feature: u16,
        function: u8,
        params: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let info = self.features.get(&feature).ok_or(Error::Unsupported(feature))?;
        t.request(self.index, info.index, function, params, timeout)
    }

    pub fn ping(&self, t: &Transport) -> bool {
        t.request(self.index, 0, 1, &[0, 0, 0xA5], PROBE_TIMEOUT).is_ok()
    }

    pub fn state(&self, t: &Transport) -> DeviceState {
        fn keep<T>(errors: &mut Vec<String>, label: &str, r: Result<Option<T>>) -> Option<T> {
            r.unwrap_or_else(|e| {
                errors.push(format!("{label}: {e}"));
                None
            })
        }
        let mut errors = Vec::new();
        let battery = keep(&mut errors, "Battery", self.battery(t));
        let dpi = keep(&mut errors, "DPI", self.dpi(t));
        let report_rate = keep(&mut errors, "Report rate", self.report_rate(t));
        let onboard_mode = keep(&mut errors, "Onboard mode", self.onboard_mode(t));
        DeviceState { battery, dpi, report_rate, onboard_mode, errors }
    }

    pub fn battery(&self, t: &Transport) -> Result<Option<Battery>> {
        if self.has(UNIFIED_BATTERY) {
            let caps = self.call(t, UNIFIED_BATTERY, 0, &[])?;
            let r = self.call(t, UNIFIED_BATTERY, 1, &[])?;
            let level = match r[1] {
                l if l & 0x08 != 0 => "full",
                l if l & 0x04 != 0 => "good",
                l if l & 0x02 != 0 => "low",
                l if l & 0x01 != 0 => "critical",
                _ => "unknown",
            };
            let status = match r[2] {
                0 => "discharging",
                1 => "charging",
                2 => "charging slowly",
                3 => "full",
                _ => "error",
            };
            let has_soc = caps[1] & 0x02 != 0;
            return Ok(Some(Battery {
                percent: has_soc.then_some(r[0]),
                level: Some(level),
                status,
                voltage_mv: None,
            }));
        }
        if self.has(BATTERY_STATUS) {
            let r = self.call(t, BATTERY_STATUS, 0, &[])?;
            let status = match r[2] {
                0 => "discharging",
                1 => "charging",
                2 => "almost full",
                3 => "full",
                4 => "charging slowly",
                _ => "error",
            };
            return Ok(Some(Battery {
                percent: (r[0] > 0).then_some(r[0]),
                level: None,
                status,
                voltage_mv: None,
            }));
        }
        if self.has(BATTERY_VOLTAGE) {
            let r = self.call(t, BATTERY_VOLTAGE, 0, &[])?;
            let status = if r[2] & 0x80 != 0 { "charging" } else { "discharging" };
            return Ok(Some(Battery {
                percent: None,
                level: None,
                status,
                voltage_mv: Some(be16(r[0], r[1])),
            }));
        }
        Ok(None)
    }

    pub fn dpi(&self, t: &Transport) -> Result<Option<Dpi>> {
        if self.has(EXTENDED_DPI) {
            // fn1 getSensorCapabilities: byte 2 bit 0 = separate Y, bit 1 = lift-off distance.
            let caps = self.call(t, EXTENDED_DPI, 1, &[SENSOR])?;
            let supports_y = caps[2] & 0x01 != 0;
            let supports_lod = caps[2] & 0x02 != 0;
            let supported = self.extended_dpi_list(t)?;
            // fn5 getSensorDpiParameters: sensor, x, default x, y, default y, lod.
            let p = self.call(t, EXTENDED_DPI, 5, &[SENSOR])?;
            return Ok(Some(Dpi {
                feature: EXTENDED_DPI,
                x: be16(p[1], p[2]),
                default: Some(be16(p[3], p[4])).filter(|d| *d > 0),
                y: supports_y.then(|| be16(p[5], p[6])),
                supported,
                supports_y,
                lod: supports_lod.then_some(p[9]),
                supports_lod,
            }));
        }
        if self.has(ADJUSTABLE_DPI) {
            // fn1 getSensorDpiList: sensor, then the encoded list.
            let list = self.call(t, ADJUSTABLE_DPI, 1, &[SENSOR])?;
            // fn2 getSensorDpi: sensor, current dpi, default dpi.
            let p = self.call(t, ADJUSTABLE_DPI, 2, &[SENSOR])?;
            return Ok(Some(Dpi {
                feature: ADJUSTABLE_DPI,
                x: be16(p[1], p[2]),
                default: Some(be16(p[3], p[4])).filter(|d| *d > 0),
                y: None,
                supported: parse_dpi_list(&list[1..]),
                supports_y: false,
                lod: None,
                supports_lod: false,
            }));
        }
        Ok(None)
    }

    /// 0x2202 fn2 getSensorDpiRanges(sensor, direction, page): the list can
    /// span several replies, each prefixed by sensor/direction/page.
    fn extended_dpi_list(&self, t: &Transport) -> Result<Vec<u16>> {
        let mut bytes = Vec::new();
        for page in 0..32u8 {
            let r = self.call(t, EXTENDED_DPI, 2, &[SENSOR, 0, page])?;
            bytes.extend_from_slice(&r[3..]);
            if bytes.ends_with(&[0, 0]) {
                break;
            }
        }
        Ok(parse_dpi_list(&bytes))
    }

    pub fn set_dpi(&self, t: &Transport, x: u16, y: Option<u16>, lod: Option<u8>) -> Result<()> {
        if self.has(EXTENDED_DPI) {
            let current = self.call(t, EXTENDED_DPI, 5, &[SENSOR])?;
            let y = y.unwrap_or(x);
            let lod = lod.unwrap_or(current[9]);
            let [xh, xl] = x.to_be_bytes();
            let [yh, yl] = y.to_be_bytes();
            // fn6 setSensorDpiParameters: sensor, x, y, lod.
            self.call(t, EXTENDED_DPI, 6, &[SENSOR, xh, xl, yh, yl, lod])?;
            return Ok(());
        }
        if self.has(ADJUSTABLE_DPI) {
            let [xh, xl] = x.to_be_bytes();
            // fn3 setSensorDpi: sensor, dpi.
            self.call(t, ADJUSTABLE_DPI, 3, &[SENSOR, xh, xl])?;
            return Ok(());
        }
        Err(Error::Unsupported(ADJUSTABLE_DPI))
    }

    pub fn report_rate(&self, t: &Transport) -> Result<Option<ReportRate>> {
        if self.has(EXTENDED_REPORT_RATE) {
            const HZ: [u32; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];
            // fn1 getActualReportRateList -> bitmask of rate indices.
            let r = self.call(t, EXTENDED_REPORT_RATE, 1, &[])?;
            let mask = be16(r[0], r[1]);
            let choices = (0..HZ.len() as u8)
                .filter(|i| mask >> i & 1 == 1)
                .map(|i| {
                    let hz = HZ[usize::from(i)];
                    Choice { value: i, label: format!("{hz} Hz"), hz }
                })
                .collect();
            // fn2 getReportRate -> rate index.
            let current = self.call(t, EXTENDED_REPORT_RATE, 2, &[])?[0];
            return Ok(Some(ReportRate { feature: EXTENDED_REPORT_RATE, current, choices }));
        }
        if self.has(REPORT_RATE) {
            // fn0 getReportRateList -> bit n set means (n + 1) ms is supported.
            let mask = self.call(t, REPORT_RATE, 0, &[])?[0];
            let choices = (0..8u8)
                .filter(|i| mask >> i & 1 == 1)
                .map(|i| {
                    let ms = i + 1;
                    let hz = 1000 / u32::from(ms);
                    Choice { value: ms, label: format!("{hz} Hz"), hz }
                })
                .collect();
            // fn1 getReportRate -> period in ms.
            let current = self.call(t, REPORT_RATE, 1, &[])?[0];
            return Ok(Some(ReportRate { feature: REPORT_RATE, current, choices }));
        }
        Ok(None)
    }

    pub fn set_report_rate(&self, t: &Transport, value: u8) -> Result<()> {
        if self.has(EXTENDED_REPORT_RATE) {
            self.call(t, EXTENDED_REPORT_RATE, 3, &[value])?;
            return Ok(());
        }
        if self.has(REPORT_RATE) {
            self.call(t, REPORT_RATE, 2, &[value])?;
            return Ok(());
        }
        Err(Error::Unsupported(REPORT_RATE))
    }

    pub fn onboard_mode(&self, t: &Transport) -> Result<Option<&'static str>> {
        if !self.has(ONBOARD_PROFILES) {
            return Ok(None);
        }
        // fn2 getOnboardMode: 1 = onboard memory, 2 = host (software) control.
        let mode = self.call(t, ONBOARD_PROFILES, 2, &[])?[0];
        Ok(Some(if mode == 2 { "host" } else { "onboard" }))
    }

    pub fn set_onboard_mode(&self, t: &Transport, host: bool) -> Result<()> {
        // fn1 setOnboardMode.
        self.call(t, ONBOARD_PROFILES, 1, &[if host { 2 } else { 1 }])?;
        Ok(())
    }

    /// The 0x8110 Mouse Button Spy remapping table: entry i is the button
    /// that physical button i+1 reports as, 0 meaning "nothing". None when
    /// the mouse lacks the feature.
    pub fn button_remap(&self, t: &Transport) -> Result<Option<Vec<u8>>> {
        if !self.has(MOUSE_BUTTON_SPY) {
            return Ok(None);
        }
        let count = usize::from(self.call(t, MOUSE_BUTTON_SPY, 0, &[])?[0]).min(16);
        let table = self.call(t, MOUSE_BUTTON_SPY, 3, &[])?;
        Ok(Some(table[..count].to_vec()))
    }

    /// Undoes what G HUB leaves behind through 0x8110 Mouse Button Spy. In
    /// software mode G HUB remaps the buttons it handles to "nothing" and
    /// listens for them itself, and that table stays in the mouse after G HUB
    /// quits. Those buttons then do nothing, even in on-board mode, where the
    /// stored bindings never see them. This stops spying and restores the
    /// identity table. Returns true when the table had to change.
    pub fn reset_button_spy(&self, t: &Transport) -> Result<bool> {
        let Some(current) = self.button_remap(t)? else {
            return Ok(false);
        };
        self.call(t, MOUSE_BUTTON_SPY, 2, &[])?; // stopSpy
        let identity = identity_remap(current.len());
        if current == identity[..current.len()] {
            return Ok(false);
        }
        self.call(t, MOUSE_BUTTON_SPY, 4, &identity)?; // setRemapping
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_dpi_list;

    #[test]
    fn parses_model_ids_per_transport() {
        // entity count, unit id, transport bits = Lightspeed | USB, pids 0x4093 and 0xC094.
        let r = [1, 0xAA, 0xBB, 0xCC, 0xDD, 0x00, 0x0C, 0x40, 0x93, 0xC0, 0x94, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(super::parse_model_ids(&r), vec![0x4093, 0xC094]);
    }

    #[test]
    fn parses_discrete_values() {
        let bytes = [0x01, 0x90, 0x03, 0x20, 0x06, 0x40, 0x00, 0x00];
        assert_eq!(parse_dpi_list(&bytes), vec![400, 800, 1600]);
    }

    #[test]
    fn identity_remap_maps_each_button_to_itself() {
        let t = super::identity_remap(11);
        assert_eq!(&t[..11], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        assert!(t[11..].iter().all(|&b| b == 0));
        // What G HUB left on a G502 LIGHTSPEED: buttons 4-9 report nothing.
        let ghub = [1u8, 2, 3, 0, 0, 0, 0, 0, 0, 3, 3];
        assert_ne!(&ghub[..], &t[..11]);
    }

    #[test]
    fn expands_step_ranges() {
        // 100, step 50 up to 300, then 400.
        let bytes = [0x00, 0x64, 0xE0, 0x32, 0x01, 0x2C, 0x01, 0x90, 0x00, 0x00];
        assert_eq!(parse_dpi_list(&bytes), vec![100, 150, 200, 250, 300, 400]);
    }
}
