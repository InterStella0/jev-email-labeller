import { useMemo, useState, type FormEvent } from "react"
import { FlaskConicalIcon, Loader2Icon, SparklesIcon } from "lucide-react"

import { EmptyState, LabelBadge, PageHeader, ProbabilityList } from "@/components/common"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { useLabels } from "@/hooks/labels"
import { usePlayground } from "@/hooks/settings"
import { actionLabels, percent } from "@/lib/format"

const samples = [
  {
    name: "Receipt",
    sender: "Acme Store <orders@acme.example>",
    subject: "Your order #48213 has been confirmed",
    body: "Thanks for your purchase!\n\nOrder #48213\n1 × Mechanical keyboard  $129.00\nTax  $10.32\nTotal  $139.32\n\nPaid with Visa ending 4242.",
  },
  {
    name: "Colleague",
    sender: "Dana Lee <dana@company.example>",
    subject: "Can you review the Q3 deck before Friday?",
    body: "Hi,\n\nI've put the Q3 deck in the shared drive. Could you take a look at slides 4–9 and tell me if the numbers match yours? I need to send it to finance on Friday morning.\n\nThanks!\nDana",
  },
  {
    name: "Promo",
    sender: "SuperShoes <news@supershoes.example>",
    subject: "48 hours only: 40% off everything 👟",
    body: "Our biggest sale of the season is here. Use code FALL40 at checkout. Free shipping on orders over $50. Unsubscribe here.",
  },
]

export default function PlaygroundPage() {
  const [sender, setSender] = useState("")
  const [subject, setSubject] = useState("")
  const [body, setBody] = useState("")
  const playground = usePlayground()
  const { data: labels = [] } = useLabels()
  const colors = useMemo(() => new Map(labels.map((l) => [l.name.toLowerCase(), l.color])), [labels])
  const result = playground.data

  function submit(e: FormEvent) {
    e.preventDefault()
    playground.mutate({ sender, subject, body })
  }

  function load(sample: (typeof samples)[number]) {
    setSender(sample.sender)
    setSubject(sample.subject)
    setBody(sample.body)
    playground.reset()
  }

  const chosen = result && labels.find((l) => l.id === result.outcome.label_id)

  return (
    <div className="space-y-6">
      <PageHeader
        title="Playground"
        description="Try an email against your current labels and thresholds. No mailbox is touched and nothing is saved."
      />

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Email</CardTitle>
            <CardDescription className="flex flex-wrap items-center gap-1.5">
              Try a sample:
              {samples.map((s) => (
                <Button key={s.name} variant="outline" size="xs" type="button" onClick={() => load(s)}>
                  {s.name}
                </Button>
              ))}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={submit}>
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="pg-from">From</FieldLabel>
                  <Input id="pg-from" value={sender} onChange={(e) => setSender(e.target.value)} />
                </Field>
                <Field>
                  <FieldLabel htmlFor="pg-subject">Subject</FieldLabel>
                  <Input id="pg-subject" value={subject} onChange={(e) => setSubject(e.target.value)} />
                </Field>
                <Field>
                  <FieldLabel htmlFor="pg-body">Body</FieldLabel>
                  <Textarea id="pg-body" rows={10} value={body} onChange={(e) => setBody(e.target.value)} />
                </Field>
                <Button type="submit" disabled={playground.isPending || (!subject.trim() && !body.trim())}>
                  {playground.isPending ? <Loader2Icon className="animate-spin" /> : <SparklesIcon />}
                  Classify
                </Button>
              </FieldGroup>
            </form>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Result</CardTitle>
            <CardDescription>What the worker would do with this email.</CardDescription>
          </CardHeader>
          <CardContent>
            {!result ? (
              <EmptyState icon={<FlaskConicalIcon />} title="No result yet">
                {playground.error ? (
                  <span className="text-destructive">{playground.error.message}</span>
                ) : (
                  "Fill in an email and press Classify."
                )}
              </EmptyState>
            ) : (
              <div className="space-y-6 text-sm">
                <div className="space-y-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-muted-foreground">Would apply</span>
                    <LabelBadge name={result.outcome.label_name} color={chosen?.color} />
                    {result.outcome.is_review && (
                      <Badge variant="outline" className="border-amber-200 bg-amber-50 text-amber-700">
                        below threshold
                      </Badge>
                    )}
                  </div>
                  <p className="text-muted-foreground">
                    Jev's top pick was <b className="text-foreground">{result.outcome.top_label ?? "—"}</b> at{" "}
                    <b className="text-foreground">{percent(result.outcome.top_probability, 1)}</b>. The threshold is{" "}
                    {percent(result.outcome.threshold)}. Confidence: {percent(result.decision.confidence)}.
                  </p>
                </div>
                <ProbabilityList
                  probabilities={result.decision.probabilities}
                  colors={colors}
                  highlight={result.outcome.top_label}
                />
                <div className="space-y-1.5">
                  <h4 className="font-medium">Rule actions</h4>
                  {result.outcome.actions.length === 0 ? (
                    <p className="text-muted-foreground">None.</p>
                  ) : (
                    <ul className="list-inside list-disc">
                      {result.outcome.actions.map((a) => (
                        <li key={a.rule_id}>
                          {actionLabels[a.action]}
                          {a.folder && ` → ${a.folder}`}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
                <details className="rounded-lg border p-3">
                  <summary className="cursor-pointer font-medium">Text sent to Jev</summary>
                  <pre className="mt-2 max-h-64 overflow-auto text-xs whitespace-pre-wrap text-muted-foreground">
                    {result.state}
                  </pre>
                </details>
                <p className="text-xs text-muted-foreground">
                  {result.decision.model ?? "Jev"} · {result.decision.input_tokens ?? "?"} in /{" "}
                  {result.decision.output_tokens ?? "?"} out tokens
                </p>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
