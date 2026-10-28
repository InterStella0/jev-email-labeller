import {
  AlertTriangleIcon,
  CheckCircle2Icon,
  EyeIcon,
  FlaskConicalIcon,
  InboxIcon,
  RefreshCwIcon,
  XCircleIcon,
} from "lucide-react"
import { Link } from "react-router"
import type { ReactNode } from "react"

import { ColorDot, LabelBadge, PageHeader, StatusBadge } from "@/components/common"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { useAccounts, usePollAccount } from "@/hooks/accounts"
import { useSettings, useStats } from "@/hooks/settings"
import { percent, providerLabels, relativeTime } from "@/lib/format"

function StatCard({ title, value, icon, hint }: { title: string; value: ReactNode; icon: ReactNode; hint?: string }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardDescription className="flex items-center justify-between">
          {title}
          <span className="text-muted-foreground [&_svg]:size-4">{icon}</span>
        </CardDescription>
        <CardTitle className="text-2xl font-semibold tabular-nums">{value}</CardTitle>
        {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      </CardHeader>
    </Card>
  )
}

function Banner({ tone, children }: { tone: "info" | "warn"; children: ReactNode }) {
  const styles = tone === "info" ? "border-sky-200 bg-sky-50 text-sky-900" : "border-amber-200 bg-amber-50 text-amber-900"
  return <div className={`flex items-start gap-3 rounded-xl border px-4 py-3 text-sm ${styles}`}>{children}</div>
}

export default function DashboardPage() {
  const { data: stats, isPending } = useStats()
  const { data: settings } = useSettings()
  const { data: accounts } = useAccounts()
  const poll = usePollAccount()

  const maxCount = Math.max(1, ...(stats?.by_label.map((l) => l.count) ?? [1]))

  return (
    <div className="space-y-6">
      <PageHeader title="Dashboard" description="What Jev has been doing with your mail." />

      {settings?.dry_run && (
        <Banner tone="info">
          <FlaskConicalIcon className="mt-0.5 size-4 shrink-0" />
          <div>
            <b>Dry-run is on.</b> Decisions are recorded but no mailbox is changed. Review them under{" "}
            <Link className="underline" to="/classifications?status=dry_run">
              Classifications
            </Link>{" "}
            and turn dry-run off in{" "}
            <Link className="underline" to="/settings">
              Settings
            </Link>{" "}
            when you trust the results.
          </div>
        </Banner>
      )}
      {settings && !settings.jev_configured && (
        <Banner tone="warn">
          <AlertTriangleIcon className="mt-0.5 size-4 shrink-0" />
          <div>
            <b>JEV_API_KEY is not set.</b> Add it to <code>.env</code> and restart the backend.
          </div>
        </Banner>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          title="Classified"
          value={isPending ? <Skeleton className="h-8 w-16" /> : stats?.total}
          icon={<InboxIcon />}
          hint={stats ? `${stats.last_24h} in the last 24h` : undefined}
        />
        <StatCard title="Applied" value={stats?.by_status.applied ?? 0} icon={<CheckCircle2Icon />} />
        <StatCard
          title="Needs review"
          value={(stats?.by_status.review ?? 0) + (stats?.by_status.dry_run ?? 0)}
          icon={<EyeIcon />}
          hint={stats?.by_status.dry_run ? `${stats.by_status.dry_run} are dry-run decisions` : undefined}
        />
        <StatCard title="Errors" value={stats?.by_status.error ?? 0} icon={<XCircleIcon />} />
      </div>

      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-1">
          <CardHeader>
            <CardTitle>By label</CardTitle>
            <CardDescription>All-time decisions per label.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {stats?.by_label.map((l) => (
              <Link
                key={l.id}
                to={`/classifications?label_id=${l.id}`}
                className="grid grid-cols-[minmax(0,8rem)_1fr_2.5rem] items-center gap-3 text-sm hover:opacity-80"
              >
                <span className="flex items-center gap-2 truncate">
                  <ColorDot color={l.color} />
                  <span className="truncate">{l.name}</span>
                </span>
                <div className="h-2 overflow-hidden rounded-full bg-muted">
                  <div className="h-full rounded-full" style={{ width: `${(l.count / maxCount) * 100}%`, background: l.color }} />
                </div>
                <span className="text-right tabular-nums text-muted-foreground">{l.count}</span>
              </Link>
            ))}
          </CardContent>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Accounts</CardTitle>
            <CardDescription>Mailboxes being watched.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {accounts?.length === 0 && (
              <p className="text-sm text-muted-foreground">
                No accounts yet.{" "}
                <Link to="/accounts" className="underline">
                  Add one
                </Link>
                .
              </p>
            )}
            {accounts?.map((a) => (
              <div key={a.id} className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3">
                <div className="min-w-0 space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{a.name}</span>
                    <Badge variant="secondary">{providerLabels[a.provider]}</Badge>
                    {!a.enabled && <Badge variant="outline">Paused</Badge>}
                    {a.dry_run && (
                      <Badge variant="outline" className="border-sky-200 bg-sky-50 text-sky-700">
                        Dry run
                      </Badge>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground">Last checked {relativeTime(a.last_polled_at)}</p>
                  {a.last_error && <p className="line-clamp-2 text-xs text-destructive">{a.last_error}</p>}
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={!a.enabled || poll.isPending}
                  onClick={() => poll.mutate(a.id)}
                >
                  <RefreshCwIcon /> Check now
                </Button>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Recent decisions</CardTitle>
          <CardDescription>The latest messages Jev has looked at.</CardDescription>
          <CardAction>
            <Button variant="outline" size="sm" asChild>
              <Link to="/classifications">View all</Link>
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          {stats?.recent.length === 0 ? (
            <p className="text-sm text-muted-foreground">Nothing classified yet.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Message</TableHead>
                  <TableHead>Label</TableHead>
                  <TableHead className="text-right">Top probability</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">When</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {stats?.recent.map((c) => (
                  <TableRow key={c.id}>
                    <TableCell className="max-w-sm">
                      <div className="truncate font-medium">{c.subject || "(no subject)"}</div>
                      <div className="truncate text-xs text-muted-foreground">{c.sender}</div>
                    </TableCell>
                    <TableCell>
                      <LabelBadge name={c.label_name} color={c.label_color} />
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{percent(c.top_probability)}</TableCell>
                    <TableCell>
                      <StatusBadge status={c.status} />
                    </TableCell>
                    <TableCell className="text-right text-muted-foreground">{relativeTime(c.created_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
