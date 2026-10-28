-- Timestamps are RFC 3339 UTC strings written by the application.

CREATE TABLE labels (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT    NOT NULL DEFAULT '',
    color       TEXT    NOT NULL DEFAULT '#64748b',
    threshold   REAL    CHECK (threshold IS NULL OR (threshold >= 0 AND threshold <= 1)),
    enabled     INTEGER NOT NULL DEFAULT 1,
    is_system   INTEGER NOT NULL DEFAULT 0,
    sort        INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE TABLE rules (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    label_id        INTEGER NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    action          TEXT    NOT NULL CHECK (action IN ('archive', 'mark_read', 'move_to_folder', 'star')),
    params          TEXT    NOT NULL DEFAULT '{}',
    -- Mailbox-changing actions need a confident decision; never below 0.9.
    min_probability REAL    NOT NULL DEFAULT 0.9 CHECK (min_probability >= 0.9 AND min_probability <= 1),
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT    NOT NULL,
    updated_at      TEXT    NOT NULL
);
CREATE INDEX rules_label ON rules (label_id);

CREATE TABLE accounts (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    name               TEXT    NOT NULL,
    provider           TEXT    NOT NULL CHECK (provider IN ('gmail', 'outlook', 'imap')),
    settings           TEXT    NOT NULL DEFAULT '{}',
    -- IMAP password or OAuth token set, sealed with APP_SECRET.
    secret_enc         BLOB,
    enabled            INTEGER NOT NULL DEFAULT 1,
    dry_run            INTEGER NOT NULL DEFAULT 1,
    poll_interval_secs INTEGER NOT NULL DEFAULT 60 CHECK (poll_interval_secs >= 15),
    cursor             TEXT,
    last_polled_at     TEXT,
    last_error         TEXT,
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL
);

CREATE TABLE classifications (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id      INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    provider_msg_id TEXT    NOT NULL,
    thread_id       TEXT,
    sender          TEXT    NOT NULL DEFAULT '',
    subject         TEXT    NOT NULL DEFAULT '',
    snippet         TEXT    NOT NULL DEFAULT '',
    received_at     TEXT,
    -- Decided label (Review when below threshold). NULL if the label was deleted.
    label_id        INTEGER REFERENCES labels (id) ON DELETE SET NULL,
    -- Name of the label actually present in the mailbox, if any.
    applied_label   TEXT,
    top_label       TEXT,
    top_probability REAL,
    confidence      REAL,
    probabilities   TEXT    NOT NULL DEFAULT '{}',
    status          TEXT    NOT NULL CHECK (status IN ('applied', 'review', 'dry_run', 'overridden', 'error')),
    actions_taken   TEXT    NOT NULL DEFAULT '[]',
    error           TEXT,
    model           TEXT,
    created_at      TEXT    NOT NULL,
    updated_at      TEXT    NOT NULL,
    UNIQUE (account_id, provider_msg_id)
);
CREATE INDEX classifications_created ON classifications (created_at DESC);
CREATE INDEX classifications_label ON classifications (label_id);
CREATE INDEX classifications_status ON classifications (status);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    expires_at TEXT NOT NULL
);

INSERT INTO labels (name, description, color, is_system, sort, created_at, updated_at) VALUES
    ('Needs reply', 'A real person wrote to me personally and expects an answer or a decision from me.', '#ef4444', 0, 10, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('Updates', 'Automated notifications, account alerts, shipping updates, calendar notices, newsletters I subscribed to.', '#3b82f6', 0, 20, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('Promos', 'Marketing, sales, discounts and promotional offers.', '#a855f7', 0, 30, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('Receipts', 'Receipts, invoices, order confirmations and payment records.', '#22c55e', 0, 40, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('Spam', 'Unsolicited bulk mail, scams and phishing.', '#71717a', 0, 50, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('Review', 'Jev was not confident enough. A human should look at it.', '#f59e0b', 1, 1000, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
