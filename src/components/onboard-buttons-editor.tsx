import { useEffect, useMemo, useRef, useState } from "react";
import { Save, Search, Undo2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { OnboardProfile, SlotSettings } from "@/lib/api";
import {
  buttonName,
  COMMAND_GROUPS,
  describeRaw,
  describeSpec,
  keyLabel,
  MEDIA,
  MOD,
  modifierForCode,
  MOUSE_BUTTONS,
  SPECIALS,
  usageForCode,
  type BindingSpec,
  type ButtonChange,
} from "@/lib/bindings";
import { cn } from "@/lib/utils";

type Tab = "commands" | "keys" | "media" | "system";
const TABS: { id: Tab; label: string }[] = [
  { id: "commands", label: "Commands" },
  { id: "keys", label: "Keys" },
  { id: "media", label: "Media" },
  { id: "system", label: "System" },
];

interface Props {
  profile: OnboardProfile | null;
  /** Writes the changes to the mouse; resolves true on success. */
  onSave: (settings: SlotSettings) => Promise<boolean>;
}

const stageKey = (gshift: boolean, index: number) => `${gshift ? "g" : "d"}${index}`;

/** One selectable row in the assignment list, styled like G HUB's command list. */
function Choice({ label, hint, onPick }: { label: string; hint?: string; onPick: () => void }) {
  return (
    <button
      type="button"
      onClick={onPick}
      className="flex w-full items-baseline justify-between gap-3 rounded-md px-2.5 py-1.5 text-left text-[13px] hover:bg-secondary"
    >
      <span className="font-medium">{label}</span>
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </button>
  );
}

function GroupTitle({ children }: { children: string }) {
  return <div className="px-2.5 pt-3 pb-1 text-[11px] font-bold tracking-wider text-muted-foreground uppercase">{children}</div>;
}

/** Press-a-combination capture plus modifier toggles, for assigning an arbitrary key. */
function KeyCapture({ onPick }: { onPick: (spec: BindingSpec) => void }) {
  const [modifiers, setModifiers] = useState(0);
  const [key, setKey] = useState(0);
  const held = useRef(0);
  const toggles = [
    [MOD.ctrl, "Ctrl"],
    [MOD.shift, "Shift"],
    [MOD.alt, "Alt"],
    [MOD.win, "Win"],
  ] as const;
  const valid = key !== 0 || modifiers !== 0;

  return (
    <div className="grid gap-3 p-2.5">
      <div
        tabIndex={0}
        role="textbox"
        aria-label="Press a key combination"
        onKeyDown={(e) => {
          e.preventDefault();
          e.stopPropagation();
          // Track physically held modifier keys by code, so left and right
          // keep their own bits (e.ctrlKey can't tell them apart).
          const own = modifierForCode(e.code);
          if (own) {
            held.current |= own;
            setModifiers(held.current);
            setKey(0);
            return;
          }
          const usage = usageForCode(e.code);
          if (usage === null) return;
          setModifiers(held.current);
          setKey(usage);
        }}
        onKeyUp={(e) => {
          held.current &= ~modifierForCode(e.code);
        }}
        onBlur={() => {
          held.current = 0;
        }}
        className="flex h-16 items-center justify-center rounded-lg border border-dashed border-input bg-background/60 text-center text-sm text-muted-foreground outline-none focus:border-primary focus:text-foreground"
      >
        {valid ? <span className="text-base font-semibold text-foreground">{keyLabel(modifiers, key)}</span> : "Click here, then press a key combination"}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {toggles.map(([bit, name]) => (
          <button
            key={name}
            type="button"
            onClick={() => setModifiers(modifiers ^ bit)}
            className={cn(
              "rounded-md px-2.5 py-1 text-xs font-semibold",
              modifiers & bit ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground",
            )}
          >
            {name}
          </button>
        ))}
        <Button variant="ghost" size="sm" onClick={() => (setModifiers(0), setKey(0))}>
          Clear
        </Button>
        <Button size="sm" className="ml-auto" disabled={!valid} onClick={() => onPick({ type: "key", modifiers, key })}>
          Assign
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        The Windows key and some system shortcuts are caught by Windows before this window sees them; use the Win toggle or the
        Commands tab for those.
      </p>
    </div>
  );
}

/**
 * Assignments for one on-board slot: pick a button, pick what it does. Bindings
 * are stored in the mouse, so they work without Open GHub or G HUB running.
 * Edits are staged until "Save to mouse"; switching to another profile object
 * (after a save or a slot change) clears them.
 */
export function ButtonAssignments({ profile, onSave }: Props) {
  const [gshift, setGshift] = useState(false);
  const [selected, setSelected] = useState(0);
  const [tab, setTab] = useState<Tab>("commands");
  const [query, setQuery] = useState("");
  const [staged, setStaged] = useState<Record<string, BindingSpec>>({});
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!profile) return;
    setGshift(false);
    setSelected(0);
    setTab("commands");
    setQuery("");
    setStaged({});
  }, [profile]);

  const q = query.trim().toLowerCase();
  const groups = useMemo(
    () =>
      COMMAND_GROUPS.map((g) => ({ ...g, commands: g.commands.filter((c) => !q || c.name.toLowerCase().includes(q)) })).filter(
        (g) => g.commands.length > 0,
      ),
    [q],
  );

  if (!profile) return null;

  const layer = gshift ? profile.gshift_buttons : profile.buttons;
  const stagedCount = Object.keys(staged).length;
  const current = (i: number): string => {
    const s = staged[stageKey(gshift, i)];
    return s ? describeSpec(s) : describeRaw(layer[i].raw);
  };
  const assign = (spec: BindingSpec) => setStaged({ ...staged, [stageKey(gshift, selected)]: spec });
  const revert = (i: number) => {
    const next = { ...staged };
    delete next[stageKey(gshift, i)];
    setStaged(next);
  };

  const save = async () => {
    const buttons: ButtonChange[] = Object.entries(staged).map(([k, binding]) => ({
      gshift: k.startsWith("g"),
      index: Number(k.slice(1)),
      binding,
    }));
    setSaving(true);
    const ok = await onSave({ slot: profile.slot, buttons });
    setSaving(false);
    if (ok) setStaged({});
  };

  return (
    <div className="grid gap-4">
        <div className="flex items-center gap-3 text-xs font-bold tracking-wider uppercase">
          <span className={cn(!gshift ? "text-foreground" : "text-muted-foreground")}>Default</span>
          <Switch checked={gshift} onCheckedChange={setGshift} aria-label="Edit the G-Shift layer" />
          <span className={cn(gshift ? "text-foreground" : "text-muted-foreground")}>G-Shift</span>
          {gshift && (
            <span className="font-normal tracking-normal text-muted-foreground normal-case">
              Used while a button assigned to G-Shift is held. Assign "G-Shift" under System first.
            </span>
          )}
        </div>

        <div className="grid min-h-0 gap-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
          <div className="grid max-h-96 content-start gap-1 overflow-y-auto pr-1">
            {layer.map((_, i) => {
              const edited = staged[stageKey(gshift, i)] !== undefined;
              return (
                <div key={i} className="flex items-center gap-1">
                  <button
                    type="button"
                    onClick={() => setSelected(i)}
                    className={cn(
                      "flex min-w-0 flex-1 items-baseline justify-between gap-3 rounded-lg border px-3 py-2 text-left",
                      selected === i ? "border-primary bg-secondary" : "border-transparent bg-secondary/50 hover:bg-secondary",
                    )}
                  >
                    <span className="truncate text-xs text-muted-foreground">{buttonName(i)}</span>
                    <span className={cn("truncate text-[13px] font-semibold", edited && "text-primary")}>{current(i)}</span>
                  </button>
                  {edited && (
                    <Button variant="ghost" size="icon-xs" aria-label={`Undo change to button ${i + 1}`} onClick={() => revert(i)}>
                      <Undo2 />
                    </Button>
                  )}
                </div>
              );
            })}
          </div>

          <div className="flex max-h-96 min-h-0 flex-col rounded-lg bg-secondary/40">
            <div className="flex items-center gap-4 px-3 pt-2.5 text-[13px] font-bold tracking-wide uppercase">
              {TABS.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => setTab(t.id)}
                  className={cn("border-b-2 pb-1", tab === t.id ? "border-primary text-foreground" : "border-transparent text-muted-foreground")}
                >
                  {t.label}
                </button>
              ))}
            </div>
            <p className="px-3 pt-2 text-xs text-muted-foreground">Assigning to {buttonName(selected)}</p>

            {tab === "commands" && (
              <div className="relative mt-2 px-3">
                <Search className="pointer-events-none absolute top-2 left-5 size-3.5 text-muted-foreground" />
                <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search for a command" className="h-7 pl-7" />
              </div>
            )}

            <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2">
              {tab === "commands" &&
                groups.map((g) => (
                  <div key={g.title}>
                    <GroupTitle>{g.title}</GroupTitle>
                    {g.commands.map((c) => (
                      <Choice
                        key={c.name}
                        label={c.name}
                        hint={keyLabel(c.modifiers, c.key)}
                        onPick={() => assign({ type: "key", modifiers: c.modifiers, key: c.key })}
                      />
                    ))}
                  </div>
                ))}
              {tab === "commands" && groups.length === 0 && <p className="px-3 py-4 text-xs text-muted-foreground">No command matches.</p>}

              {tab === "keys" && <KeyCapture key={stageKey(gshift, selected)} onPick={assign} />}

              {tab === "media" && MEDIA.map((m) => <Choice key={m.name} label={m.name} onPick={() => assign({ type: "media", usage: m.usage })} />)}

              {tab === "system" && (
                <>
                  <GroupTitle>Mouse buttons</GroupTitle>
                  {[...MOUSE_BUTTONS, ...Array.from({ length: Math.max(0, layer.length - MOUSE_BUTTONS.length) }, (_, i) => `Button ${MOUSE_BUTTONS.length + i + 1}`)].map(
                    (name, i) => (
                      <Choice key={name} label={name} onPick={() => assign({ type: "mouse", button: i + 1 })} />
                    ),
                  )}
                  <GroupTitle>Mouse functions</GroupTitle>
                  {SPECIALS.map((s) => (
                    <Choice key={s.id} label={s.name} onPick={() => assign({ type: "special", id: s.id })} />
                  ))}
                  <GroupTitle>Other</GroupTitle>
                  <Choice label="Unassigned" hint="does nothing" onPick={() => assign({ type: "none" })} />
                  <Choice label="Disabled" hint="not reported at all" onPick={() => assign({ type: "disabled" })} />
                </>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center justify-between gap-3">
          <p className="text-xs text-muted-foreground">
            Only the buttons you change are written. A backup is saved first and the write is verified by reading it back.
          </p>
          <div className="flex shrink-0 gap-2">
            <Button variant="ghost" size="lg" onClick={() => setStaged({})} disabled={saving || stagedCount === 0}>
              Discard
            </Button>
            <Button size="lg" onClick={save} disabled={saving || stagedCount === 0}>
            <Save /> {saving ? "Writing to mouse…" : stagedCount ? `Save ${stagedCount} change${stagedCount > 1 ? "s" : ""} to mouse` : "Save to mouse"}
            </Button>
          </div>
        </div>
    </div>
  );
}
