import { invoke } from "@tauri-apps/api/core";
import type { ButtonChange } from "@/lib/bindings";

// Mirrors the Serialize types in src-tauri/src/{main,device}.rs.

export interface FeatureInfo {
  id: number;
  index: number;
  version: number;
  flags: number;
  name: string;
}

export interface DeviceSummary {
  id: string;
  name: string;
  product: string;
  product_id: string;
  kind: string;
  /** Name the firmware reports; `name` may be the marketing name instead. */
  firmware_name: string;
  /** Product ids per transport (hex), e.g. ["4093"]. */
  model_ids: string[];
  connection: string;
  protocol: string;
  features: FeatureInfo[];
}

export interface Battery {
  percent: number | null;
  level: string | null;
  status: string;
  voltage_mv: number | null;
}

export interface Dpi {
  feature: number;
  x: number;
  y: number | null;
  default: number | null;
  supported: number[];
  supports_y: boolean;
  lod: number | null;
  supports_lod: boolean;
}

export interface Choice {
  value: number;
  label: string;
  hz: number;
}

export interface ReportRate {
  feature: number;
  current: number;
  choices: Choice[];
}

export interface DeviceState {
  battery: Battery | null;
  dpi: Dpi | null;
  report_rate: ReportRate | null;
  onboard_mode: "onboard" | "host" | null;
  errors: string[];
}

export interface ScanResult {
  devices: DeviceSummary[];
  warnings: string[];
}

// Mirrors src-tauri/src/profiles.rs (fields are snake_case on the wire).

export interface ProfileLevel {
  x: number;
  /** null means Y follows X. */
  y: number | null;
}

export interface Profile {
  /** Empty string when creating a new profile. */
  id: string;
  name: string;
  /** Executable or install folder that activates the profile; null for Desktop. */
  app_path: string | null;
  levels: ProfileLevel[];
  active_level: number;
  report_rate_hz: number | null;
  ghub_id?: string | null;
}

export interface ProfilesView {
  profiles: Profile[];
  auto_switch: boolean;
  active_id: string;
}

export interface Activation {
  id: string;
  name: string;
  reason: "manual" | "auto" | "tray";
  errors: string[];
}

export interface ImportResult {
  summary: { imported: number; updated: number; skipped: number };
  view: ProfilesView;
}

export interface AppSettings {
  check_updates: boolean;
  auto_install_updates: boolean;
}

export interface UpdateInfo {
  version: string;
  current_version: string;
  notes: string | null;
  date: string | null;
}

export interface AppInfo {
  version: string;
  settings: AppSettings;
  update: UpdateInfo | null;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

// Mirrors src-tauri/src/onboard.rs (read-only on-board memory view).

export interface OnboardInfo {
  memory_model: number;
  profile_format: number;
  macro_format: number;
  profile_count: number;
  rom_profile_count: number;
  button_count: number;
  sector_count: number;
  sector_size: number;
  mechanical_layout: number;
  various_info: number;
}

export interface OnboardButton {
  raw: number[];
  kind: string;
  description: string;
}

export interface OnboardProfile {
  slot: number;
  sector: number;
  rom: boolean;
  enabled: boolean;
  /** null for factory (ROM) profiles, which carry no checksum. */
  crc_valid: boolean | null;
  name: string | null;
  report_rate_hz: number;
  default_dpi_index: number;
  shift_dpi_index: number;
  dpi_levels: number[];
  angle_snapping: number;
  powersave_timeout: number;
  poweroff_timeout: number;
  buttons: OnboardButton[];
  gshift_buttons: OnboardButton[];
}

export interface OnboardSnapshot {
  info: OnboardInfo;
  directory_crc_valid: boolean;
  /** 0 when no on-board profile is active (software mode). */
  active_profile: number;
  active_dpi_index: number | null;
  profiles: OnboardProfile[];
  rom_profiles: OnboardProfile[];
}

/** What to change in one slot. DPI levels, start index and report rate go together; buttons are separate. */
export interface SlotSettings {
  slot: number;
  dpi_levels?: number[];
  default_index?: number;
  report_rate_hz?: number;
  buttons?: ButtonChange[];
}

/** A keyboard device of the receiver/mouse that Windows isn't running. */
export interface BlockedKeyboard {
  instance_id: string;
  /** 22 = disabled in Device Manager. */
  problem_code: number;
}

export interface OnboardWriteResult {
  report: { slot: number; sector: number; bytes_changed: number; unchanged: boolean };
  backup_path: string;
  snapshot: OnboardSnapshot;
}

export interface OnboardRestoreResult {
  sectors_written: number;
  backup_path: string;
  snapshot: OnboardSnapshot;
}

/** Report rates the on-board format can store. */
export const ONBOARD_RATES_HZ = [125, 250, 500, 1000];

export const DESKTOP_ID = "desktop";
export const REPORT_RATES_HZ = [125, 250, 500, 1000, 2000, 4000, 8000];

export const api = {
  scan: () => invoke<ScanResult>("scan_devices"),
  /** Current devices without rescanning; the backend keeps them up to date. */
  listDevices: () => invoke<DeviceSummary[]>("list_devices"),
  state: (id: string) => invoke<DeviceState>("device_state", { id }),
  setDpi: (id: string, x: number, y: number | null, lod: number | null) =>
    invoke<void>("set_dpi", { id, x, y, lod }),
  setReportRate: (id: string, value: number) => invoke<void>("set_report_rate", { id, value }),
  setOnboardMode: (id: string, host: boolean) => invoke<void>("set_onboard_mode", { id, host }),

  profiles: () => invoke<ProfilesView>("get_profiles"),
  saveProfile: (profile: Profile) => invoke<ProfilesView>("save_profile", { profile }),
  deleteProfile: (id: string) => invoke<ProfilesView>("delete_profile", { id }),
  setAutoSwitch: (enabled: boolean) => invoke<ProfilesView>("set_auto_switch", { enabled }),
  activateProfile: (id: string) => invoke<Activation>("activate_profile", { id }),
  importGhub: () => invoke<ImportResult>("import_ghub_profiles"),

  onboardSnapshot: (id: string) => invoke<OnboardSnapshot>("onboard_snapshot", { id }),
  /** `productId` is the hex id from DeviceSummary.product_id (the receiver's, for wireless mice). */
  blockedKeyboards: (productId: string) => invoke<BlockedKeyboard[]>("blocked_keyboards", { productId }),
  saveOnboardBackup: (id: string, path: string) => invoke<void>("save_onboard_backup", { id, path }),
  writeOnboardSlot: (id: string, settings: SlotSettings) =>
    invoke<OnboardWriteResult>("write_onboard_slot", { id, settings }),
  restoreOnboardBackup: (id: string, path: string) =>
    invoke<OnboardRestoreResult>("restore_onboard_backup", { id, path }),

  appInfo: () => invoke<AppInfo>("get_app_info"),
  setAppSettings: (settings: AppSettings) => invoke<void>("set_app_settings", { settings }),
  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  /** On Windows the app exits and the installer restarts it when this succeeds. */
  installUpdate: () => invoke<void>("install_update"),
};

/** Battery percent, estimated from the coarse level when the device has no exact figure. */
export function batteryPercent(b: Battery): number | null {
  if (b.percent != null) return b.percent;
  const byLevel: Record<string, number> = { full: 100, good: 70, low: 25, critical: 8 };
  return b.level ? (byLevel[b.level] ?? null) : null;
}
