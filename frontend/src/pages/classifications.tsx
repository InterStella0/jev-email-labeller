import { useEffect, useMemo, useState } from "react"
import {
  CheckIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  InboxIcon,
  InfoIcon,
  MoreHorizontalIcon,
  RefreshCwIcon,
  SearchIcon,
  TagIcon,
  Trash2Icon,
} from "lucide-react"
import { useSearchParams } from "react-router"
import { toast } from "sonner"

import { ConfirmDialog } from "@/components/confirm-dialog"
import { EmptyState, LabelBadge, PageHeader, ProbabilityBar, ProbabilityList, StatusBadge } from "@/components/common"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { useAccounts } from "@/hooks/accounts"
import {
  useApplyClassification,
  useClassifications,
  useDeleteClassification,
  useOverrideClassification,
  useReclassify,
} from "@/hooks/classifications"
import { useLabels } from "@/hooks/labels"
import { actionLabels, dateTime, percent, relativeTime, statusLabels } from "@/lib/format"
import type { Classification, Label, Status } from "@/lib/types"

const ALL = "all"
const PER_PAGE = 50

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), ms)
    return () => clearTimeout(t)
  }, [value, ms])
  return debounced
}

function FilterSelect({
  value,
  onChange,
  placeholder,
  options,
}: {
  value: string | null
  onChange: (v: string | null) => void
  placeholder: string
  options: { value: string; label: string }[]
}) {
  return (
    <Select value={value ?? ALL} onValueChange={(v) => onChange(v === ALL ? null : v)}>
      <SelectTrigger className="w-full sm:w-44">
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={ALL}>{placeholder}</SelectItem>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function DetailsDialog({
  item,
  labels,
  onOpenChange,
}: {
  item: Classification | null
  labels: Label[]
  onOpenChange: (open: boolean) => void
}) {
  const colors = useMemo(() => new Map(labels.map((l) => [l.name.toLowerCase(), l.color])), [labels])
  return (
    <Dialog open={!!item} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        {item && (
          <>
            <DialogHeader>
              <DialogTitle className="pr-6">{item.subject || "(no subject)"}</DialogTitle>
              <DialogDescription>{item.sender}</DialogDescription>
            </DialogHeader>
            <div className="max-h-[65vh] space-y-5 overflow-y-auto pr-1 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <StatusBadge status={item.status} />
                <LabelBadge name={item.label_name} color={item.label_color} />
                {item.applied_label ? (
                  <span className="text-muted-foreground">
                    In mailbox as <b>{item.applied_label}</b>
                  </span>
                ) : (
                  <span className="text-muted-foreground">Mailbox not changed</span>
                )}
              </div>
              {item.error && (
                <p className="rounded-lg border border-red-200 bg-red-50 p-3 text-red-800">{item.error}</p>
              )}
              {item.snippet && <p className="rounded-lg bg-muted p-3 text-muted-foreground">{item.snippet}</p>}
              <div className="space-y-2">
                <h4 className="font-medium">Jev probabilities</h4>
                <ProbabilityList probabilities={item.probabilities} colors={colors} highlight={item.top_label} />
              </div>
              {item.actions_taken.length > 0 && (
                <div className="space-y-2">
                  <h4 className="font-medium">Rule actions</h4>
                  <ul className="space-y-1">
                    {item.actions_taken.map((a, i) => (
                      <li key={i} className="flex items-center gap-2">
                        <Badge variant={a.done ? "secondary" : "outline"}>{a.done ? "done" : "planned"}</Badge>
                        {actionLabels[a.action]}
                        {a.folder && <span className="text-muted-foreground">→ {a.folder}</span>}
                        {a.error && <span className="text-destructive">{a.error}</span>}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              <dl className="grid grid-cols-[8rem_1fr] gap-x-4 gap-y-1 text-xs text-muted-foreground">
                <dt>Account</dt>
                <dd>{item.account_name}</dd>
                <dt>Received</dt>
                <dd>{dateTime(item.received_at)}</dd>
                <dt>Classified</dt>
                <dd>{dateTime(item.created_at)}</dd>
                <dt>Confidence</dt>
                <dd>{percent(item.confidence)}</dd>
                <dt>Model</dt>
                <dd>{item.model ?? "—"}</dd>
                <dt>Message id</dt>
                <dd className="break-all font-mono">{item.provider_msg_id}</dd>
              </dl>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

function OverrideDialog({
  item,
  labels,
  onOpenChange,
}: {
  item: Classification | null
  labels: Label[]
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={!!item} onOpenChange={onOpenChange}>
      <DialogContent>
        {item && <OverrideForm key={item.id} item={item} labels={labels} onDone={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  )
}

function OverrideForm({ item, labels, onDone }: { item: Classification; labels: Label[]; onDone: () => void }) {
  const [labelId, setLabelId] = useState(item.label_id ? String(item.label_id) : "")
  const [applyToMailbox, setApplyToMailbox] = useState(true)
  const override = useOverrideClassification()

  function submit() {
    if (!labelId) return
    override.mutate(
      { id: item.id, label_id: Number(labelId), apply_to_mailbox: applyToMailbox },
      {
        onSuccess: () => {
          toast.success("Label changed")
          onDone()
        },
      },
    )
  }

  return (
    <>
        <DialogHeader>
          <DialogTitle>Change label</DialogTitle>
          <DialogDescription className="truncate">{item.subject}</DialogDescription>
        </DialogHeader>
        <FieldGroup>
          <Field>
            <FieldLabel>Label</FieldLabel>
            <Select value={labelId} onValueChange={setLabelId}>
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Choose a label" />
              </SelectTrigger>
              <SelectContent>
                {labels.map((l) => (
                  <SelectItem key={l.id} value={String(l.id)}>
                    {l.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field orientation="horizontal">
            <Switch id="apply-mailbox" checked={applyToMailbox} onCheckedChange={setApplyToMailbox} />
            <div className="space-y-0.5">
              <FieldLabel htmlFor="apply-mailbox">Also update the mailbox</FieldLabel>
              <FieldDescription>
                Removes {item.applied_label ? <b>{item.applied_label}</b> : "the old label"} and applies the new one.
                This is a manual action, so it runs even in dry-run mode.
              </FieldDescription>
            </div>
          </Field>
        </FieldGroup>
        <DialogFooter>
          <Button variant="outline" onClick={onDone}>
            Cancel
          </Button>
          <Button onClick={submit} disabled={!labelId || override.isPending}>
            Save
          </Button>
        </DialogFooter>
    </>
  )
}

export default function ClassificationsPage() {
  const [params, setParams] = useSearchParams()
  const [search, setSearch] = useState(params.get("q") ?? "")
  const debouncedSearch = useDebounced(search, 300)

  const filters = {
    account_id: params.get("account_id") ? Number(params.get("account_id")) : undefined,
    label_id: params.get("label_id") ? Number(params.get("label_id")) : undefined,
    status: (params.get("status") as Status | null) ?? undefined,
    q: params.get("q") ?? undefined,
    page: Number(params.get("page") ?? 1),
    per_page: PER_PAGE,
  }

  const { data, isPending, isFetching } = useClassifications(filters)
  const { data: labels = [] } = useLabels()
  const { data: accounts = [] } = useAccounts()
  const apply = useApplyClassification()
  const reclassify = useReclassify()
  const remove = useDeleteClassification()

  const [details, setDetails] = useState<Classification | null>(null)
  const [overriding, setOverriding] = useState<Classification | null>(null)
  const [deleting, setDeleting] = useState<Classification | null>(null)

  function setParam(key: string, value: string | null) {
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev)
        if (value) next.set(key, value)
        else next.delete(key)
        if (key !== "page") next.delete("page")
        return next
      },
      { replace: true },
    )
  }

  useEffect(() => {
    if ((params.get("q") ?? "") !== debouncedSearch) setParam("q", debouncedSearch || null)
    // Only react to the debounced text; params/setParam change on every navigation.
  }, [debouncedSearch]) // oxlint-disable-line react-hooks/exhaustive-deps

  const totalPages = data ? Math.max(1, Math.ceil(data.total / data.per_page)) : 1
  const colors = useMemo(() => new Map(labels.map((l) => [l.name.toLowerCase(), l.color])), [labels])

  return (
    <div className="space-y-6">
      <PageHeader
        title="Classifications"
        description="Every message Jev has seen. Correct a label, apply a dry-run decision, or ask Jev again."
      />

      <div className="flex flex-col gap-2 sm:flex-row sm:flex-wrap">
        <div className="relative flex-1 sm:min-w-64">
          <SearchIcon className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-8"
            placeholder="Search subject, sender or text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <FilterSelect
          value={params.get("label_id")}
          onChange={(v) => setParam("label_id", v)}
          placeholder="All labels"
          options={labels.map((l) => ({ value: String(l.id), label: l.name }))}
        />
        <FilterSelect
          value={params.get("status")}
          onChange={(v) => setParam("status", v)}
          placeholder="All statuses"
          options={Object.entries(statusLabels).map(([value, label]) => ({ value, label }))}
        />
        <FilterSelect
          value={params.get("account_id")}
          onChange={(v) => setParam("account_id", v)}
          placeholder="All accounts"
          options={accounts.map((a) => ({ value: String(a.id), label: a.name }))}
        />
      </div>

      <Card className="py-0">
        <CardContent className="px-0">
          {isPending ? (
            <div className="space-y-2 p-4">
              {Array.from({ length: 6 }).map((_, i) => (
                <Skeleton key={i} className="h-12 w-full" />
              ))}
            </div>
          ) : data && data.items.length === 0 ? (
            <div className="p-4">
              <EmptyState icon={<InboxIcon />} title="No classifications">
                Nothing matches these filters yet. New mail shows up here after the next poll.
              </EmptyState>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Message</TableHead>
                  <TableHead>Label</TableHead>
                  <TableHead className="w-44">Jev</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>When</TableHead>
                  <TableHead className="w-12 pr-4" />
                </TableRow>
              </TableHeader>
              <TableBody className={isFetching ? "opacity-70" : undefined}>
                {data?.items.map((c) => (
                  <TableRow key={c.id} className="cursor-pointer" onClick={() => setDetails(c)}>
                    <TableCell className="max-w-md pl-4">
                      <div className="truncate font-medium">{c.subject || "(no subject)"}</div>
                      <div className="truncate text-xs text-muted-foreground">
                        {c.sender}
                        {c.account_name && <span> · {c.account_name}</span>}
                      </div>
                    </TableCell>
                    <TableCell>
                      <LabelBadge name={c.label_name} color={c.label_color} />
                    </TableCell>
                    <TableCell>
                      <div className="space-y-1">
                        <div className="flex justify-between text-xs">
                          <span className="truncate text-muted-foreground">{c.top_label ?? "—"}</span>
                          <span className="tabular-nums">{percent(c.top_probability)}</span>
                        </div>
                        <ProbabilityBar
                          value={c.top_probability ?? 0}
                          color={c.top_label ? colors.get(c.top_label.toLowerCase()) : undefined}
                        />
                      </div>
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={c.status} />
                    </TableCell>
                    <TableCell className="text-muted-foreground" title={dateTime(c.created_at)}>
                      {relativeTime(c.created_at)}
                    </TableCell>
                    <TableCell className="pr-4" onClick={(e) => e.stopPropagation()}>
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button variant="ghost" size="icon-sm" aria-label="Actions">
                            <MoreHorizontalIcon />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem onSelect={() => setDetails(c)}>
                            <InfoIcon /> Details
                          </DropdownMenuItem>
                          <DropdownMenuItem onSelect={() => setOverriding(c)}>
                            <TagIcon /> Change label…
                          </DropdownMenuItem>
                          {(c.status === "dry_run" || c.status === "error") && (
                            <DropdownMenuItem
                              onSelect={() =>
                                apply.mutate(c.id, { onSuccess: () => toast.success("Applied to the mailbox") })
                              }
                            >
                              <CheckIcon /> Apply to mailbox
                            </DropdownMenuItem>
                          )}
                          <DropdownMenuItem
                            onSelect={() =>
                              toast.promise(reclassify.mutateAsync(c.id), {
                                loading: "Asking Jev again…",
                                success: (r) => `Now: ${r.label_name ?? "—"} (${percent(r.top_probability)})`,
                                error: (e: Error) => e.message,
                              })
                            }
                          >
                            <RefreshCwIcon /> Re-classify
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem variant="destructive" onSelect={() => setDeleting(c)}>
                            <Trash2Icon /> Forget record
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {data && data.total > 0 && (
        <div className="flex items-center justify-between text-sm text-muted-foreground">
          <span>
            {data.total} message{data.total === 1 ? "" : "s"}
          </span>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="icon-sm"
              disabled={filters.page <= 1}
              onClick={() => setParam("page", String(filters.page - 1))}
              aria-label="Previous page"
            >
              <ChevronLeftIcon />
            </Button>
            <span className="tabular-nums">
              {filters.page} / {totalPages}
            </span>
            <Button
              variant="outline"
              size="icon-sm"
              disabled={filters.page >= totalPages}
              onClick={() => setParam("page", String(filters.page + 1))}
              aria-label="Next page"
            >
              <ChevronRightIcon />
            </Button>
          </div>
        </div>
      )}

      <DetailsDialog item={details} labels={labels} onOpenChange={(o) => !o && setDetails(null)} />
      <OverrideDialog item={overriding} labels={labels} onOpenChange={(o) => !o && setOverriding(null)} />
      <ConfirmDialog
        open={!!deleting}
        onOpenChange={(o) => !o && setDeleting(null)}
        title="Forget this record?"
        description="The record is removed from this app. The email and any label already in your mailbox stay exactly as they are."
        confirmLabel="Forget"
        onConfirm={() => deleting && remove.mutate(deleting.id, { onSuccess: () => toast.success("Record removed") })}
      />
    </div>
  )
}
