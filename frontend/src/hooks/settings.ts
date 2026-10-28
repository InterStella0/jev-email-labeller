import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { api } from "@/lib/api"
import type { PlaygroundResult, Settings, Stats } from "@/lib/types"

export type SettingsInput = Partial<
  Pick<Settings, "threshold" | "body_chars" | "dry_run" | "max_per_poll" | "instructions">
>

export function useSettings() {
  return useQuery({ queryKey: ["settings"], queryFn: () => api.get<Settings>("/settings") })
}

export function useUpdateSettings() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: SettingsInput) => api.put<Settings>("/settings", input),
    onSuccess: (data) => qc.setQueryData(["settings"], data),
  })
}

export function useStats() {
  return useQuery({ queryKey: ["stats"], queryFn: () => api.get<Stats>("/stats"), refetchInterval: 30_000 })
}

export function usePlayground() {
  return useMutation({
    mutationFn: (input: { sender: string; subject: string; body: string }) =>
      api.post<PlaygroundResult>("/playground/classify", input),
  })
}
