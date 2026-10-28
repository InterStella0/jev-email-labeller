import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { api } from "@/lib/api"

export function useSession() {
  return useQuery({ queryKey: ["me"], queryFn: () => api.get<{ ok: boolean }>("/auth/me"), retry: false })
}

export function useLogin() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (password: string) => api.post("/auth/login", { password }),
    onSuccess: () => qc.invalidateQueries(),
  })
}

export function useLogout() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api.post("/auth/logout"),
    onSuccess: () => {
      qc.clear()
      window.location.assign("/login")
    },
  })
}
