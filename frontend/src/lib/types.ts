export type Label = {
  id: number
  name: string
  description: string
  color: string
  threshold: number | null
  enabled: boolean
  is_system: boolean
  sort: number
  created_at: string
  updated_at: string
}

export type RuleAction = "archive" | "mark_read" | "move_to_folder" | "star"

export type Rule = {
  id: number
  label_id: number
  action: RuleAction
  params: { folder?: string }
  min_probability: number
  enabled: boolean
  created_at: string
  updated_at: string
}

export type Provider = "gmail" | "outlook" | "imap"
export type ImapSecurity = "tls" | "starttls" | "none"

export type ImapSettings = {
  host: string
  port: number
  username: string
  security: ImapSecurity
  accept_invalid_certs: boolean
  mailbox: string
  archive_folder: string
}

export type Account = {
  id: number
  name: string
  provider: Provider
  settings: Partial<ImapSettings> & { email?: string | null }
  connected: boolean
  enabled: boolean
  dry_run: boolean
  poll_interval_secs: number
  last_polled_at: string | null
  last_error: string | null
  created_at: string
  updated_at: string
}

export type Status = "applied" | "review" | "dry_run" | "overridden" | "error"

export type ActionRecord = {
  action: RuleAction
  folder?: string
  done: boolean
  error?: string
}

export type Classification = {
  id: number
  account_id: number
  provider_msg_id: string
  thread_id: string | null
  sender: string
  subject: string
  snippet: string
  received_at: string | null
  label_id: number | null
  applied_label: string | null
  top_label: string | null
  top_probability: number | null
  confidence: number | null
  probabilities: Record<string, number>
  status: Status
  actions_taken: ActionRecord[]
  error: string | null
  model: string | null
  created_at: string
  updated_at: string
  label_name: string | null
  label_color: string | null
  account_name: string | null
}

export type Page<T> = { items: T[]; total: number; page: number; per_page: number }

export type Settings = {
  threshold: number
  body_chars: number
  dry_run: boolean
  max_per_poll: number
  instructions: string
  jev_configured: boolean
  jev_model: string
  google_configured: boolean
  microsoft_configured: boolean
  public_url: string
}

export type Stats = {
  total: number
  last_24h: number
  by_status: Partial<Record<Status, number>>
  by_label: { id: number; name: string; color: string; is_system: boolean; count: number }[]
  recent: Classification[]
}

export type PlannedAction = { rule_id: number; action: RuleAction; folder?: string }

export type PlaygroundResult = {
  state: string
  decision: {
    choice: string
    probabilities: Record<string, number>
    confidence: number | null
    model: string | null
    input_tokens: number | null
    output_tokens: number | null
  }
  outcome: {
    label_id: number
    label_name: string
    is_review: boolean
    top_label: string | null
    top_probability: number
    threshold: number
    actions: PlannedAction[]
  }
}
