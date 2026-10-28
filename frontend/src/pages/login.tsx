import { useState, type FormEvent } from "react"
import { Loader2Icon } from "lucide-react"
import { useNavigate, useSearchParams } from "react-router"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { useLogin } from "@/hooks/auth"
import { ApiError } from "@/lib/api"

export default function LoginPage() {
  const [password, setPassword] = useState("")
  const login = useLogin()
  const navigate = useNavigate()
  const [params] = useSearchParams()

  function submit(e: FormEvent) {
    e.preventDefault()
    login.mutate(password, {
      onSuccess: () => {
        const next = params.get("next")
        navigate(next?.startsWith("/") && !next.startsWith("//") ? next : "/", { replace: true })
      },
    })
  }

  const error =
    login.error instanceof ApiError && login.error.status === 401 ? "Wrong password." : login.error?.message

  return (
    <div className="flex min-h-svh items-center justify-center bg-muted/40 p-6">
      <Card className="w-full max-w-sm">
        <CardHeader className="items-center text-center">
          <img src="/favicon.svg" alt="" className="mx-auto mb-2 size-10" />
          <CardTitle className="text-xl">Jev Labeller</CardTitle>
          <CardDescription>Sign in with the admin password.</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={submit}>
            <FieldGroup>
              <Field data-invalid={!!error}>
                <FieldLabel htmlFor="password">Password</FieldLabel>
                <Input
                  id="password"
                  type="password"
                  autoComplete="current-password"
                  autoFocus
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  aria-invalid={!!error}
                />
                {error && <FieldError>{error}</FieldError>}
              </Field>
              <Button type="submit" disabled={!password || login.isPending}>
                {login.isPending && <Loader2Icon className="animate-spin" />}
                Sign in
              </Button>
            </FieldGroup>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
