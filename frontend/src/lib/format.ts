import type { Provider, RuleAction, Status } from "@/lib/types"

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" })
const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31536000],
  ["month", 2592000],
  ["week", 604800],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
  ["second", 1],
]

export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return "never"
  const seconds = (new Date(iso).getTime() - Date.now()) / 1000
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size || unit === "second") {
      return rtf.format(Math.round(seconds / size), unit)
    }
  }
  return iso
}

export function dateTime(iso: string | null | undefined): string {
  return iso ? new Date(iso).toLocaleString() : "—"
}

export function percent(p: number | null | undefined, digits = 0): string {
  return p == null ? "—" : `${(p * 100).toFixed(digits)}%`
}

export const statusLabels: Record<Status, string> = {
  applied: "Applied",
  review: "Review",
  dry_run: "Dry run",
  overridden: "Overridden",
  error: "Error",
}

export const actionLabels: Record<RuleAction, string> = {
  archive: "Archive",
  mark_read: "Mark as read",
  move_to_folder: "Move to folder",
  star: "Star / flag",
}

export const providerLabels: Record<Provider, string> = {
  gmail: "Gmail",
  outlook: "Outlook",
  imap: "IMAP",
}
