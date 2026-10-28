import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { api } from "@/lib/api"
import type { Account, ImapSettings, Provider } from "@/lib/types"

export type AccountInput = {
  name?: string
  provider?: Provider
  settings?: Partial<ImapSettings>
  password?: string
  enabled?: boolean
  dry_run?: boolean
  poll_interval_secs?: number
}

export function useAccounts() {
  return useQuery({
    queryKey: ["accounts"],
    queryFn: () => api.get<Account[]>("/accounts"),
    refetchInterval: 15_000,
  })
}

function useInvalidate() {
  const qc = useQueryClient()
  return () => {
    qc.invalidateQueries({ queryKey: ["accounts"] })
    qc.invalidateQueries({ queryKey: ["stats"] })
  }
}

export function useCreateAccount() {
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: (input: AccountInput) => api.post<Account>("/accounts", input),
    onSuccess: invalidate,
  })
}

export function useUpdateAccount() {
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: ({ id, ...input }: AccountInput & { id: number }) => api.patch<Account>(`/accounts/${id}`, input),
    onSuccess: invalidate,
  })
}

export function useDeleteAccount() {
  const qc = useQueryClient()
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/accounts/${id}`),
    onSuccess: () => {
      invalidate()
      qc.invalidateQueries({ queryKey: ["classifications"] })
    },
  })
}

export function useTestAccount() {
  return useMutation({
    mutationFn: (id: number) => api.post<{ ok: boolean; message: string }>(`/accounts/${id}/test`),
  })
}

export function usePollAccount() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.post<{ queued: boolean }>(`/accounts/${id}/poll`),
    // The poll runs in the background; refresh a little later to show its results.
    onSuccess: () =>
      setTimeout(() => {
        qc.invalidateQueries({ queryKey: ["accounts"] })
        qc.invalidateQueries({ queryKey: ["stats"] })
        qc.invalidateQueries({ queryKey: ["classifications"] })
      }, 4000),
  })
}

export function oauthStartUrl(account: Account): string {
  const provider = account.provider === "gmail" ? "google" : "microsoft"
  return `/api/oauth/${provider}/start?account_id=${account.id}`
}
