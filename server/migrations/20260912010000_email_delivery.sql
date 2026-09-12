ALTER TABLE email_outbox
    ADD COLUMN attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    ADD COLUMN locked_at timestamptz,
    ADD COLUMN next_attempt_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN last_error text;

DROP INDEX email_outbox_unsent;

CREATE INDEX email_outbox_available
    ON email_outbox (next_attempt_at, created_at)
    WHERE sent_at IS NULL;
