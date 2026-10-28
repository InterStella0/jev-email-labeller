import { useEffect, useState } from "react"
import { zodResolver } from "@hookform/resolvers/zod"
import { PencilIcon, PlusIcon, ShieldCheckIcon, Trash2Icon, WorkflowIcon } from "lucide-react"
import { Controller, useForm, useWatch } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

import { ConfirmDialog } from "@/components/confirm-dialog"
import { EmptyState, LabelBadge, PageHeader } from "@/components/common"
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
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Slider } from "@/components/ui/slider"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { useLabels } from "@/hooks/labels"
import { useCreateRule, useDeleteRule, useRules, useUpdateRule } from "@/hooks/rules"
import { actionLabels, percent } from "@/lib/format"
import type { Rule, RuleAction } from "@/lib/types"

const MIN = 0.9

const schema = z
  .object({
    label_id: z.string().min(1, "Choose a label"),
    action: z.enum(["archive", "mark_read", "move_to_folder", "star"]),
    folder: z.string(),
    min_probability: z.number().min(MIN).max(1),
    enabled: z.boolean(),
  })
  .refine((v) => v.action !== "move_to_folder" || v.folder.trim().length > 0, {
    path: ["folder"],
    message: "Enter a folder",
  })
type FormValues = z.infer<typeof schema>

const actionHelp: Record<RuleAction, string> = {
  archive: "Gmail: removes it from the inbox. Outlook and IMAP: moves it to the archive folder.",
  mark_read: "Marks the message as read.",
  move_to_folder: "Moves the message to a folder (Gmail: adds that label and leaves the inbox).",
  star: "Stars (Gmail) or flags (Outlook, IMAP) the message.",
}

function RuleDialog({ open, rule, onOpenChange }: { open: boolean; rule: Rule | null; onOpenChange: (o: boolean) => void }) {
  const { data: labels = [] } = useLabels()
  const create = useCreateRule()
  const update = useUpdateRule()
  const form = useForm<FormValues>({ resolver: zodResolver(schema) })

  useEffect(() => {
    if (!open) return
    form.reset({
      label_id: rule ? String(rule.label_id) : "",
      action: rule?.action ?? "archive",
      folder: rule?.params.folder ?? "",
      min_probability: rule?.min_probability ?? MIN,
      enabled: rule?.enabled ?? true,
    })
  }, [open, rule, form])

  const action = useWatch({ control: form.control, name: "action" })
  const minProbability = useWatch({ control: form.control, name: "min_probability" })
  const errors = form.formState.errors

  function submit(v: FormValues) {
    const input = {
      label_id: Number(v.label_id),
      action: v.action,
      folder: v.action === "move_to_folder" ? v.folder.trim() : null,
      min_probability: v.min_probability,
      enabled: v.enabled,
    }
    const done = {
      onSuccess: () => {
        toast.success(rule ? "Rule saved" : "Rule created")
        onOpenChange(false)
      },
    }
    if (rule) update.mutate({ id: rule.id, ...input }, done)
    else create.mutate(input, done)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{rule ? "Edit rule" : "New rule"}</DialogTitle>
          <DialogDescription>Runs after the label is applied, only when Jev is very confident.</DialogDescription>
        </DialogHeader>
        <form id="rule-form" onSubmit={form.handleSubmit(submit)}>
          <FieldGroup>
            <Field data-invalid={!!errors.label_id}>
              <FieldLabel>When the label is</FieldLabel>
              <Controller
                control={form.control}
                name="label_id"
                render={({ field }) => (
                  <Select value={field.value} onValueChange={field.onChange}>
                    <SelectTrigger className="w-full" aria-invalid={!!errors.label_id}>
                      <SelectValue placeholder="Choose a label" />
                    </SelectTrigger>
                    <SelectContent>
                      {labels
                        .filter((l) => !l.is_system)
                        .map((l) => (
                          <SelectItem key={l.id} value={String(l.id)}>
                            {l.name}
                          </SelectItem>
                        ))}
                    </SelectContent>
                  </Select>
                )}
              />
              <FieldError errors={[errors.label_id]} />
            </Field>
            <Field>
              <FieldLabel>Do this</FieldLabel>
              <Controller
                control={form.control}
                name="action"
                render={({ field }) => (
                  <Select value={field.value} onValueChange={field.onChange}>
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {(Object.keys(actionLabels) as RuleAction[]).map((a) => (
                        <SelectItem key={a} value={a}>
                          {actionLabels[a]}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              />
              <FieldDescription>{action && actionHelp[action]}</FieldDescription>
            </Field>
            {action === "move_to_folder" && (
              <Field data-invalid={!!errors.folder}>
                <FieldLabel htmlFor="rule-folder">Folder</FieldLabel>
                <Input id="rule-folder" placeholder="e.g. Newsletters" {...form.register("folder")} />
                <FieldDescription>Created on first use if it doesn't exist.</FieldDescription>
                <FieldError errors={[errors.folder]} />
              </Field>
            )}
            <Field>
              <div className="flex items-center justify-between">
                <FieldLabel>Only if Jev is at least</FieldLabel>
                <span className="text-sm font-medium tabular-nums">{percent(minProbability)}</span>
              </div>
              <Controller
                control={form.control}
                name="min_probability"
                render={({ field }) => (
                  <Slider min={MIN} max={1} step={0.005} value={[field.value]} onValueChange={([v]) => field.onChange(v)} />
                )}
              />
              <FieldDescription>Never lower than 90%.</FieldDescription>
            </Field>
            <Field orientation="horizontal">
              <Controller
                control={form.control}
                name="enabled"
                render={({ field }) => <Switch id="rule-enabled" checked={field.value} onCheckedChange={field.onChange} />}
              />
              <FieldLabel htmlFor="rule-enabled">Enabled</FieldLabel>
            </Field>
          </FieldGroup>
        </form>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button type="submit" form="rule-form" disabled={create.isPending || update.isPending}>
            {rule ? "Save" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export default function RulesPage() {
  const { data: rules, isPending } = useRules()
  const { data: labels = [] } = useLabels()
  const update = useUpdateRule()
  const remove = useDeleteRule()

  const [editing, setEditing] = useState<Rule | null>(null)
  const [creating, setCreating] = useState(false)
  const [deleting, setDeleting] = useState<Rule | null>(null)
  const labelById = new Map(labels.map((l) => [l.id, l]))

  return (
    <div className="space-y-6">
      <PageHeader
        title="Rules"
        description="Optional actions that run after a label is applied."
        actions={
          <Button onClick={() => setCreating(true)}>
            <PlusIcon /> New rule
          </Button>
        }
      />

      <div className="flex items-start gap-3 rounded-xl border bg-muted/40 px-4 py-3 text-sm">
        <ShieldCheckIcon className="mt-0.5 size-4 shrink-0 text-emerald-600" />
        <p className="text-muted-foreground">
          Rules never run in dry-run mode or on the Review label. They need at least <b>90%</b> from Jev. At most one
          move or archive runs per message. <b>Nothing is ever deleted.</b>
        </p>
      </div>

      <Card className="py-0">
        <CardContent className="px-0">
          {isPending ? (
            <div className="p-4">
              <Skeleton className="h-10 w-full" />
            </div>
          ) : rules?.length === 0 ? (
            <div className="p-4">
              <EmptyState icon={<WorkflowIcon />} title="No rules yet">
                Labels are applied without any extra actions. Add a rule to, say, archive high-confidence promos.
              </EmptyState>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">When labelled</TableHead>
                  <TableHead>Action</TableHead>
                  <TableHead>Min. probability</TableHead>
                  <TableHead>Enabled</TableHead>
                  <TableHead className="w-24 pr-4" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {rules?.map((r) => {
                  const label = labelById.get(r.label_id)
                  return (
                    <TableRow key={r.id} className={!r.enabled ? "opacity-60" : undefined}>
                      <TableCell className="pl-4">
                        <LabelBadge name={label?.name ?? "?"} color={label?.color} />
                      </TableCell>
                      <TableCell>
                        {actionLabels[r.action]}
                        {r.params.folder && <span className="text-muted-foreground"> → {r.params.folder}</span>}
                      </TableCell>
                      <TableCell className="tabular-nums">≥ {percent(r.min_probability, 1)}</TableCell>
                      <TableCell>
                        <Switch
                          checked={r.enabled}
                          onCheckedChange={(enabled) => update.mutate({ id: r.id, enabled })}
                          aria-label="Enabled"
                        />
                      </TableCell>
                      <TableCell className="pr-4 text-right">
                        <Button variant="ghost" size="icon-sm" onClick={() => setEditing(r)} aria-label="Edit">
                          <PencilIcon />
                        </Button>
                        <Button variant="ghost" size="icon-sm" onClick={() => setDeleting(r)} aria-label="Delete">
                          <Trash2Icon />
                        </Button>
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <RuleDialog
        open={creating || !!editing}
        rule={editing}
        onOpenChange={(o) => {
          if (!o) {
            setCreating(false)
            setEditing(null)
          }
        }}
      />
      <ConfirmDialog
        open={!!deleting}
        onOpenChange={(o) => !o && setDeleting(null)}
        title="Delete this rule?"
        description="Future messages won't get this action. Messages already handled stay as they are."
        onConfirm={() => deleting && remove.mutate(deleting.id, { onSuccess: () => toast.success("Rule deleted") })}
      />
    </div>
  )
}
