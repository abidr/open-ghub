import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Download, HardDrive, Pencil, RefreshCw, Upload } from "lucide-react";
import { toast } from "sonner";

import { OnboardSlotEditor } from "@/components/onboard-slot-editor";
import { PanelSection } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { api, type DeviceSummary, type OnboardButton, type OnboardProfile, type OnboardSnapshot, type SlotSettings } from "@/lib/api";
import { describeRaw } from "@/lib/bindings";
import { cn } from "@/lib/utils";

/** Buttons that do something other than their own mouse button. */
function remapped(buttons: OnboardButton[]): OnboardButton[] {
  return buttons.filter((b, i) => b.description !== `Mouse button ${i + 1}`);
}

function ProfileCard({ profile, active, onEdit }: { profile: OnboardProfile; active: boolean; onEdit?: () => void }) {
  const changed = remapped(profile.buttons);
  const gshift = profile.gshift_buttons.filter((b) => b.kind !== "disabled");
  return (
    <div className={cn("rounded-lg bg-secondary/60 px-3.5 py-3 text-[13px]", !profile.enabled && "opacity-60")}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-semibold">
          {profile.rom ? "Factory default" : `Slot ${profile.slot}`}
          {profile.name ? ` · ${profile.name}` : ""}
        </span>
        {active && <Badge>Active</Badge>}
        {!profile.enabled && <Badge variant="secondary">Disabled</Badge>}
        {profile.crc_valid === false && <Badge variant="destructive">Checksum error</Badge>}
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">{profile.report_rate_hz} Hz</span>
        {onEdit && (
          <Button variant="ghost" size="icon-sm" aria-label={`Edit slot ${profile.slot}`} title="DPI and report rate" onClick={onEdit}>
            <Pencil />
          </Button>
        )}
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {profile.dpi_levels.map((dpi, i) => (
          <span
            key={i}
            title={i === profile.default_dpi_index ? "Starting DPI" : undefined}
            className={cn(
              "rounded-md px-2 py-0.5 text-xs font-semibold tabular-nums",
              i === profile.default_dpi_index ? "bg-primary text-primary-foreground" : "bg-background/60 text-muted-foreground",
            )}
          >
            {dpi}
          </span>
        ))}
      </div>
      <div className="mt-2 text-xs text-muted-foreground">
        {changed.length === 0 ? (
          "Buttons: standard"
        ) : (
          <>Buttons (edit them under Assignments): {changed.map((b) => `${profile.buttons.indexOf(b) + 1} → ${describeRaw(b.raw)}`).join(", ")}</>
        )}
        {gshift.length > 0 && <> · G-Shift: {gshift.length} assigned</>}
      </div>
    </div>
  );
}

/** Profiles stored in the mouse: view, edit DPI/report rate per slot, back up and restore. */
export function OnboardSection({ device, supportedDpi }: { device: DeviceSummary; supportedDpi: number[] }) {
  const [snapshot, setSnapshot] = useState<OnboardSnapshot | null>(null);
  const [reading, setReading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [editing, setEditing] = useState<OnboardProfile | null>(null);
  const [restorePath, setRestorePath] = useState<string | null>(null);
  const [restoring, setRestoring] = useState(false);

  const writeSlot = async (settings: SlotSettings) => {
    try {
      const result = await api.writeOnboardSlot(device.id, settings);
      setSnapshot(result.snapshot);
      if (result.report.unchanged) {
        toast.info(`Slot ${settings.slot} already has these settings`);
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

  const pickRestore = async () => {
    const path = await open({
      title: "Restore on-board memory from backup",
      multiple: false,
      directory: false,
      filters: [{ name: "Backup", extensions: ["json"] }],
    });
    if (typeof path === "string") setRestorePath(path);
  };

  const restore = async () => {
    if (!restorePath) return;
    setRestoring(true);
    try {
      const result = await api.restoreOnboardBackup(device.id, restorePath);
      setSnapshot(result.snapshot);
      toast.success(
        result.sectors_written === 0 ? "The mouse already matches this backup" : `Restored ${result.sectors_written} sector(s) from the backup`,
        { description: `The previous state was backed up to ${result.backup_path}` },
      );
      setRestorePath(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRestoring(false);
    }
  };

  const read = async () => {
    setReading(true);
    try {
      setSnapshot(await api.onboardSnapshot(device.id));
    } catch (e) {
      toast.error(`Could not read on-board memory: ${e}`);
    } finally {
      setReading(false);
    }
  };

  const backup = async () => {
    const date = new Date().toISOString().slice(0, 10);
    const path = await save({
      title: "Save on-board memory backup",
      defaultPath: `open-ghub-onboard-${device.name.replace(/[^\w-]+/g, "_")}-${date}.json`,
      filters: [{ name: "Backup", extensions: ["json"] }],
    });
    if (!path) return;
    setSaving(true);
    try {
      await api.saveOnboardBackup(device.id, path);
      toast.success("On-board memory backed up");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  const info = snapshot?.info;
  return (
    <PanelSection
      title="On-board memory"
      help="Profiles stored inside the mouse, used in on-board memory mode even with Open GHub closed. Reading never changes the mouse; every save is backed up first."
    >
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" onClick={read} disabled={reading}>
          <RefreshCw className={cn(reading && "animate-spin")} /> {snapshot ? "Read again" : "Read from mouse"}
        </Button>
        <Button variant="ghost" onClick={backup} disabled={saving}>
          <Download /> {saving ? "Saving…" : "Save backup…"}
        </Button>
        <Button variant="ghost" onClick={pickRestore} disabled={restoring}>
          <Upload /> Restore from backup…
        </Button>
      </div>

      {snapshot && info && (
        <div className="mt-4 grid gap-2">
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <HardDrive className="size-3.5" />
            {info.profile_count} profile slots · {info.button_count} buttons · format {info.profile_format} ·{" "}
            {info.sector_count} sectors of {info.sector_size} bytes ·{" "}
            {snapshot.active_profile === 0 ? "none active (software mode)" : `slot ${snapshot.active_profile} active`}
          </p>
          {!snapshot.directory_crc_valid && (
            <p className="text-xs text-destructive">The profile directory failed its checksum; the mouse is using its factory profile.</p>
          )}
          {snapshot.profiles.map((p) => (
            <ProfileCard
              key={`u${p.slot}`}
              profile={p}
              active={snapshot.active_profile === p.slot}
              onEdit={p.crc_valid ? () => setEditing(p) : undefined}
            />
          ))}
          {snapshot.rom_profiles.map((p) => (
            <ProfileCard key={`r${p.slot}`} profile={p} active={false} />
          ))}
        </div>
      )}

      <OnboardSlotEditor profile={editing} supportedDpi={supportedDpi} onClose={() => setEditing(null)} onSave={writeSlot} />

      <Dialog open={restorePath !== null} onOpenChange={(o) => !o && setRestorePath(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Restore on-board memory?</DialogTitle>
            <DialogDescription>
              The mouse's profile directory and user profiles will be replaced with the ones in this backup. The factory
              profile is never touched, and the current memory is backed up first.
            </DialogDescription>
          </DialogHeader>
          <p className="font-mono text-xs break-all text-muted-foreground">{restorePath}</p>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setRestorePath(null)}>
              Cancel
            </Button>
            <Button onClick={restore} disabled={restoring}>
              <Upload /> {restoring ? "Restoring…" : "Restore"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </PanelSection>
  );
}
