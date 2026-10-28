import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { api } from "@/lib/api"
import type { Label } from "@/lib/types"

export type LabelInput = Partial<Pick<Label, "name" | "description" | "color" | "threshold" | "enabled" | "sort">>

export function useLabels() {
  return useQuery({ queryKey: ["labels"], queryFn: () => api.get<Label[]>("/labels") })
}

function useInvalidate() {
  const qc = useQueryClient()
  return () => {
    qc.invalidateQueries({ queryKey: ["labels"] })
    qc.invalidateQueries({ queryKey: ["stats"] })
  }
}

export function useCreateLabel() {
  const invalidate = useInvalidate()
  return useMutation({ mutationFn: (input: LabelInput) => api.post<Label>("/labels", input), onSuccess: invalidate })
}

export function useUpdateLabel() {
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: ({ id, ...input }: LabelInput & { id: number }) => api.patch<Label>(`/labels/${id}`, input),
    onSuccess: invalidate,
  })
}

export function useDeleteLabel() {
  const qc = useQueryClient()
  const invalidate = useInvalidate()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/labels/${id}`),
    onSuccess: () => {
      invalidate()
      qc.invalidateQueries({ queryKey: ["rules"] })
      qc.invalidateQueries({ queryKey: ["classifications"] })
    },
  })
}
