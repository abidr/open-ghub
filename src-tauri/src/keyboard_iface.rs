//! Windows-side check for keystroke bindings.
//!
//! A receiver (or a mouse on a cable) exposes a HID keyboard interface next
//! to its mouse interface. On-board bindings that send keys (Ctrl+C, media
//! keys excluded) arrive through it. If Windows has that keyboard device
//! disabled, every keystroke binding silently does nothing while mouse-button
//! bindings keep working. G HUB never needs the interface, because it types
//! shortcuts in software, so it can stay disabled unnoticed.

use serde::Serialize;

/// A keyboard device of this receiver/mouse that Windows isn't running.
#[derive(Clone, Debug, Serialize)]
pub struct BlockedKeyboard {
    pub instance_id: String,
    /// Configuration Manager problem code; 22 means disabled in Device Manager.
    pub problem_code: u32,
}

/// Keyboard devices with vendor 046D and this product id that have a
/// problem (disabled, failed to start). Empty when all is well, or when not
/// on Windows.
pub fn blocked_keyboards(product_id: u16) -> Vec<BlockedKeyboard> {
    blocked_keyboards_for(0x046D, product_id)
}

/// Same as [`blocked_keyboards`] for any vendor (diagnostics).
#[cfg(windows)]
pub fn blocked_keyboards_for(vendor_id: u16, product_id: u16) -> Vec<BlockedKeyboard> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_Status, CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW, CM_Locate_DevNodeW, CM_GETIDLIST_FILTER_CLASS,
        CM_GETIDLIST_FILTER_PRESENT, CM_LOCATE_DEVNODE_PHANTOM, CR_SUCCESS,
    };

    // GUID_DEVCLASS_KEYBOARD
    let class: Vec<u16> = "{4d36e96b-e325-11ce-bfc1-08002be10318}".encode_utf16().chain([0]).collect();
    let flags = CM_GETIDLIST_FILTER_CLASS | CM_GETIDLIST_FILTER_PRESENT;
    let mut len = 0u32;
    if unsafe { CM_Get_Device_ID_List_SizeW(&mut len, class.as_ptr(), flags) } != CR_SUCCESS || len == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u16; len as usize];
    if unsafe { CM_Get_Device_ID_ListW(class.as_ptr(), buf.as_mut_ptr(), len, flags) } != CR_SUCCESS {
        return Vec::new();
    }

    let needle = format!("VID_{vendor_id:04X}&PID_{product_id:04X}");
    let mut out = Vec::new();
    for id in buf.split(|&c| c == 0).filter(|s| !s.is_empty()) {
        let text = String::from_utf16_lossy(id);
        if !text.to_ascii_uppercase().contains(&needle) {
            continue;
        }
        let wide: Vec<u16> = id.iter().copied().chain([0]).collect();
        let mut devinst = 0u32;
        // PHANTOM also finds a devnode that isn't started, which is the case we're after.
        if unsafe { CM_Locate_DevNodeW(&mut devinst, wide.as_ptr(), CM_LOCATE_DEVNODE_PHANTOM) } != CR_SUCCESS {
            continue;
        }
        let (mut status, mut problem) = (0u32, 0u32);
        if unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, devinst, 0) } != CR_SUCCESS {
            continue;
        }
        if problem != 0 {
            out.push(BlockedKeyboard { instance_id: text, problem_code: problem });
        }
    }
    out
}

#[cfg(not(windows))]
pub fn blocked_keyboards_for(_vendor_id: u16, _product_id: u16) -> Vec<BlockedKeyboard> {
    Vec::new()
}
