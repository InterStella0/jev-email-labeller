import { useEffect, useState } from "react"
import { zodResolver } from "@hookform/resolvers/zod"
import { LockIcon, PencilIcon, PlusIcon, TagIcon, Trash2Icon } from "lucide-react"
import { Controller, useForm, useWatch } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

import { ConfirmDialog } from "@/components/confirm-dialog"
import { ColorDot, EmptyState, PageHeader } from "@/components/common"
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
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Slider } from "@/components/ui/slider"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { useCreateLabel, useDeleteLabel, useLabels, useUpdateLabel } from "@/hooks/labels"
import { useRules } from "@/hooks/rules"
import { useSettings } from "@/hooks/settings"
import { percent } from "@/lib/format"
import type { Label } from "@/lib/types"

const SWATCHES = ["#ef4444", "#f97316", "#f59e0b", "#22c55e", "#14b8a6", "#0ea5e9", "#3b82f6", "#a855f7", "#ec4899", "#71717a"]

const schema = z.object({
  name: z.string().trim().min(1, "Name is required").max(64, "At most 64 characters"),
  description: z.string().max(1000, "At most 1000 characters"),
  color: z.string().regex(/^#[0-9a-fA-F]{6}$/, "Pick a colour"),
  customThreshold: z.boolean(),
  threshold: z.number().min(0).max(1),
  enabled: z.boolean(),
})
type FormValues = z.infer<typeof schema>

function LabelDialog({
  open,
  label,
  globalThreshold,
  onOpenChange,
}: {
  open: boolean
  label: Label | null
  globalThreshold: number
  onOpenChange: (open: boolean) => void
}) {
  const create = useCreateLabel()
  const update = useUpdateLabel()
  const isSystem = !!label?.is_system
  const form = useForm<FormValues>({ resolver: zodResolver(schema) })

  useEffect(() => {
    if (!open) return
    form.reset({
      name: label?.name ?? "",
      description: label?.description ?? "",
      color: label?.color ?? SWATCHES[6],
      customThreshold: label?.threshold != null,
      threshold: label?.threshold ?? globalThreshold,
      enabled: label?.enabled ?? true,
    })
  }, [open, label, globalThreshold, form])

  const customThreshold = useWatch({ control: form.control, name: "customThreshold" })
  const threshold = useWatch({ control: form.control, name: "threshold" })

  function submit(values: FormValues) {
    const input = {
      name: values.name,
      description: values.description,
      color: values.color,
      ...(isSystem ? {} : { threshold: values.customThreshold ? values.threshold : null, enabled: values.enabled }),
    }
    const done = {
      onSuccess: () => {
        toast.success(label ? "Label saved" : "Label created")
        onOpenChange(false)
      },
    }
    if (label) update.mutate({ id: label.id, ...input }, done)
    else create.mutate(input, done)
  }

  const errors = form.formState.errors

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{label ? "Edit label" : "New label"}</DialogTitle>
          <DialogDescription>
            {isSystem
              ? "Review is applied whenever Jev isn't confident enough. You can rename and recolour it."
              : "Jev reads the name and description to decide whether an email belongs here."}
          </DialogDescription>
        </DialogHeader>
        <form id="label-form" onSubmit={form.handleSubmit(submit)}>
          <FieldGroup>
            <Field data-invalid={!!errors.name}>
              <FieldLabel htmlFor="label-name">Name</FieldLabel>
              <Input id="label-name" {...form.register("name")} aria-invalid={!!errors.name} />
              <FieldError errors={[errors.name]} />
            </Field>
            <Field data-invalid={!!errors.description}>
              <FieldLabel htmlFor="label-description">Description for Jev</FieldLabel>
              <Textarea
                id="label-description"
                rows={3}
                placeholder="e.g. Invoices, receipts and payment confirmations from shops and services."
                {...form.register("description")}
              />
              <FieldDescription>Be specific. Describe what belongs here, not what doesn't.</FieldDescription>
              <FieldError errors={[errors.description]} />
            </Field>
            <Field>
              <FieldLabel>Colour</FieldLabel>
              <Controller
                control={form.control}
                name="color"
                render={({ field }) => (
                  <div className="flex flex-wrap items-center gap-2">
                    {SWATCHES.map((c) => (
                      <button
                        key={c}
                        type="button"
                        aria-label={c}
                        onClick={() => field.onChange(c)}
                        className="size-7 rounded-full ring-offset-2 ring-offset-background transition data-[active=true]:ring-2 data-[active=true]:ring-foreground"
                        data-active={field.value?.toLowerCase() === c}
                        style={{ background: c }}
                      />
                    ))}
                    <Input
                      type="color"
                      className="h-7 w-10 cursor-pointer p-0.5"
                      value={field.value ?? "#000000"}
                      onChange={(e) => field.onChange(e.target.value)}
                    />
                  </div>
                )}
              />
            </Field>
            {!isSystem && (
              <>
                <Field orientation="horizontal">
                  <Controller
                    control={form.control}
                    name="customThreshold"
                    render={({ field }) => (
                      <Switch id="custom-threshold" checked={field.value} onCheckedChange={field.onChange} />
                    )}
                  />
                  <div className="space-y-0.5">
                    <FieldLabel htmlFor="custom-threshold">Custom threshold</FieldLabel>
                    <FieldDescription>
                      Otherwise the global threshold ({percent(globalThreshold)}) applies.
                    </FieldDescription>
                  </div>
                </Field>
                {customThreshold && (
                  <Field>
                    <div className="flex items-center justify-between">
                      <FieldLabel>Apply when Jev is at least</FieldLabel>
                      <span className="text-sm font-medium tabular-nums">{percent(threshold)}</span>
                    </div>
                    <Controller
                      control={form.control}
                      name="threshold"
                      render={({ field }) => (
                        <Slider
                          min={0.5}
                          max={1}
                          step={0.01}
                          value={[field.value]}
                          onValueChange={([v]) => field.onChange(v)}
                        />
                      )}
                    />
                    <FieldDescription>Below this, the email gets the Review label instead.</FieldDescription>
                  </Field>
                )}
                <Field orientation="horizontal">
                  <Controller
                    control={form.control}
                    name="enabled"
                    render={({ field }) => (
                      <Switch id="label-enabled" checked={field.value} onCheckedChange={field.onChange} />
                    )}
                  />
                  <FieldLabel htmlFor="label-enabled">Offer this label to Jev</FieldLabel>
                </Field>
              </>
            )}
          </FieldGroup>
        </form>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button type="submit" form="label-form" disabled={create.isPending || update.isPending}>
            {label ? "Save" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export default function LabelsPage() {
  const { data: labels, isPending } = useLabels()
  const { data: rules = [] } = useRules()
  const { data: settings } = useSettings()
  const update = useUpdateLabel()
  const remove = useDeleteLabel()

  const [editing, setEditing] = useState<Label | null>(null)
  const [creating, setCreating] = useState(false)
  const [deleting, setDeleting] = useState<Label | null>(null)
  const globalThreshold = settings?.threshold ?? 0.7

  return (
    <div className="space-y-6">
      <PageHeader
        title="Labels"
        description="The categories Jev chooses between. Deleting a label here never removes it from your mailbox."
        actions={
          <Button onClick={() => setCreating(true)}>
            <PlusIcon /> New label
          </Button>
        }
      />

      <Card className="py-0">
        <CardContent className="px-0">
          {isPending ? (
            <div className="space-y-2 p-4">
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          ) : labels?.length === 0 ? (
            <div className="p-4">
              <EmptyState icon={<TagIcon />} title="No labels" />
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Label</TableHead>
                  <TableHead>Description</TableHead>
                  <TableHead>Threshold</TableHead>
                  <TableHead>Rules</TableHead>
                  <TableHead>Enabled</TableHead>
                  <TableHead className="w-24 pr-4" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {labels?.map((l) => {
                  const ruleCount = rules.filter((r) => r.label_id === l.id).length
                  return (
                    <TableRow key={l.id} className={!l.enabled ? "opacity-60" : undefined}>
                      <TableCell className="pl-4">
                        <div className="flex items-center gap-2 font-medium">
                          <ColorDot color={l.color} />
                          {l.name}
                          {l.is_system && (
                            <Badge variant="secondary" className="gap-1">
                              <LockIcon className="size-3" /> System
                            </Badge>
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="max-w-md">
                        <p className="line-clamp-2 whitespace-normal text-muted-foreground">
                          {l.description || <i>No description</i>}
                        </p>
                      </TableCell>
                      <TableCell className="tabular-nums">
                        {l.is_system ? (
                          <span className="text-muted-foreground">Fallback</span>
                        ) : l.threshold != null ? (
                          percent(l.threshold)
                        ) : (
                          <span className="text-muted-foreground">Global ({percent(globalThreshold)})</span>
                        )}
                      </TableCell>
                      <TableCell className="tabular-nums">{l.is_system ? "—" : ruleCount}</TableCell>
                      <TableCell>
                        <Switch
                          checked={l.enabled}
                          disabled={l.is_system}
                          onCheckedChange={(enabled) => update.mutate({ id: l.id, enabled })}
                          aria-label={`Enable ${l.name}`}
                        />
                      </TableCell>
                      <TableCell className="pr-4 text-right">
                        <Button variant="ghost" size="icon-sm" onClick={() => setEditing(l)} aria-label="Edit">
                          <PencilIcon />
                        </Button>
                        {!l.is_system && (
                          <Button variant="ghost" size="icon-sm" onClick={() => setDeleting(l)} aria-label="Delete">
                            <Trash2Icon />
                          </Button>
                        )}
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <LabelDialog
        open={creating || !!editing}
        label={editing}
        globalThreshold={globalThreshold}
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
        title={`Delete “${deleting?.name}”?`}
        description="Its rules are deleted too, and past decisions lose their label reference. Labels already applied in your mailboxes are not touched."
        onConfirm={() => deleting && remove.mutate(deleting.id, { onSuccess: () => toast.success("Label deleted") })}
      />
    </div>
  )
}
