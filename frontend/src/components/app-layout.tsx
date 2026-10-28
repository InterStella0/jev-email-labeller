import {
  FlaskConicalIcon,
  InboxIcon,
  LayoutDashboardIcon,
  LogOutIcon,
  MailIcon,
  SettingsIcon,
  TagIcon,
  WorkflowIcon,
} from "lucide-react"
import { Navigate, NavLink, Outlet, useLocation } from "react-router"

import { Badge } from "@/components/ui/badge"
import { Separator } from "@/components/ui/separator"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import { Skeleton } from "@/components/ui/skeleton"
import { useLogout, useSession } from "@/hooks/auth"
import { useSettings } from "@/hooks/settings"

const nav = [
  { to: "/", label: "Dashboard", icon: LayoutDashboardIcon },
  { to: "/classifications", label: "Classifications", icon: InboxIcon },
  { to: "/labels", label: "Labels", icon: TagIcon },
  { to: "/rules", label: "Rules", icon: WorkflowIcon },
  { to: "/accounts", label: "Accounts", icon: MailIcon },
  { to: "/playground", label: "Playground", icon: FlaskConicalIcon },
  { to: "/settings", label: "Settings", icon: SettingsIcon },
]

function AppSidebar() {
  const { pathname } = useLocation()
  const logout = useLogout()
  const { data: settings } = useSettings()

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <div className="flex items-center gap-2.5 px-1 py-1.5">
          <img src="/favicon.svg" alt="" className="size-8 shrink-0" />
          <div className="grid leading-tight group-data-[collapsible=icon]:hidden">
            <span className="font-semibold">Jev Labeller</span>
            <span className="text-xs text-muted-foreground">Email triage</span>
          </div>
        </div>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              {nav.map((item) => {
                const active = item.to === "/" ? pathname === "/" : pathname.startsWith(item.to)
                return (
                  <SidebarMenuItem key={item.to}>
                    <SidebarMenuButton asChild isActive={active} tooltip={item.label}>
                      <NavLink to={item.to}>
                        <item.icon />
                        <span>{item.label}</span>
                      </NavLink>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                )
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        {settings?.dry_run && (
          <div className="px-2 group-data-[collapsible=icon]:hidden">
            <Badge variant="outline" className="border-sky-200 bg-sky-50 text-sky-700">
              Global dry-run on
            </Badge>
          </div>
        )}
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton onClick={() => logout.mutate()} tooltip="Sign out">
              <LogOutIcon />
              <span>Sign out</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  )
}

export function AppLayout() {
  const session = useSession()
  const location = useLocation()

  if (session.isError) {
    const next = encodeURIComponent(location.pathname + location.search)
    return <Navigate to={`/login?next=${next}`} replace />
  }

  const current = nav.find((n) => (n.to === "/" ? location.pathname === "/" : location.pathname.startsWith(n.to)))

  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset>
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 h-4 data-[orientation=vertical]:h-4" />
          <span className="text-sm font-medium">{current?.label}</span>
        </header>
        <main className="mx-auto w-full max-w-7xl flex-1 p-4 md:p-8">
          {session.isPending ? (
            <div className="space-y-4">
              <Skeleton className="h-8 w-48" />
              <Skeleton className="h-40 w-full" />
            </div>
          ) : (
            <Outlet />
          )}
        </main>
      </SidebarInset>
    </SidebarProvider>
  )
}
