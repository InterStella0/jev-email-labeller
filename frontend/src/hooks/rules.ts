import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { api } from "@/lib/api"
import type { Rule, RuleAction } from "@/lib/types"

export type RuleInput = {
  label_id?: number
  action?: RuleAction
  folder?: string | null
  min_probability?: number
  enabled?: boolean
}

export function useRules() {
  return useQuery({ queryKey: ["rules"], queryFn: () => api.get<Rule[]>("/rules") })
}

export function useCreateRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: RuleInput) => api.post<Rule>("/rules", input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rules"] }),
  })
}

export function useUpdateRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...input }: RuleInput & { id: number }) => api.patch<Rule>(`/rules/${id}`, input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rules"] }),
  })
}

export function useDeleteRule() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/rules/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rules"] }),
  })
}
