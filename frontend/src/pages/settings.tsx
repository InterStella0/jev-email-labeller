import { useState } from "react"
import { CheckCircle2Icon, CopyIcon, XCircleIcon } from "lucide-react"
import { toast } from "sonner"

import { ConfirmDialog } from "@/components/confirm-dialog"
import { PageHeader } from "@/components/common"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Slider } from "@/components/ui/slider"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { useSettings, useUpdateSettings, type SettingsInput } from "@/hooks/settings"
import { percent } from "@/lib/format"
import type { Settings } from "@/lib/types"

function Configured({ ok }: { ok: boolean | undefined }) {
  return ok ? (
    <Badge variant="outline" className="gap-1 border-emerald-200 bg-emerald-50 text-emerald-700">
      <CheckCircle2Icon className="size-3" /> Configured
    </Badge>
  ) : (
    <Badge variant="outline" className="gap-1 text-muted-foreground">
      <XCircleIcon className="size-3" /> Not set
    </Badge>
  )
}

function CopyField({ value }: { value: string }) {
  return (
    <div className="flex gap-2">
      <Input readOnly value={value} className="font-mono text-xs" />
      <Button
        variant="outline"
        size="icon"
        type="button"
        aria-label="Copy"
        onClick={() => navigator.clipboard.writeText(value).then(() => toast.success("Copied"))}
      >
        <CopyIcon />
      </Button>
    </div>
  )
}

function editable(s: Settings): Required<SettingsInput> {
  return {
    threshold: s.threshold,
    body_chars: s.body_chars,
    dry_run: s.dry_run,
    max_per_poll: s.max_per_poll,
    instructions: s.instructions,
  }
}

export default function SettingsPage() {
  const { data: settings } = useSettings()
  if (!settings) return <Skeleton className="h-96 w-full" />
  // Re-mount the form (resetting the draft) whenever the saved values change.
  return <SettingsForm key={JSON.stringify(editable(settings))} settings={settings} />
}

function SettingsForm({ settings }: { settings: Settings }) {
  const save = useUpdateSettings()
  const [draft, setDraft] = useState(() => editable(settings))
  const [confirmLive, setConfirmLive] = useState(false)

  const set = <K extends keyof SettingsInput>(key: K, value: Required<SettingsInput>[K]) =>
    setDraft((d) => ({ ...d, [key]: value }))

  function submit() {
    save.mutate(draft, { onSuccess: () => toast.success("Settings saved") })
  }

  const dirty =
    draft.threshold !== settings.threshold ||
    draft.body_chars !== settings.body_chars ||
    draft.dry_run !== settings.dry_run ||
    draft.max_per_poll !== settings.max_per_poll ||
    draft.instructions !== settings.instructions

  return (
    <div className="space-y-6">
      <PageHeader
        title="Settings"
        description="Global behaviour. Labels can override the threshold, and accounts can add their own dry-run."
        actions={
          <Button onClick={submit} disabled={!dirty || save.isPending}>
            Save changes
          </Button>
        }
      />

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Decisions</CardTitle>
            <CardDescription>When Jev's answer turns into a label.</CardDescription>
          </CardHeader>
          <CardContent>
            <FieldGroup>
              <Field orientation="horizontal">
                <Switch
                  id="dry-run"
                  checked={draft.dry_run}
                  onCheckedChange={(v) => (v ? set("dry_run", true) : setConfirmLive(true))}
                />
                <div className="space-y-0.5">
                  <FieldLabel htmlFor="dry-run">Global dry-run</FieldLabel>
                  <FieldDescription>
                    While on, every account only records decisions. Accounts with their own dry-run switch stay in dry
                    run even when this is off.
                  </FieldDescription>
                </div>
              </Field>
              <Field>
                <div className="flex items-center justify-between">
                  <FieldLabel>Threshold</FieldLabel>
                  <span className="text-sm font-medium tabular-nums">{percent(draft.threshold)}</span>
                </div>
                <Slider
                  min={0.5}
                  max={1}
                  step={0.01}
                  value={[draft.threshold]}
                  onValueChange={([v]) => set("threshold", v)}
                />
                <FieldDescription>
                  Jev's top probability must reach this for the label to be applied. Otherwise the email gets Review.
                </FieldDescription>
              </Field>
              <div className="grid grid-cols-2 gap-4">
                <Field>
                  <FieldLabel htmlFor="body-chars">Body characters sent</FieldLabel>
                  <Input
                    id="body-chars"
                    type="number"
                    min={100}
                    max={20000}
                    value={draft.body_chars}
                    onChange={(e) => set("body_chars", Number(e.target.value))}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="max-per-poll">Max messages per check</FieldLabel>
                  <Input
                    id="max-per-poll"
                    type="number"
                    min={1}
                    max={500}
                    value={draft.max_per_poll}
                    onChange={(e) => set("max_per_poll", Number(e.target.value))}
                  />
                </Field>
              </div>
            </FieldGroup>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Jev</CardTitle>
            <CardDescription>TypeSafe System One, asked one Choice question per email.</CardDescription>
          </CardHeader>
          <CardContent>
            <FieldGroup>
              <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                <span>
                  API key <span className="text-muted-foreground">(JEV_API_KEY)</span>
                </span>
                <Configured ok={settings.jev_configured} />
              </div>
              <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                <span>Model</span>
                <code className="rounded bg-muted px-1.5 py-0.5 text-xs">{settings.jev_model}</code>
              </div>
              <Field>
                <FieldLabel htmlFor="instructions">Question instructions</FieldLabel>
                <Textarea
                  id="instructions"
                  rows={3}
                  value={draft.instructions}
                  onChange={(e) => set("instructions", e.target.value)}
                />
                <FieldDescription>
                  Sent as the Choice question's instructions. The labels and their descriptions are the options.
                </FieldDescription>
              </Field>
            </FieldGroup>
          </CardContent>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Integrations</CardTitle>
            <CardDescription>
              OAuth apps are configured with environment variables. Register these redirect URIs with each provider.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="grid gap-6 md:grid-cols-2">
              <FieldGroup>
                <div className="flex items-center justify-between text-sm">
                  <span className="font-medium">Google (Gmail)</span>
                  <Configured ok={settings.google_configured} />
                </div>
                <CopyField value={`${settings.public_url}/api/oauth/google/callback`} />
              </FieldGroup>
              <FieldGroup>
                <div className="flex items-center justify-between text-sm">
                  <span className="font-medium">Microsoft (Outlook)</span>
                  <Configured ok={settings.microsoft_configured} />
                </div>
                <CopyField value={`${settings.public_url}/api/oauth/microsoft/callback`} />
              </FieldGroup>
            </div>
          </CardContent>
          <CardFooter className="text-xs text-muted-foreground">
            IMAP accounts need no app registration. Add them on the Accounts page.
          </CardFooter>
        </Card>
      </div>

      <ConfirmDialog
        open={confirmLive}
        onOpenChange={setConfirmLive}
        title="Turn off global dry-run?"
        description="Once saved, accounts without their own dry-run switch will start applying labels and running rules in their mailboxes. Nothing is ever deleted."
        confirmLabel="Turn off"
        destructive={false}
        onConfirm={() => set("dry_run", false)}
      />
    </div>
  )
}
