import type { ReactNode } from "react";
import { TriangleAlert } from "lucide-react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";

/** Slide-out side panel used for each device feature (G HUB's left canvas). */
export function PanelShell({
  title,
  aside,
  toolbar,
  wide,
  children,
}: {
  title: string;
  aside?: string;
  toolbar?: ReactNode;
  /** For panels with a two-column layout. */
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <aside
      className={cn(
        "flex h-full min-h-0 flex-col border-r border-border bg-sidebar duration-200 animate-in fade-in slide-in-from-left-4",
        wide ? "w-[min(880px,66vw)]" : "w-[min(560px,46vw)]",
      )}
    >
      <div className="flex items-baseline justify-between px-7 pt-5.5 pb-2.5">
        <h2 className="text-[22px] font-extrabold tracking-tight uppercase">{title}</h2>
        {aside && <span className="text-xs text-muted-foreground tabular-nums">{aside}</span>}
      </div>
      {toolbar && <div className="flex items-center gap-2 px-5.5 pb-3">{toolbar}</div>}
      <Separator className="mx-7 w-auto!" />
      <ScrollArea className="min-h-0 flex-1">
        <div className="pb-8">{children}</div>
      </ScrollArea>
    </aside>
  );
}

export function PanelSection({ title, help, children }: { title?: string; help?: string; children: ReactNode }) {
  return (
    <section className="px-7 pt-5">
      {title && <h3 className="mb-1 text-[13px] font-bold tracking-widest uppercase">{title}</h3>}
      {help && <p className="mb-3 text-[12.5px] text-muted-foreground">{help}</p>}
      {children}
    </section>
  );
}

export function StateErrors({ errors }: { errors: string[] }) {
  if (!errors.length) return null;
  return (
    <div role="alert" className="mx-7 mt-3 flex gap-2.5 rounded-md border-l-3 border-destructive bg-destructive/10 px-3 py-2.5 text-[12.5px] text-[#ffb48a]">
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div>
        Some settings could not be read:
        <ul className="mt-1 list-disc pl-4">
          {errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}
