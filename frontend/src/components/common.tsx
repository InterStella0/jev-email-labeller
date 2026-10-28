import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

import { Badge } from "@/components/ui/badge"
import { statusLabels, percent } from "@/lib/format"
import type { Status } from "@/lib/types"

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string
  description?: ReactNode
  actions?: ReactNode
}) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description && <p className="max-w-2xl text-sm text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  )
}

export function ColorDot({ color, className }: { color: string; className?: string }) {
  return <span className={cn("inline-block size-2.5 shrink-0 rounded-full", className)} style={{ background: color }} />
}

export function LabelBadge({ name, color }: { name: string | null; color?: string | null }) {
  if (!name) return <span className="text-sm text-muted-foreground">—</span>
  return (
    <Badge variant="outline" className="gap-1.5 font-medium">
      <ColorDot color={color ?? "#64748b"} />
      {name}
    </Badge>
  )
}

const statusStyles: Record<Status, string> = {
  applied: "bg-emerald-50 text-emerald-700 border-emerald-200",
  review: "bg-amber-50 text-amber-700 border-amber-200",
  dry_run: "bg-sky-50 text-sky-700 border-sky-200",
  overridden: "bg-violet-50 text-violet-700 border-violet-200",
  error: "bg-red-50 text-red-700 border-red-200",
}

export function StatusBadge({ status }: { status: Status }) {
  return (
    <Badge variant="outline" className={cn("font-medium", statusStyles[status])}>
      {statusLabels[status]}
    </Badge>
  )
}

export function ProbabilityBar({
  value,
  threshold,
  color,
  className,
}: {
  value: number
  threshold?: number
  color?: string
  className?: string
}) {
  return (
    <div className={cn("relative h-2 w-full overflow-hidden rounded-full bg-muted", className)}>
      <div
        className="h-full rounded-full bg-primary transition-[width]"
        style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%`, background: color }}
      />
      {threshold != null && (
        <div
          className="absolute inset-y-0 w-0.5 bg-foreground/60"
          style={{ left: `calc(${threshold * 100}% - 1px)` }}
          title={`Threshold ${percent(threshold)}`}
        />
      )}
    </div>
  )
}

/** Sorted per-label probability rows, used by the details dialog and the playground. */
export function ProbabilityList({
  probabilities,
  colors,
  highlight,
}: {
  probabilities: Record<string, number>
  colors: Map<string, string>
  highlight?: string | null
}) {
  const rows = Object.entries(probabilities).sort((a, b) => b[1] - a[1])
  if (rows.length === 0) return <p className="text-sm text-muted-foreground">No probabilities recorded.</p>
  return (
    <div className="space-y-2.5">
      {rows.map(([name, p]) => (
        <div key={name} className="grid grid-cols-[minmax(0,9rem)_1fr_3.5rem] items-center gap-3 text-sm">
          <span className={cn("flex items-center gap-2 truncate", name === highlight && "font-semibold")}>
            <ColorDot color={colors.get(name.toLowerCase()) ?? "#94a3b8"} />
            <span className="truncate">{name}</span>
          </span>
          <ProbabilityBar value={p} color={colors.get(name.toLowerCase())} />
          <span className="text-right tabular-nums text-muted-foreground">{percent(p, 1)}</span>
        </div>
      ))}
    </div>
  )
}

export function EmptyState({ icon, title, children }: { icon: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-xl border border-dashed px-6 py-12 text-center">
      <div className="text-muted-foreground [&_svg]:size-8">{icon}</div>
      <p className="font-medium">{title}</p>
      {children && <div className="max-w-md text-sm text-muted-foreground">{children}</div>}
    </div>
  )
}
