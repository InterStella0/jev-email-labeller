import { useEffect, useMemo, useState } from "react"
import { zodResolver } from "@hookform/resolvers/zod"
import {
  AlertTriangleIcon,
  CheckCircle2Icon,
  LinkIcon,
  MailIcon,
  PencilIcon,
  PlugZapIcon,
  PlusIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react"
import { Controller, useForm, useWatch } from "react-hook-form"
import { useSearchParams } from "react-router"
import { toast } from "sonner"
import { z } from "zod"

import { ConfirmDialog } from "@/components/confirm-dialog"
import { EmptyState, PageHeader } from "@/components/common"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
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
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  oauthStartUrl,
  useAccounts,
  useCreateAccount,
  useDeleteAccount,
  usePollAccount,
  useTestAccount,
  useUpdateAccount,
  type AccountInput,
} from "@/hooks/accounts"
import { useSettings } from "@/hooks/settings"
import { providerLabels, relativeTime } from "@/lib/format"
import type { Account, Provider } from "@/lib/types"

function makeSchema(isNew: boolean) {
  return z
    .object({
      provider: z.enum(["gmail", "outlook", "imap"]),
      name: z.string().trim().min(1, "Give the account a name"),
      poll_interval_secs: z.number({ error: "Enter a number" }).int().min(15, "At least 15 seconds"),
      dry_run: z.boolean(),
      host: z.string(),
      port: z.number({ error: "Enter a port" }).int().min(1).max(65535),
      username: z.string(),
      password: z.string(),
      security: z.enum(["tls", "starttls", "none"]),
      accept_invalid_certs: z.boolean(),
      mailbox: z.string(),
      archive_folder: z.string(),
    })
    .superRefine((v, ctx) => {
      if (v.provider !== "imap") return
      if (!v.host.trim()) ctx.addIssue({ code: "custom", path: ["host"], message: "Host is required" })
      if (!v.username.trim()) ctx.addIssue({ code: "custom", path: ["username"], message: "Username is required" })
      if (isNew && !v.password) ctx.addIssue({ code: "custom", path: ["password"], message: "Password is required" })
    })
}
type FormValues = z.infer<ReturnType<typeof makeSchema>>

const defaultPorts: Record<string, number> = { tls: 993, starttls: 143, none: 143 }

function AccountDialog({
  open,
  account,
  onOpenChange,
}: {
  open: boolean
  account: Account | null
  onOpenChange: (o: boolean) => void
}) {
  const isNew = !account
  const schema = useMemo(() => makeSchema(isNew), [isNew])
  const form = useForm<FormValues>({ resolver: zodResolver(schema) })
  const create = useCreateAccount()
  const update = useUpdateAccount()
  const { data: settings } = useSettings()

  useEffect(() => {
    if (!open) return
    const s = account?.settings ?? {}
    form.reset({
      provider: account?.provider ?? "gmail",
      name: account?.name ?? "",
      poll_interval_secs: account?.poll_interval_secs ?? 60,
      dry_run: account?.dry_run ?? true,
      host: s.host ?? "",
      port: s.port ?? 993,
      username: s.username ?? "",
      password: "",
      security: s.security ?? "tls",
      accept_invalid_certs: s.accept_invalid_certs ?? false,
      mailbox: s.mailbox ?? "INBOX",
      archive_folder: s.archive_folder ?? "Archive",
    })
  }, [open, account, form])

  const provider = useWatch({ control: form.control, name: "provider" })
  const security = useWatch({ control: form.control, name: "security" })
  const errors = form.formState.errors
  const oauthConfigured =
    provider === "gmail" ? settings?.google_configured : provider === "outlook" ? settings?.microsoft_configured : true

  function submit(v: FormValues) {
    const input: AccountInput = {
      name: v.name,
      poll_interval_secs: v.poll_interval_secs,
      dry_run: v.dry_run,
    }
    if (v.provider === "imap") {
      input.settings = {
        host: v.host.trim(),
        port: v.port,
        username: v.username.trim(),
        security: v.security,
        accept_invalid_certs: v.accept_invalid_certs,
        mailbox: v.mailbox.trim() || "INBOX",
        archive_folder: v.archive_folder.trim() || "Archive",
      }
      if (v.password) input.password = v.password
    }

    if (account) {
      update.mutate(
        { id: account.id, ...input },
        {
          onSuccess: () => {
            toast.success("Account saved")
            onOpenChange(false)
          },
        },
      )
    } else {
      create.mutate(
        { ...input, provider: v.provider },
        {
          onSuccess: (created) => {
            if (created.provider === "imap") {
              toast.success("Account added. It will be checked shortly.")
              onOpenChange(false)
            } else {
              window.location.assign(oauthStartUrl(created))
            }
          },
        },
      )
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{account ? `Edit ${account.name}` : "Add account"}</DialogTitle>
          <DialogDescription>
            New accounts start in dry-run mode: Jev classifies mail but nothing in the mailbox changes.
          </DialogDescription>
        </DialogHeader>
        <form id="account-form" onSubmit={form.handleSubmit(submit)} className="max-h-[65vh] overflow-y-auto pr-1">
          <FieldGroup>
            {isNew && (
              <Controller
                control={form.control}
                name="provider"
                render={({ field }) => (
                  <Tabs value={field.value} onValueChange={(v) => field.onChange(v as Provider)}>
                    <TabsList className="w-full">
                      <TabsTrigger value="gmail">Gmail</TabsTrigger>
                      <TabsTrigger value="outlook">Outlook</TabsTrigger>
                      <TabsTrigger value="imap">IMAP</TabsTrigger>
                    </TabsList>
                  </Tabs>
                )}
              />
            )}
            {isNew && provider !== "imap" && !oauthConfigured && (
              <div className="flex gap-2 rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
                <AlertTriangleIcon className="mt-0.5 size-4 shrink-0" />
                <span>
                  {provider === "gmail" ? "GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET" : "MS_CLIENT_ID / MS_CLIENT_SECRET"}{" "}
                  aren't set on the server, so connecting will fail. See the README for setup.
                </span>
              </div>
            )}
            <Field data-invalid={!!errors.name}>
              <FieldLabel htmlFor="acc-name">Name</FieldLabel>
              <Input id="acc-name" placeholder="Personal inbox" {...form.register("name")} />
              <FieldError errors={[errors.name]} />
            </Field>

            {provider === "imap" && (
              <>
                <div className="grid grid-cols-[1fr_6rem] gap-3">
                  <Field data-invalid={!!errors.host}>
                    <FieldLabel htmlFor="acc-host">IMAP host</FieldLabel>
                    <Input id="acc-host" placeholder="imap.fastmail.com" {...form.register("host")} />
                    <FieldError errors={[errors.host]} />
                  </Field>
                  <Field data-invalid={!!errors.port}>
                    <FieldLabel htmlFor="acc-port">Port</FieldLabel>
                    <Input id="acc-port" type="number" {...form.register("port", { valueAsNumber: true })} />
                    <FieldError errors={[errors.port]} />
                  </Field>
                </div>
                <Field>
                  <FieldLabel>Security</FieldLabel>
                  <Controller
                    control={form.control}
                    name="security"
                    render={({ field }) => (
                      <Select
                        value={field.value}
                        onValueChange={(v) => {
                          field.onChange(v)
                          form.setValue("port", defaultPorts[v])
                        }}
                      >
                        <SelectTrigger className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="tls">TLS (port 993)</SelectItem>
                          <SelectItem value="starttls">STARTTLS (port 143)</SelectItem>
                          <SelectItem value="none">None: local test servers only</SelectItem>
                        </SelectContent>
                      </Select>
                    )}
                  />
                </Field>
                {security !== "none" && (
                  <Field orientation="horizontal">
                    <Controller
                      control={form.control}
                      name="accept_invalid_certs"
                      render={({ field }) => (
                        <Switch id="acc-invalid-certs" checked={field.value} onCheckedChange={field.onChange} />
                      )}
                    />
                    <div className="space-y-0.5">
                      <FieldLabel htmlFor="acc-invalid-certs">Accept self-signed certificate</FieldLabel>
                      <FieldDescription>Needed for Proton Mail Bridge and similar local bridges.</FieldDescription>
                    </div>
                  </Field>
                )}
                <Field data-invalid={!!errors.username}>
                  <FieldLabel htmlFor="acc-user">Username</FieldLabel>
                  <Input id="acc-user" autoComplete="off" {...form.register("username")} />
                  <FieldError errors={[errors.username]} />
                </Field>
                <Field data-invalid={!!errors.password}>
                  <FieldLabel htmlFor="acc-pass">Password</FieldLabel>
                  <Input
                    id="acc-pass"
                    type="password"
                    autoComplete="new-password"
                    placeholder={account ? "Leave blank to keep the current one" : "App password recommended"}
                    {...form.register("password")}
                  />
                  <FieldDescription>Encrypted at rest. It's never sent back to the browser.</FieldDescription>
                  <FieldError errors={[errors.password]} />
                </Field>
                <div className="grid grid-cols-2 gap-3">
                  <Field>
                    <FieldLabel htmlFor="acc-mailbox">Mailbox to watch</FieldLabel>
                    <Input id="acc-mailbox" {...form.register("mailbox")} />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor="acc-archive">Archive folder</FieldLabel>
                    <Input id="acc-archive" {...form.register("archive_folder")} />
                  </Field>
                </div>
              </>
            )}

            <Separator />
            <Field data-invalid={!!errors.poll_interval_secs}>
              <FieldLabel htmlFor="acc-interval">Check every (seconds)</FieldLabel>
              <Input
                id="acc-interval"
                type="number"
                min={15}
                {...form.register("poll_interval_secs", { valueAsNumber: true })}
              />
              <FieldError errors={[errors.poll_interval_secs]} />
            </Field>
            <Field orientation="horizontal">
              <Controller
                control={form.control}
                name="dry_run"
                render={({ field }) => <Switch id="acc-dry" checked={field.value} onCheckedChange={field.onChange} />}
              />
              <div className="space-y-0.5">
                <FieldLabel htmlFor="acc-dry">Dry run</FieldLabel>
                <FieldDescription>Log decisions only. Don't touch this mailbox.</FieldDescription>
              </div>
            </Field>
          </FieldGroup>
        </form>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button type="submit" form="account-form" disabled={create.isPending || update.isPending}>
            {account ? "Save" : provider === "imap" ? "Add account" : `Continue to ${providerLabels[provider]}`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function AccountCard({
  account,
  onEdit,
  onDelete,
}: {
  account: Account
  onEdit: () => void
  onDelete: () => void
}) {
  const update = useUpdateAccount()
  const test = useTestAccount()
  const poll = usePollAccount()
  const isOAuth = account.provider !== "imap"
  const address = isOAuth ? account.settings.email : `${account.settings.username} @ ${account.settings.host}`

  return (
    <Card className={!account.enabled ? "opacity-75" : undefined}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <MailIcon className="size-4 text-muted-foreground" />
          {account.name}
        </CardTitle>
        <CardDescription className="truncate">{address || "Not connected yet"}</CardDescription>
        <CardAction className="flex gap-1">
          <Button variant="ghost" size="icon-sm" onClick={onEdit} aria-label="Edit">
            <PencilIcon />
          </Button>
          <Button variant="ghost" size="icon-sm" onClick={onDelete} aria-label="Delete">
            <Trash2Icon />
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-1.5">
          <Badge variant="secondary">{providerLabels[account.provider]}</Badge>
          {account.connected ? (
            <Badge variant="outline" className="gap-1 border-emerald-200 bg-emerald-50 text-emerald-700">
              <CheckCircle2Icon className="size-3" /> Connected
            </Badge>
          ) : (
            <Badge variant="outline" className="border-amber-200 bg-amber-50 text-amber-700">
              Not connected
            </Badge>
          )}
          <Badge variant="outline">every {account.poll_interval_secs}s</Badge>
        </div>
        <div className="grid grid-cols-2 gap-3 text-sm">
          <label className="flex items-center gap-2">
            <Switch
              checked={account.enabled}
              onCheckedChange={(enabled) => update.mutate({ id: account.id, enabled })}
            />
            Active
          </label>
          <label className="flex items-center gap-2">
            <Switch
              checked={account.dry_run}
              onCheckedChange={(dry_run) => update.mutate({ id: account.id, dry_run })}
            />
            Dry run
          </label>
        </div>
        <div className="text-xs text-muted-foreground">Last checked {relativeTime(account.last_polled_at)}</div>
        {account.last_error && (
          <p className="line-clamp-3 rounded-lg border border-red-200 bg-red-50 p-2 text-xs text-red-800">
            {account.last_error}
          </p>
        )}
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2 border-t pt-4">
        {isOAuth && (
          <Button size="sm" variant={account.connected ? "outline" : "default"} asChild>
            <a href={oauthStartUrl(account)}>
              <LinkIcon /> {account.connected ? "Reconnect" : "Connect"}
            </a>
          </Button>
        )}
        <Button
          size="sm"
          variant="outline"
          disabled={!account.connected || test.isPending}
          onClick={() =>
            test.mutate(account.id, { onSuccess: (r) => toast.success(r.message, { duration: 6000 }) })
          }
        >
          <PlugZapIcon /> Test
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!account.enabled || !account.connected || poll.isPending}
          onClick={() => poll.mutate(account.id, { onSuccess: () => toast.success("Checking now…") })}
        >
          <RefreshCwIcon /> Check now
        </Button>
      </CardFooter>
    </Card>
  )
}

export default function AccountsPage() {
  const { data: accounts, isPending } = useAccounts()
  const remove = useDeleteAccount()
  const [params, setParams] = useSearchParams()
  const [editing, setEditing] = useState<Account | null>(null)
  const [creating, setCreating] = useState(false)
  const [deleting, setDeleting] = useState<Account | null>(null)

  // Result of the OAuth redirect from the backend.
  useEffect(() => {
    const status = params.get("oauth")
    if (!status) return
    const message = params.get("message") ?? ""
    if (status === "connected") toast.success(message || "Account connected")
    else toast.error(message || "Connecting failed")
    setParams({}, { replace: true })
  }, [params, setParams])

  return (
    <div className="space-y-6">
      <PageHeader
        title="Accounts"
        description="Mailboxes Jev watches. Gmail uses native labels, Outlook uses categories, and IMAP uses keywords or folders."
        actions={
          <Button onClick={() => setCreating(true)}>
            <PlusIcon /> Add account
          </Button>
        }
      />

      {isPending ? (
        <div className="grid gap-4 md:grid-cols-2">
          <Skeleton className="h-56" />
          <Skeleton className="h-56" />
        </div>
      ) : accounts?.length === 0 ? (
        <EmptyState icon={<MailIcon />} title="No accounts yet">
          Connect Gmail or Outlook, or add any IMAP mailbox. New accounts start in dry-run mode.
        </EmptyState>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {accounts?.map((a) => (
            <AccountCard key={a.id} account={a} onEdit={() => setEditing(a)} onDelete={() => setDeleting(a)} />
          ))}
        </div>
      )}

      <AccountDialog
        open={creating || !!editing}
        account={editing}
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
        title={`Remove “${deleting?.name}”?`}
        description="Stops watching this mailbox and forgets its classification history. Nothing in the mailbox itself is changed."
        confirmLabel="Remove"
        onConfirm={() => deleting && remove.mutate(deleting.id, { onSuccess: () => toast.success("Account removed") })}
      />
    </div>
  )
}
