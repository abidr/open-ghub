// Button bindings that can be stored in a mouse's on-board profile.
//
// The mouse stores a binding as 4 bytes (see `BindingSpec` in
// src-tauri/src/onboard.rs). Only what the firmware can run on its own fits:
// a key with modifiers, a media key, a mouse button or a mouse-side function.
// G HUB's Actions and Macros need its PC agent, so they are not offered here.

export type BindingSpec =
  | { type: "none" }
  | { type: "disabled" }
  | { type: "mouse"; button: number }
  | { type: "key"; modifiers: number; key: number }
  | { type: "media"; usage: number }
  | { type: "special"; id: number };

export interface ButtonChange {
  index: number;
  gshift: boolean;
  binding: BindingSpec;
}

/** HID keyboard modifier bits. */
export const MOD = { ctrl: 0x01, shift: 0x02, alt: 0x04, win: 0x08 } as const;

const MOD_LABELS: [number, string][] = [
  [MOD.ctrl, "Ctrl"],
  [MOD.shift, "Shift"],
  [MOD.alt, "Alt"],
  [MOD.win, "Win"],
  [0x10, "Right Ctrl"],
  [0x20, "Right Shift"],
  [0x40, "Right Alt"],
  [0x80, "Right Win"],
];

/** HID keyboard usages by `KeyboardEvent.code`, with the label to show. */
const KEY_TABLE: Record<string, [number, string]> = {};
(() => {
  for (let i = 0; i < 26; i++) {
    const letter = String.fromCharCode(65 + i);
    KEY_TABLE[`Key${letter}`] = [0x04 + i, letter];
  }
  for (let i = 1; i <= 9; i++) KEY_TABLE[`Digit${i}`] = [0x1d + i, String(i)];
  KEY_TABLE.Digit0 = [0x27, "0"];
  for (let i = 1; i <= 12; i++) KEY_TABLE[`F${i}`] = [0x39 + i, `F${i}`];
  for (let i = 13; i <= 24; i++) KEY_TABLE[`F${i}`] = [0x67 + (i - 12), `F${i}`];
  for (let i = 1; i <= 9; i++) KEY_TABLE[`Numpad${i}`] = [0x58 + i, `Num ${i}`];
  KEY_TABLE.Numpad0 = [0x62, "Num 0"];
  Object.assign(KEY_TABLE, {
    Enter: [0x28, "Enter"],
    Escape: [0x29, "Esc"],
    Backspace: [0x2a, "Backspace"],
    Tab: [0x2b, "Tab"],
    Space: [0x2c, "Space"],
    Minus: [0x2d, "-"],
    Equal: [0x2e, "="],
    BracketLeft: [0x2f, "["],
    BracketRight: [0x30, "]"],
    Backslash: [0x31, "\\"],
    Semicolon: [0x33, ";"],
    Quote: [0x34, "'"],
    Backquote: [0x35, "`"],
    Comma: [0x36, ","],
    Period: [0x37, "."],
    Slash: [0x38, "/"],
    CapsLock: [0x39, "Caps Lock"],
    PrintScreen: [0x46, "Print Screen"],
    ScrollLock: [0x47, "Scroll Lock"],
    Pause: [0x48, "Pause"],
    Insert: [0x49, "Insert"],
    Home: [0x4a, "Home"],
    PageUp: [0x4b, "Page Up"],
    Delete: [0x4c, "Delete"],
    End: [0x4d, "End"],
    PageDown: [0x4e, "Page Down"],
    ArrowRight: [0x4f, "Right"],
    ArrowLeft: [0x50, "Left"],
    ArrowDown: [0x51, "Down"],
    ArrowUp: [0x52, "Up"],
    NumLock: [0x53, "Num Lock"],
    NumpadDivide: [0x54, "Num /"],
    NumpadMultiply: [0x55, "Num *"],
    NumpadSubtract: [0x56, "Num -"],
    NumpadAdd: [0x57, "Num +"],
    NumpadEnter: [0x58, "Num Enter"],
    NumpadDecimal: [0x63, "Num ."],
    ContextMenu: [0x65, "Menu"],
  });
})();

const KEY_NAMES = new Map<number, string>(Object.values(KEY_TABLE).map(([usage, name]) => [usage, name]));

/** HID usage for a `KeyboardEvent.code`, or null when the mouse can't send it. */
export function usageForCode(code: string): number | null {
  return KEY_TABLE[code]?.[0] ?? null;
}

/** Modifier bit for a `KeyboardEvent.code` that is itself a modifier key. */
export function modifierForCode(code: string): number {
  switch (code) {
    case "ControlLeft": return MOD.ctrl;
    case "ShiftLeft": return MOD.shift;
    case "AltLeft": return MOD.alt;
    case "MetaLeft": return MOD.win;
    case "ControlRight": return 0x10;
    case "ShiftRight": return 0x20;
    case "AltRight": return 0x40;
    case "MetaRight": return 0x80;
    default: return 0;
  }
}

export function keyLabel(modifiers: number, key: number): string {
  const parts = MOD_LABELS.filter(([bit]) => modifiers & bit).map(([, name]) => name);
  if (key) parts.push(KEY_NAMES.get(key) ?? `Key 0x${key.toString(16).padStart(2, "0")}`);
  return parts.join(" + ");
}

// ---- Media keys (HID consumer usages) --------------------------------------

export const MEDIA: { name: string; usage: number }[] = [
  { name: "Play / Pause", usage: 0x00cd },
  { name: "Next Track", usage: 0x00b5 },
  { name: "Previous Track", usage: 0x00b6 },
  { name: "Stop", usage: 0x00b7 },
  { name: "Mute", usage: 0x00e2 },
  { name: "Volume Up", usage: 0x00e9 },
  { name: "Volume Down", usage: 0x00ea },
  { name: "Calculator", usage: 0x0192 },
  { name: "My Computer", usage: 0x0194 },
];

// ---- Mouse-side functions (index = the firmware's special-function id) -----

export const SPECIALS: { id: number; name: string }[] = [
  { id: 3, name: "DPI Up" },
  { id: 4, name: "DPI Down" },
  { id: 5, name: "DPI Cycle" },
  { id: 6, name: "DPI Default" },
  { id: 7, name: "DPI Shift (sniper)" },
  { id: 8, name: "Profile Up" },
  { id: 9, name: "Profile Down" },
  { id: 10, name: "Profile Cycle" },
  { id: 11, name: "G-Shift" },
  { id: 1, name: "Wheel Left" },
  { id: 2, name: "Wheel Right" },
  { id: 12, name: "Battery Indicator" },
  { id: 16, name: "Scroll Down" },
  { id: 17, name: "Scroll Up" },
];

/** Names for every special id the firmware defines (libratbag hidpp20.h), for display. */
const SPECIAL_NAMES: Record<number, string> = {
  ...Object.fromEntries(SPECIALS.map((s) => [s.id, s.name])),
  13: "Enable Profile",
  14: "Performance Switch",
  15: "Host",
};

export const MOUSE_BUTTONS = ["Left Click", "Right Click", "Middle Click", "Back", "Forward"];

// ---- Commands: named keyboard shortcuts, like G HUB's Commands tab ----------

const C = MOD.ctrl;
const S = MOD.shift;
const A = MOD.alt;
const W = MOD.win;

export interface Command {
  name: string;
  modifiers: number;
  key: number;
}

export const COMMAND_GROUPS: { title: string; commands: Command[] }[] = [
  {
    title: "Editing",
    commands: [
      { name: "Copy", modifiers: C, key: 0x06 },
      { name: "Paste", modifiers: C, key: 0x19 },
      { name: "Cut", modifiers: C, key: 0x1b },
      { name: "Undo", modifiers: C, key: 0x1d },
      { name: "Redo", modifiers: C, key: 0x1c },
      { name: "Select All", modifiers: C, key: 0x04 },
      { name: "Save", modifiers: C, key: 0x16 },
      { name: "Find", modifiers: C, key: 0x09 },
      { name: "Zoom In", modifiers: C, key: 0x2e },
      { name: "Zoom Out", modifiers: C, key: 0x2d },
      { name: "Zoom Reset", modifiers: C, key: 0x27 },
    ],
  },
  {
    title: "Windows",
    commands: [
      { name: "Open Task View", modifiers: W, key: 0x2b },
      { name: "Hide/Show Desktop", modifiers: W, key: 0x07 },
      { name: "Minimize All Windows", modifiers: W, key: 0x10 },
      { name: "Open File Explorer", modifiers: W, key: 0x08 },
      { name: "Run dialog", modifiers: W, key: 0x15 },
      { name: "Open Search", modifiers: W, key: 0x16 },
      { name: "Lock PC", modifiers: W, key: 0x0f },
      { name: "Open Emoji Panel", modifiers: W, key: 0x37 },
      { name: "Screenshot (Snip)", modifiers: W | S, key: 0x16 },
      { name: "Switch Window", modifiers: A, key: 0x2b },
      { name: "Close Window", modifiers: A, key: 0x3d },
      { name: "Next Desktop", modifiers: C | W, key: 0x4f },
      { name: "Previous Desktop", modifiers: C | W, key: 0x50 },
    ],
  },
  {
    title: "Browser",
    commands: [
      { name: "Browser Back", modifiers: A, key: 0x50 },
      { name: "Browser Forward", modifiers: A, key: 0x4f },
      { name: "New Tab", modifiers: C, key: 0x17 },
      { name: "Close Tab", modifiers: C, key: 0x1a },
      { name: "Reopen Closed Tab", modifiers: C | S, key: 0x17 },
      { name: "Next Tab", modifiers: C, key: 0x2b },
      { name: "Previous Tab", modifiers: C | S, key: 0x2b },
      { name: "Refresh", modifiers: 0, key: 0x3e },
    ],
  },
];

const COMMAND_NAMES = new Map<string, string>();
for (const g of COMMAND_GROUPS) for (const c of g.commands) COMMAND_NAMES.set(`${c.modifiers}:${c.key}`, c.name);

// ---- Describing what a slot currently holds -------------------------------

/** Friendly text for the 4 raw bytes of a stored binding. */
export function describeRaw(raw: number[]): string {
  const [type, sub, b2, b3] = raw;
  if (type === 0xff) return "Disabled";
  if (type === 0x90) return SPECIAL_NAMES[sub] ?? `Function ${sub}`;
  if (type === 0x00) return "Macro";
  if (type !== 0x80) return `Unknown (0x${type.toString(16)})`;
  switch (sub) {
    case 0x00:
      return "Unassigned";
    case 0x01: {
      const mask = (b2 << 8) | b3;
      const n = mask === 0 ? 0 : Math.log2(mask & -mask) + 1;
      return MOUSE_BUTTONS[n - 1] ?? `Mouse button ${n}`;
    }
    case 0x02:
      return COMMAND_NAMES.get(`${b2}:${b3}`) ?? keyLabel(b2, b3);
    case 0x03: {
      const usage = (b2 << 8) | b3;
      return MEDIA.find((m) => m.usage === usage)?.name ?? `Media 0x${usage.toString(16)}`;
    }
    default:
      return `Unknown (0x80 0x${sub.toString(16)})`;
  }
}

/** Friendly text for a binding we're about to write. */
export function describeSpec(spec: BindingSpec): string {
  switch (spec.type) {
    case "none":
      return "Unassigned";
    case "disabled":
      return "Disabled";
    case "mouse":
      return MOUSE_BUTTONS[spec.button - 1] ?? `Mouse button ${spec.button}`;
    case "key":
      return COMMAND_NAMES.get(`${spec.modifiers}:${spec.key}`) ?? keyLabel(spec.modifiers, spec.key);
    case "media":
      return MEDIA.find((m) => m.usage === spec.usage)?.name ?? `Media 0x${spec.usage.toString(16)}`;
    case "special":
      return SPECIAL_NAMES[spec.id] ?? `Function ${spec.id}`;
  }
}

/** Display name of a button by its 0-based index. */
export function buttonName(index: number): string {
  return MOUSE_BUTTONS[index] ? `${MOUSE_BUTTONS[index]} (button ${index + 1})` : `Button ${index + 1}`;
}
