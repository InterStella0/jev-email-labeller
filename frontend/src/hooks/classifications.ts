import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { api } from "@/lib/api"
import type { Classification, Page, Status } from "@/lib/types"

export type ClassificationFilters = {
  account_id?: number
  label_id?: number
  status?: Status
  q?: string
  page?: number
  per_page?: number
}

function toQuery(filters: ClassificationFilters): string {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(filters)) {
    if (value !== undefined && value !== "") params.set(key, String(value))
  }
  const s = params.toString()
  return s ? `?${s}` : ""
}

export function useClassifications(filters: ClassificationFilters) {
  return useQuery({
    queryKey: ["classifications", filters],
    queryFn: () => api.get<Page<Classification>>(`/classifications${toQuery(filters)}`),
    placeholderData: keepPreviousData,
    refetchInterval: 30_000,
  })
}

function useInvalidate() {
  const qc = useQueryClient()
  return () => {
    qc.invalidateQueries({ queryKey: ["classifications"] })
    qc.invalidateQueries({ queryKey: ["stats"] })
  }
}

export function useOverrideClassification() {
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: ({ id, label_id, apply_to_mailbox }: { id: number; label_id: number; apply_to_mailbox: boolean }) =>
      api.patch<Classification>(`/classifications/${id}`, { label_id, apply_to_mailbox }),
    onSuccess: invalidate,
  })
}

export function useApplyClassification() {
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: (id: number) => api.post<Classification>(`/classifications/${id}/apply`),
    onSuccess: invalidate,
  })
}

export function useReclassify() {
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: (id: number) => api.post<Classification>(`/classifications/${id}/reclassify`),
    onSuccess: invalidate,
  })
}

export function useDeleteClassification() {
  const invalidate = useInvalidate()
  return useMutation({ mutationFn: (id: number) => api.delete(`/classifications/${id}`), onSuccess: invalidate })
}
