import { useCallback, useEffect, useState } from "react";
import { TriangleAlert } from "lucide-react";
import { toast } from "sonner";

import { ButtonAssignments } from "@/components/onboard-buttons-editor";
import { PanelSection, PanelShell } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api, type BlockedKeyboard, type DeviceState, type DeviceSummary, type OnboardSnapshot, type SlotSettings } from "@/lib/api";
import { cn } from "@/lib/utils";

interface Props {
  device: DeviceSummary;
  state: DeviceState;
  onModeChange: (host: boolean) => void;
}

/**
 * Button assignments, stored in the mouse's on-board profiles. Reads the
 * mouse when opened; every save is backed up and verified by reading back.
 */
export function AssignmentsPanel({ device, state, onModeChange }: Props) {
  const [snapshot, setSnapshot] = useState<OnboardSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [slot, setSlot] = useState<number | null>(null);
  const [blocked, setBlocked] = useState<BlockedKeyboard[]>([]);

  const read = useCallback(async () => {
    try {
      const snap = await api.onboardSnapshot(device.id);
      setSnapshot(snap);
      setError(null);
      setSlot((cur) => cur ?? (snap.active_profile || snap.profiles[0]?.slot || null));
    } catch (e) {
      setError(String(e));
    }
  }, [device.id]);

  // Keystroke bindings arrive through the receiver's keyboard interface; if
  // Windows has it disabled they silently do nothing. Checked on open and
  // whenever the window regains focus (e.g. after fixing it in Device Manager).
  useEffect(() => {
    const check = () =>
      api
        .blockedKeyboards(device.product_id)
        .then(setBlocked)
        .catch(() => setBlocked([]));
    void check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [device.product_id]);

  // Re-read when the mode flips, so "active slot" is current.
  useEffect(() => {
    void read();
  }, [read, state.onboard_mode]);

  const save = async (settings: SlotSettings) => {
    try {
      const result = await api.writeOnboardSlot(device.id, settings);
      setSnapshot(result.snapshot);
      if (result.report.unchanged) {
        toast.info(`Slot ${settings.slot} already has these assignments`);
      } else {
        toast.success(`Slot ${settings.slot} saved to the mouse`, {
          description: `Verified by reading it back. Backup: ${result.backup_path}`,
        });
      }
      return true;
    } catch (e) {
      toast.error(String(e));
      return false;
    }
  };

  const profile = snapshot?.profiles.find((p) => p.slot === slot) ?? null;
  const host = state.onboard_mode === "host";

  return (
    <PanelShell title="Assignments" aside={device.name} wide>
      {host && (
        <div role="status" className="mx-7 mt-4 flex items-start gap-3 rounded-md border-l-3 border-amber-500 bg-amber-500/10 px-3 py-2.5 text-[12.5px]">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-amber-500" />
          <div className="grid gap-2">
            <span>
              The mouse is under software control, so it ignores the assignments stored on it. Switch to on-board memory for them to take
              effect.
            </span>
            <Button size="sm" className="w-fit" onClick={() => onModeChange(false)}>
              Switch to on-board memory
            </Button>
          </div>
        </div>
      )}

      {blocked.length > 0 && (
        <div role="alert" className="mx-7 mt-4 flex items-start gap-3 rounded-md border-l-3 border-destructive bg-destructive/10 px-3 py-2.5 text-[12.5px]">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
          <div className="grid gap-1.5">
            <span>
              Windows has the keyboard interface of this mouse's receiver (or cable){" "}
              {blocked.every((b) => b.problem_code === 22) ? "disabled" : "stopped"}, so assignments that send keys (shortcuts like Ctrl+C)
              won't do anything. Mouse buttons and functions still work. Enable it in Device Manager under Keyboards (the "HID Keyboard
              Device" with a warning icon), or from an administrator prompt:
            </span>
            {blocked.map((b) => (
              <code key={b.instance_id} className="font-mono text-[11.5px] break-all select-text">
                pnputil /enable-device "{b.instance_id}"
              </code>
            ))}
          </div>
        </div>
      )}

      {error && (
        <p role="alert" className="mx-7 mt-4 text-[12.5px] text-destructive">
          Could not read on-board memory: {error}
        </p>
      )}

      {snapshot && (
        <PanelSection>
          <div className="mb-4 flex flex-wrap items-center gap-2">
            {snapshot.profiles.map((p) => (
              <button
                key={p.slot}
                type="button"
                disabled={!p.crc_valid}
                onClick={() => setSlot(p.slot)}
                className={cn(
                  "flex items-center gap-2 rounded-lg border px-3 py-1.5 text-left text-[13px] font-semibold disabled:opacity-50",
                  slot === p.slot ? "border-primary bg-secondary" : "border-transparent bg-secondary/50 hover:bg-secondary",
                )}
              >
                Slot {p.slot}
                {p.name ? <span className="font-normal text-muted-foreground">· {p.name}</span> : null}
                {snapshot.active_profile === p.slot && <Badge>Active</Badge>}
              </button>
            ))}
          </div>
          {profile ? (
            <ButtonAssignments profile={profile} onSave={save} />
          ) : (
            <p className="text-[12.5px] text-muted-foreground">This mouse has no editable on-board profile.</p>
          )}
        </PanelSection>
      )}
    </PanelShell>
  );
}
