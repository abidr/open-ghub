#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod device;
mod device_db;
mod events;
mod foreground;
mod hidpp;
mod keyboard_iface;
mod manager;
mod onboard;
mod profiles;
mod settings;
mod tray;
mod updater;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager as _, State};

use device::{DeviceState, FeatureInfo};
use manager::Manager;
use profiles::{ImportSummary, Profile, ProfileStore};

/// The manager is created lazily so a HID init failure is reported to the UI
/// instead of aborting startup.
struct AppState(Mutex<Option<Manager>>);

fn manager<'a>(state: &'a State<AppState>) -> Result<MutexGuard<'a, Option<Manager>>, String> {
    let mut guard = state.0.lock().map_err(|_| "device manager lock poisoned".to_string())?;
    if guard.is_none() {
        *guard = Some(Manager::new(events::sink()).map_err(|e| e.to_string())?);
    }
    Ok(guard)
}

#[derive(Serialize)]
struct DeviceSummary {
    id: String,
    name: String,
    product: String,
    product_id: String,
    kind: &'static str,
    firmware_name: String,
    model_ids: Vec<String>,
    connection: String,
    protocol: String,
    features: Vec<FeatureInfo>,
}

#[derive(Serialize)]
struct ScanResult {
    devices: Vec<DeviceSummary>,
    warnings: Vec<String>,
}

#[tauri::command(async)]
fn scan_devices(app: AppHandle, state: State<AppState>) -> Result<ScanResult, String> {
    let mut guard = manager(&state)?;
    let warnings = guard.as_mut().expect("manager initialised").scan().map_err(|e| e.to_string())?;
    drop(guard);
    apply_to_new_devices(&app);
    let guard = manager(&state)?;
    Ok(ScanResult { devices: summaries(guard.as_ref().expect("manager initialised")), warnings })
}

/// Current devices without rescanning (the event loop keeps them current).
#[tauri::command(async)]
fn list_devices(state: State<AppState>) -> Result<Vec<DeviceSummary>, String> {
    let guard = manager(&state)?;
    Ok(summaries(guard.as_ref().expect("manager initialised")))
}

fn summaries(manager: &Manager) -> Vec<DeviceSummary> {
    manager
        .devices()
        .iter()
        .map(|d| DeviceSummary {
            id: d.id.clone(),
            name: d.name.clone(),
            product: d.product.clone(),
            product_id: format!("{:04X}", d.product_id),
            kind: d.kind,
            firmware_name: d.firmware_name.clone(),
            model_ids: d.model_ids.iter().map(|id| format!("{id:04X}")).collect(),
            connection: if d.index == hidpp::DIRECT {
                "Direct (USB)".into()
            } else {
                format!("{} · slot {}", d.product, d.index)
            },
            protocol: d.protocol.clone(),
            features: d.features.values().cloned().collect(),
        })
        .collect()
}

#[tauri::command(async)]
fn device_state(state: State<AppState>, id: String) -> Result<DeviceState, String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| Ok(d.state(t))).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_dpi(state: State<AppState>, id: String, x: u16, y: Option<u16>, lod: Option<u8>) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_dpi(t, x, y, lod)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_report_rate(state: State<AppState>, id: String, value: u8) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_report_rate(t, value)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_onboard_mode(state: State<AppState>, id: String, host: bool) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager
        .with_device(&id, |d, t| {
            d.set_onboard_mode(t, host)?;
            if !host {
                // Undo G HUB's button capture so the stored bindings see the
                // buttons, and make sure the mouse has a stored profile to run.
                // G HUB would put its table straight back, so leave it alone then.
                if !ghub_agent_running() {
                    if let Err(e) = d.reset_button_spy(t) {
                        eprintln!("could not reset the button remap table: {e}");
                    }
                }
                let _ = d.ensure_active_profile(t);
            }
            Ok(())
        })
        .map_err(|e| e.to_string())
}

// ---------- profiles ----------

struct ProfilesState {
    store: Mutex<ProfileStore>,
    path: PathBuf,
}

fn lock_store<'a>(state: &'a ProfilesState) -> Result<MutexGuard<'a, ProfileStore>, String> {
    state.store.lock().map_err(|_| "profile store lock poisoned".to_string())
}

#[derive(Serialize)]
struct ProfilesView {
    profiles: Vec<Profile>,
    auto_switch: bool,
    active_id: String,
}

fn view(store: &ProfileStore) -> ProfilesView {
    ProfilesView { profiles: store.profiles.clone(), auto_switch: store.auto_switch, active_id: store.active_id.clone() }
}

/// Mutates the store, persists it, and returns the new view.
fn update_store(state: &ProfilesState, f: impl FnOnce(&mut ProfileStore)) -> Result<ProfilesView, String> {
    let mut store = lock_store(state)?;
    f(&mut store);
    store.save(&state.path).map_err(|e| format!("could not save profiles: {e}"))?;
    Ok(view(&store))
}

#[derive(Clone, Serialize)]
struct Activation {
    id: String,
    name: String,
    /// "manual" or "auto"
    reason: &'static str,
    errors: Vec<String>,
}

/// Makes `id` the active profile and applies it to every connected mouse.
/// Lock order is always profiles, then devices.
fn activate(app: &AppHandle, id: &str, reason: &'static str) -> Result<Activation, String> {
    let profiles = app.state::<ProfilesState>();
    let profile = {
        let mut store = lock_store(&profiles)?;
        let profile = store.get(id).cloned().ok_or_else(|| "profile not found".to_string())?;
        store.active_id = profile.id.clone();
        store.save(&profiles.path).map_err(|e| format!("could not save profiles: {e}"))?;
        profile
    };

    let devices = app.state::<AppState>();
    let mut guard = manager(&devices)?;
    let manager = guard.as_mut().expect("manager initialised");
    if manager.devices().is_empty() {
        manager.scan().map_err(|e| e.to_string())?;
    }
    let mut errors = Vec::new();
    for d in manager.devices() {
        match manager.with_device(&d.id, |d, t| Ok(profiles::apply_to_device(&profile, d, t))) {
            Ok(errs) => errors.extend(errs),
            Err(e) => errors.push(format!("{}: {e}", d.name)),
        }
    }
    if let Ok(mut applied) = app.state::<AppliedDevices>().0.lock() {
        applied.extend(manager.devices().iter().map(|d| d.id.clone()));
    }
    drop(guard);

    let activation = Activation { id: profile.id, name: profile.name, reason, errors };
    tray::refresh(app);
    let _ = app.emit("profile-activated", activation.clone());
    Ok(activation)
}

/// Devices the active profile has been applied to since they connected.
/// A device that disconnects drops out, so it gets the profile again on return.
#[derive(Default)]
struct AppliedDevices(Mutex<HashSet<String>>);

/// Applies the active profile to devices that connected (or reconnected)
/// since it was last applied: after power-cycling the mouse, waking it, or
/// starting Open GHub. Does nothing until the user has saved settings, so a
/// first launch never changes the mouse unasked.
/// Lock order: profiles, then devices, then the applied set.
fn apply_to_new_devices(app: &AppHandle) {
    let profiles = app.state::<ProfilesState>();
    if !profiles.path.exists() {
        return;
    }
    let profile = match lock_store(&profiles) {
        Ok(store) => match store.get(&store.active_id) {
            Some(p) => p.clone(),
            None => return,
        },
        Err(_) => return,
    };

    let devices = app.state::<AppState>();
    let Ok(guard) = manager(&devices) else { return };
    let Some(manager) = guard.as_ref() else { return };
    let present: Vec<String> = manager.devices().iter().map(|d| d.id.clone()).collect();
    let applied_state = app.state::<AppliedDevices>();
    let Ok(mut applied) = applied_state.0.lock() else { return };
    applied.retain(|id| present.contains(id));
    let new: Vec<String> = present.into_iter().filter(|id| !applied.contains(id)).collect();
    for id in &new {
        match manager.with_device(id, |d, t| Ok(profiles::apply_to_device(&profile, d, t))) {
            Ok(errors) => errors.iter().for_each(|e| eprintln!("reapply {}: {e}", profile.name)),
            Err(e) => eprintln!("reapply {}: {e}", profile.name),
        }
        if events::logging() {
            eprintln!("[event] applied profile '{}' to {id}", profile.name);
        }
        applied.insert(id.clone());
    }
    drop(applied);
    drop(guard);
    for id in new {
        let _ = app.emit("device-changed", id);
    }
}

/// Switches profiles when another application comes to the foreground,
/// like G HUB's process watcher. Driven by a Windows event hook, not polling.
fn spawn_profile_watcher(app: AppHandle) {
    let own_exe = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
    let last_exe = Mutex::new(String::new());
    foreground::watch(move || {
        let Some(exe) = foreground::foreground_exe() else { return };
        if events::logging() {
            eprintln!("[event] foreground: {exe}");
        }
        {
            let Ok(mut last) = last_exe.lock() else { return };
            if *last == exe {
                return;
            }
            *last = exe.clone();
        }
        if own_exe.as_deref() == Some(exe.to_lowercase().as_str()) {
            return;
        }
        let target = {
            let state = app.state::<ProfilesState>();
            let Ok(store) = lock_store(&state) else { return };
            if !store.auto_switch {
                return;
            }
            let profile = store.match_exe(&exe);
            if profile.id == store.active_id {
                return;
            }
            profile.id.clone()
        };
        if let Err(e) = activate(&app, &target, "auto") {
            eprintln!("auto-switch failed: {e}");
        }
    });
}

/// Reads the mouse's on-board profile memory. Read-only.
#[tauri::command(async)]
fn onboard_snapshot(state: State<AppState>, id: String) -> Result<onboard::Snapshot, String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.onboard_snapshot(t).map(|(s, _)| s)).map_err(|e| e.to_string())
}

/// Keyboard devices of this receiver/mouse that Windows has disabled or failed to start.
#[tauri::command(async)]
fn blocked_keyboards(product_id: String) -> Result<Vec<keyboard_iface::BlockedKeyboard>, String> {
    let pid = u16::from_str_radix(&product_id, 16).map_err(|_| format!("bad product id {product_id}"))?;
    Ok(keyboard_iface::blocked_keyboards(pid))
}

/// Writes a backup file: decoded snapshot plus raw sectors (hex).
fn write_onboard_backup(
    app_version: &str,
    path: &std::path::Path,
    device: &device::Device,
    snapshot: &onboard::Snapshot,
    raw: &[onboard::RawSector],
) -> Result<(), String> {
    let backup = serde_json::json!({
        "format": "open-ghub-onboard-backup/1",
        "app_version": app_version,
        "created": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "device": device.name,
        "model_ids": device.model_ids.iter().map(|id| format!("{id:04X}")).collect::<Vec<_>>(),
        "snapshot": snapshot,
        "sectors": raw,
    });
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Automatic backup taken before every on-board write.
fn auto_backup_path(app: &AppHandle, device: &device::Device) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name: String = device.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("onboard-backups");
    Ok(dir.join(format!("{name}-{stamp}.json")))
}

/// G HUB's agent also writes on-board memory; two writers at once could
/// corrupt a sector, so writes are refused while it runs.
fn ghub_agent_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq lghub_agent.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("lghub_agent.exe"))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

const GHUB_RUNNING: &str = "G HUB is running. Quit G HUB (tray icon → Quit) before changing on-board memory, so the two don't write at the same time.";

/// Reads the on-board memory and writes a backup to `path`. Read-only towards the device.
#[tauri::command(async)]
fn save_onboard_backup(app: AppHandle, state: State<AppState>, id: String, path: String) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager
        .with_device(&id, |d, t| {
            let (snapshot, raw) = d.onboard_snapshot(t)?;
            write_onboard_backup(&app.package_info().version.to_string(), std::path::Path::new(&path), d, &snapshot, &raw).map_err(hidpp::Error::Other)
        })
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct OnboardWriteResult {
    report: onboard::WriteReport,
    backup_path: String,
    snapshot: onboard::Snapshot,
}

/// Backs up the on-board memory, then writes new DPI levels, starting level
/// and report rate into one user slot, verified by reading back.
#[tauri::command(async)]
fn write_onboard_slot(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    settings: onboard::SlotSettings,
) -> Result<OnboardWriteResult, String> {
    if ghub_agent_running() {
        return Err(GHUB_RUNNING.into());
    }
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager
        .with_device(&id, |d, t| {
            let (before, raw) = d.onboard_snapshot(t)?;
            let backup = auto_backup_path(&app, d).map_err(hidpp::Error::Other)?;
            write_onboard_backup(&app.package_info().version.to_string(), &backup, d, &before, &raw).map_err(hidpp::Error::Other)?;
            let report = d.write_slot_settings(t, &settings)?;
            let (snapshot, _) = d.onboard_snapshot(t)?;
            Ok(OnboardWriteResult { report, backup_path: backup.display().to_string(), snapshot })
        })
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct OnboardRestoreResult {
    sectors_written: usize,
    backup_path: String,
    snapshot: onboard::Snapshot,
}

/// Restores the directory and user profiles from a backup file made for the
/// same mouse model. The current state is backed up first.
#[tauri::command(async)]
fn restore_onboard_backup(app: AppHandle, state: State<AppState>, id: String, path: String) -> Result<OnboardRestoreResult, String> {
    if ghub_agent_running() {
        return Err(GHUB_RUNNING.into());
    }
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?)
        .map_err(|e| format!("not a backup file: {e}"))?;
    if file.get("format").and_then(|f| f.as_str()) != Some("open-ghub-onboard-backup/1") {
        return Err("not an Open GHub on-board backup".into());
    }
    let raw: Vec<onboard::RawSector> =
        serde_json::from_value(file.get("sectors").cloned().unwrap_or_default()).map_err(|e| format!("backup sectors are unreadable: {e}"))?;
    let sectors = onboard::restorable_sectors(&raw)?;
    let backup_models: Vec<String> = serde_json::from_value(file.get("model_ids").cloned().unwrap_or_default()).unwrap_or_default();

    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager
        .with_device(&id, |d, t| {
            let models: Vec<String> = d.model_ids.iter().map(|m| format!("{m:04X}")).collect();
            if !backup_models.is_empty() && !backup_models.iter().any(|m| models.contains(m)) {
                return Err(hidpp::Error::Other(format!("this backup is for a different mouse ({})", backup_models.join(", "))));
            }
            let (before, current_raw) = d.onboard_snapshot(t)?;
            let backup = auto_backup_path(&app, d).map_err(hidpp::Error::Other)?;
            write_onboard_backup(&app.package_info().version.to_string(), &backup, d, &before, &current_raw).map_err(hidpp::Error::Other)?;
            let sectors_written = d.restore_sectors(t, &sectors)?;
            let (snapshot, _) = d.onboard_snapshot(t)?;
            Ok(OnboardRestoreResult { sectors_written, backup_path: backup.display().to_string(), snapshot })
        })
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn get_profiles(state: State<ProfilesState>) -> Result<ProfilesView, String> {
    Ok(view(&*lock_store(&state)?))
}

#[tauri::command(async)]
fn save_profile(app: AppHandle, state: State<ProfilesState>, mut profile: Profile) -> Result<ProfilesView, String> {
    if profile.id.is_empty() {
        profile.id = profiles::new_id();
    }
    if profile.name.trim().is_empty() || profile.levels.is_empty() {
        return Err("a profile needs a name and at least one DPI level".into());
    }
    let view = update_store(&state, |store| store.upsert(profile))?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn delete_profile(app: AppHandle, state: State<ProfilesState>, id: String) -> Result<ProfilesView, String> {
    let view = update_store(&state, |store| store.delete(&id))?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn set_auto_switch(app: AppHandle, state: State<ProfilesState>, enabled: bool) -> Result<ProfilesView, String> {
    let view = update_store(&state, |store| store.auto_switch = enabled)?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn activate_profile(app: AppHandle, id: String) -> Result<Activation, String> {
    activate(&app, &id, "manual")
}

#[derive(Serialize)]
struct ImportResult {
    summary: ImportSummary,
    view: ProfilesView,
}

#[tauri::command(async)]
fn import_ghub_profiles(app: AppHandle, state: State<ProfilesState>) -> Result<ImportResult, String> {
    let path = profiles::ghub_settings_path()
        .filter(|p| p.exists())
        .ok_or_else(|| "G HUB settings not found (expected %LOCALAPPDATA%\\LGHUB\\settings.db)".to_string())?;
    let mut summary = None;
    let view = update_store(&state, |store| summary = Some(profiles::import_ghub(store, &path)))?;
    let summary = summary.expect("import ran")?;
    tray::refresh(&app);
    Ok(ImportResult { summary, view })
}

// ---------- app settings and updates ----------

#[derive(Serialize)]
struct AppInfo {
    version: String,
    settings: settings::AppSettings,
    update: Option<updater::UpdateInfo>,
}

#[tauri::command(async)]
fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        settings: app.state::<settings::SettingsState>().get(),
        update: app.state::<updater::UpdateState>().available(),
    }
}

#[tauri::command(async)]
fn set_app_settings(app: AppHandle, settings: settings::AppSettings) -> Result<(), String> {
    app.state::<settings::SettingsState>().set(settings)
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Option<updater::UpdateInfo>, String> {
    updater::check(&app).await
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    updater::install(&app).await
}

/// Runs `f` on the first device with on-board profiles (CLI diagnostics).
/// Refuses while G HUB's agent runs, like the app.
fn onboard_cli(f: impl FnOnce(&device::Device, &hidpp::Transport) -> Result<String, String>) {
    if ghub_agent_running() {
        return eprintln!("{GHUB_RUNNING}");
    }
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    if let Err(e) = manager.scan() {
        return eprintln!("scan failed: {e}");
    }
    let Some(id) = manager.devices().iter().find(|d| d.has(device::feature::ONBOARD_PROFILES)).map(|d| d.id.clone()) else {
        return eprintln!("no device with on-board profiles found");
    };
    let mut f = Some(f);
    match manager.with_device(&id, |d, t| Ok((f.take().expect("called once"))(d, t))) {
        Ok(Ok(out)) => println!("{out}"),
        Ok(Err(e)) => eprintln!("{e}"),
        Err(e) => eprintln!("{e}"),
    }
}

/// `open-ghub --onboard-dump [backup.json]`: read-only. Prints the decoded
/// on-board profiles; with a path, also writes the raw sectors there.
fn onboard_dump(backup_path: Option<String>) {
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    if let Err(e) = manager.scan() {
        return eprintln!("scan failed: {e}");
    }
    for d in manager.devices() {
        if !d.has(device::feature::ONBOARD_PROFILES) {
            continue;
        }
        match manager.with_device(&d.id, |d, t| d.onboard_snapshot(t)) {
            Ok((snapshot, raw)) => {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "device": d.name, "snapshot": snapshot })).unwrap_or_default());
                if let Some(path) = &backup_path {
                    let backup = serde_json::json!({
                        "device": d.name, "model_ids": d.model_ids, "info": snapshot.info, "sectors": raw,
                    });
                    match std::fs::write(path, serde_json::to_vec_pretty(&backup).unwrap_or_default()) {
                        Ok(()) => eprintln!("raw sectors written to {path}"),
                        Err(e) => eprintln!("could not write {path}: {e}"),
                    }
                }
            }
            Err(e) => eprintln!("{}: {e}", d.name),
        }
    }
}

/// `open-ghub --probe`: read-only diagnostic that prints every detected
/// device and its current settings as JSON, without opening a window.
fn probe() {
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    match manager.scan() {
        Ok(warnings) => warnings.iter().for_each(|w| eprintln!("warning: {w}")),
        Err(e) => return eprintln!("scan failed: {e}"),
    }
    for d in manager.devices() {
        let state = manager.with_device(&d.id, |d, t| Ok(d.state(t))).ok();
        let features: Vec<String> = d.features.values().map(|f| format!("{:04X}", f.id)).collect();
        let report = serde_json::json!({
            "name": d.name, "firmware_name": d.firmware_name,
            "model_ids": d.model_ids.iter().map(|id| format!("{id:04X}")).collect::<Vec<_>>(), "kind": d.kind, "product_id": format!("{:04X}", d.product_id),
            "index": d.index, "protocol": d.protocol, "features": features, "state": state,
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    }
    if manager.devices().is_empty() {
        println!("no HID++ devices found");
    }
    println!("foreground app: {}", foreground::foreground_exe().unwrap_or_else(|| "(none)".into()));
}

/// `open-ghub --button-state`: read-only. Prints what decides whether the
/// mouse runs its stored button bindings: on-board mode, active slot, and the
/// 0x8110 Mouse Button Spy button count and remapping table.
fn button_state() {
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    if let Err(e) = manager.scan() {
        return eprintln!("scan failed: {e}");
    }
    let hex = |r: &[u8]| r.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    for d in manager.devices() {
        let out = manager.with_device(&d.id, |d, t| {
            let mut out = format!("{}\n", d.name);
            out += &format!("  mode: {:?}\n", d.onboard_mode(t));
            if d.has(device::feature::ONBOARD_PROFILES) {
                out += &format!("  0x8100 getActiveProfile: {:?}\n", d.call(t, device::feature::ONBOARD_PROFILES, 4, &[]).map(|r| hex(&r)));
            }
            if d.has(0x8110) {
                for f in 0..=3u8 {
                    if f == 1 || f == 2 {
                        continue; // start/stop spy change state
                    }
                    out += &format!("  0x8110 fn{f}: {:?}\n", d.call(t, 0x8110, f, &[]).map(|r| hex(&r)));
                }
            }
            Ok(out)
        });
        match out {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("{}: {e}", d.name),
        }
    }
    if manager.devices().is_empty() {
        println!("no HID++ devices found");
    }
}

/// `open-ghub --listen [seconds]`: read-only. Prints every HID++ notification
/// for a while (default 15 s), with the feature index map, so you can see what
/// a button press sends.
fn listen(seconds: u64) {
    let sink: hidpp::EventSink = std::sync::Arc::new(|event| {
        if let hidpp::Event::Notification { report, .. } = event {
            let hex: Vec<String> = report.iter().take(20).map(|b| format!("{b:02x}")).collect();
            println!("{}", hex.join(" "));
        }
    });
    let mut manager = match Manager::new(sink) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    if let Err(e) = manager.scan() {
        return eprintln!("scan failed: {e}");
    }
    for d in manager.devices() {
        let mut map: Vec<_> = d.features.values().map(|f| (f.index, f.id)).collect();
        map.sort();
        let map: Vec<String> = map.iter().map(|(i, id)| format!("{i:02x}={id:04X}")).collect();
        println!("{} (device index {:02x}): {}", d.name, d.index, map.join(" "));
    }
    println!("listening {seconds}s; press the buttons now");
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    drop(manager);
}

fn main() {
    if std::env::args().any(|a| a == "--probe") {
        return probe();
    }
    // Keyboard devices Windows isn't running: --blocked-keyboards <vid hex> <pid hex>
    if let Some(pos) = std::env::args().position(|a| a == "--blocked-keyboards") {
        let arg = |i: usize| std::env::args().nth(pos + i).and_then(|s| u16::from_str_radix(&s, 16).ok());
        let (Some(vid), Some(pid)) = (arg(1), arg(2)) else { return eprintln!("usage: --blocked-keyboards <vid> <pid>") };
        return println!("{}", serde_json::to_string_pretty(&keyboard_iface::blocked_keyboards_for(vid, pid)).unwrap_or_default());
    }
    if let Some(pos) = std::env::args().position(|a| a == "--listen") {
        return listen(std::env::args().nth(pos + 1).and_then(|s| s.parse().ok()).unwrap_or(15));
    }
    if std::env::args().any(|a| a == "--button-state") {
        return button_state();
    }
    // Read-only: dump on-board profile memory as JSON (and raw sectors to a file).
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-dump") {
        return onboard_dump(std::env::args().nth(pos + 1));
    }
    // Writes one on-board slot (backup first): --onboard-write <slot> <dpi,dpi,..> <start> <hz> <backup.json>
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-write") {
        let a: Vec<String> = std::env::args().skip(pos + 1).take(5).collect();
        return onboard_cli(|d, t| {
            let [slot, dpis, start, hz, backup] = a.as_slice() else { return Err("usage: --onboard-write <slot> <dpi,..> <start> <hz> <backup.json>".into()) };
            let settings = onboard::SlotSettings {
                slot: slot.parse().map_err(|_| "bad slot")?,
                dpi_levels: Some(dpis.split(',').map(|v| v.parse().map_err(|_| "bad dpi")).collect::<Result<_, _>>()?),
                default_index: Some(start.parse().map_err(|_| "bad start index")?),
                report_rate_hz: Some(hz.parse().map_err(|_| "bad rate")?),
                buttons: Vec::new(),
            };
            let (snapshot, raw) = d.onboard_snapshot(t).map_err(|e| e.to_string())?;
            write_onboard_backup(env!("CARGO_PKG_VERSION"), std::path::Path::new(backup), d, &snapshot, &raw)?;
            let report = d.write_slot_settings(t, &settings).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string_pretty(&report).unwrap_or_default())
        });
    }
    // Switches mode: --onboard-mode <onboard|host>. Prints the mode and active slot before and after.
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-mode") {
        let target = std::env::args().nth(pos + 1).unwrap_or_default();
        return onboard_cli(move |d, t| {
            let host = match target.as_str() {
                "host" => true,
                "onboard" => false,
                _ => return Err("usage: --onboard-mode <onboard|host>".into()),
            };
            let active = |d: &device::Device, t: &hidpp::Transport| d.call(t, device::feature::ONBOARD_PROFILES, 4, &[]).map(|r| r[1]).map_err(|e| e.to_string());
            let mode = |d: &device::Device, t: &hidpp::Transport| d.onboard_mode(t).map_err(|e| e.to_string());
            let remap = |d: &device::Device, t: &hidpp::Transport| d.button_remap(t).map_err(|e| e.to_string());
            let mut out = format!("before: mode {:?}, active slot {}, button remap {:?}\n", mode(d, t)?, active(d, t)?, remap(d, t)?);
            d.set_onboard_mode(t, host).map_err(|e| e.to_string())?;
            if !host {
                let reset = d.reset_button_spy(t).map_err(|e| e.to_string())?;
                let slot = d.ensure_active_profile(t).map_err(|e| e.to_string())?;
                out += &format!("button spy reset: {reset}; ensured slot {slot}\n");
            }
            out += &format!("after: mode {:?}, active slot {}, button remap {:?}", mode(d, t)?, active(d, t)?, remap(d, t)?);
            Ok(out)
        });
    }
    // Rebinds one button (backup first): --onboard-bind <slot> <button 1-based> <default|gshift> '<binding json>' <backup.json>
    // e.g. --onboard-bind 1 4 default '{"type":"key","modifiers":1,"key":6}' backup.json   (Ctrl+C)
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-bind") {
        let a: Vec<String> = std::env::args().skip(pos + 1).take(5).collect();
        return onboard_cli(|d, t| {
            let [slot, button, layer, binding, backup] = a.as_slice() else {
                return Err("usage: --onboard-bind <slot> <button> <default|gshift> '<binding json>' <backup.json>".into());
            };
            let button: u8 = button.parse().map_err(|_| "bad button")?;
            let settings = onboard::SlotSettings {
                slot: slot.parse().map_err(|_| "bad slot")?,
                dpi_levels: None,
                default_index: None,
                report_rate_hz: None,
                buttons: vec![onboard::ButtonChange {
                    index: button.checked_sub(1).ok_or("buttons are numbered from 1")?,
                    gshift: match layer.as_str() {
                        "default" => false,
                        "gshift" => true,
                        _ => return Err("layer must be default or gshift".into()),
                    },
                    binding: serde_json::from_str(binding).map_err(|e| format!("bad binding json: {e}"))?,
                }],
            };
            let (snapshot, raw) = d.onboard_snapshot(t).map_err(|e| e.to_string())?;
            write_onboard_backup(env!("CARGO_PKG_VERSION"), std::path::Path::new(backup), d, &snapshot, &raw)?;
            let report = d.write_slot_settings(t, &settings).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string_pretty(&report).unwrap_or_default())
        });
    }
    // Restores user sectors from a backup: --onboard-restore <backup.json>
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-restore") {
        let path = std::env::args().nth(pos + 1).unwrap_or_default();
        return onboard_cli(move |d, t| {
            let file: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let raw: Vec<onboard::RawSector> = serde_json::from_value(file["sectors"].clone()).map_err(|e| e.to_string())?;
            let sectors = onboard::restorable_sectors(&raw)?;
            let written = d.restore_sectors(t, &sectors).map_err(|e| e.to_string())?;
            Ok(format!("restored {written} sector(s)"))
        });
    }
    // Read-only: print what an import from G HUB would produce, without saving.
    if std::env::args().any(|a| a == "--ghub-profiles") {
        let mut store = ProfileStore::default();
        let result = profiles::ghub_settings_path()
            .ok_or_else(|| "LOCALAPPDATA not set".to_string())
            .and_then(|p| profiles::import_ghub(&mut store, &p));
        match result {
            Ok(s) => println!(
                "{}\nimported {}, updated {}, skipped {}",
                serde_json::to_string_pretty(&store.profiles).unwrap_or_default(),
                s.imported,
                s.updated,
                s.skipped
            ),
            Err(e) => eprintln!("{e}"),
        }
        return;
    }
    tauri::Builder::default()
        // Must be first: a second launch focuses this window instead of
        // starting another instance that would also talk to the mouse.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .plugin(tauri_plugin_autostart::Builder::new().args(["--minimized"]).build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::UpdateState::default())
        .manage(tray::BatteryCache::default())
        .manage(AppliedDevices::default())
        .manage(AppState(Mutex::new(None)))
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("profiles.json");
            let store = ProfileStore::load(&path);
            app.manage(ProfilesState { store: Mutex::new(store), path });
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            app.manage(settings::SettingsState::load(settings_path));
            // Before anything creates the device manager, so HID readers get a live sink.
            events::start(app.handle().clone());
            spawn_profile_watcher(app.handle().clone());
            tray::create(app.handle())?;
            updater::spawn_background(app.handle().clone());
            // Started by "Start with Windows": stay in the tray.
            if !std::env::args().any(|a| a == "--minimized") {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        // Closing the window destroys it (freeing WebView2); the app keeps
        // running in the tray, see the ExitRequested handler below.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                tray::tray_hint_once(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan_devices,
            list_devices,
            device_state,
            set_dpi,
            set_report_rate,
            set_onboard_mode,
            get_profiles,
            save_profile,
            delete_profile,
            set_auto_switch,
            activate_profile,
            import_ghub_profiles,
            onboard_snapshot,
            blocked_keyboards,
            save_onboard_backup,
            write_onboard_slot,
            restore_onboard_backup,
            get_app_info,
            set_app_settings,
            check_for_update,
            install_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building Open GHub")
        .run(|_app, event| {
            // The last window closing would normally quit; stay in the tray
            // instead. Quit from the tray menu passes an exit code and goes through.
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}
