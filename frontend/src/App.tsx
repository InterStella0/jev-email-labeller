import { lazy, Suspense } from "react"
import { Navigate, Route, Routes } from "react-router"

import { AppLayout } from "@/components/app-layout"
import { Skeleton } from "@/components/ui/skeleton"

const AccountsPage = lazy(() => import("@/pages/accounts"))
const ClassificationsPage = lazy(() => import("@/pages/classifications"))
const DashboardPage = lazy(() => import("@/pages/dashboard"))
const LabelsPage = lazy(() => import("@/pages/labels"))
const LoginPage = lazy(() => import("@/pages/login"))
const PlaygroundPage = lazy(() => import("@/pages/playground"))
const RulesPage = lazy(() => import("@/pages/rules"))
const SettingsPage = lazy(() => import("@/pages/settings"))

export default function App() {
  return (
    <Suspense fallback={<Skeleton className="m-8 h-40" />}>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route element={<AppLayout />}>
          <Route index element={<DashboardPage />} />
          <Route path="classifications" element={<ClassificationsPage />} />
          <Route path="labels" element={<LabelsPage />} />
          <Route path="rules" element={<RulesPage />} />
          <Route path="accounts" element={<AccountsPage />} />
          <Route path="playground" element={<PlaygroundPage />} />
          <Route path="settings" element={<SettingsPage />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Suspense>
  )
}
