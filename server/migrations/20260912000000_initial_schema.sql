CREATE EXTENSION IF NOT EXISTS citext;

CREATE TABLE accounts (
    id uuid PRIMARY KEY,
    email citext NOT NULL UNIQUE,
    email_verified_at timestamptz,
    display_name text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    disabled_at timestamptz
);

CREATE TABLE blogs (
    id uuid PRIMARY KEY,
    slug citext NOT NULL UNIQUE,
    title text NOT NULL,
    description text,
    default_comments_enabled boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz,
    CONSTRAINT blogs_slug_format CHECK (slug::text ~ '^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$')
);

CREATE TABLE blog_members (
    blog_id uuid NOT NULL REFERENCES blogs(id),
    account_id uuid NOT NULL REFERENCES accounts(id),
    role text NOT NULL CHECK (role IN ('owner', 'author')),
    invited_by uuid REFERENCES accounts(id),
    invite_token_hash text UNIQUE,
    invited_at timestamptz,
    accepted_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (blog_id, account_id)
);

CREATE UNIQUE INDEX blog_members_one_owner
    ON blog_members (blog_id)
    WHERE role = 'owner';

CREATE TABLE login_challenges (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES accounts(id),
    scope text NOT NULL CHECK (scope IN ('app', 'blog')),
    blog_id uuid REFERENCES blogs(id),
    code_hash text NOT NULL,
    attempts_remaining smallint NOT NULL CHECK (attempts_remaining >= 0),
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    requested_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT login_challenges_scope CHECK (
        (scope = 'app' AND blog_id IS NULL)
        OR (scope = 'blog' AND blog_id IS NOT NULL)
    )
);

CREATE TABLE sessions (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES accounts(id),
    scope text NOT NULL CHECK (scope IN ('app', 'blog')),
    blog_id uuid REFERENCES blogs(id),
    token_hash text NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sessions_scope CHECK (
        (scope = 'app' AND blog_id IS NULL)
        OR (scope = 'blog' AND blog_id IS NOT NULL)
    )
);

CREATE INDEX login_challenges_active_account
    ON login_challenges (account_id, expires_at DESC)
    WHERE consumed_at IS NULL;

CREATE TABLE posts (
    id uuid PRIMARY KEY,
    blog_id uuid NOT NULL REFERENCES blogs(id),
    slug citext NOT NULL,
    title text NOT NULL,
    summary text,
    content_json jsonb NOT NULL DEFAULT '{"type":"doc","content":[]}'::jsonb,
    content_schema_version integer NOT NULL DEFAULT 1,
    rendered_html text NOT NULL DEFAULT '',
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    publication_status text NOT NULL DEFAULT 'draft'
        CHECK (publication_status IN ('draft', 'published')),
    visibility text NOT NULL DEFAULT 'private'
        CHECK (visibility IN ('private', 'public')),
    comments_enabled boolean NOT NULL DEFAULT true,
    created_by uuid NOT NULL REFERENCES accounts(id),
    updated_by uuid NOT NULL REFERENCES accounts(id),
    published_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz,
    UNIQUE (blog_id, slug),
    UNIQUE (id, blog_id),
    CONSTRAINT posts_publication_time CHECK (
        (publication_status = 'draft' AND published_at IS NULL)
        OR (publication_status = 'published' AND published_at IS NOT NULL)
    ),
    CONSTRAINT posts_slug_format CHECK (slug::text ~ '^[a-z0-9][a-z0-9-]{0,78}[a-z0-9]$')
);

CREATE INDEX posts_public_listing
    ON posts (blog_id, published_at DESC)
    WHERE publication_status = 'published'
      AND visibility = 'public'
      AND deleted_at IS NULL;

CREATE TABLE blog_access_grants (
    id uuid PRIMARY KEY,
    blog_id uuid NOT NULL REFERENCES blogs(id),
    account_id uuid NOT NULL REFERENCES accounts(id),
    granted_by uuid NOT NULL REFERENCES accounts(id),
    invite_token_hash text NOT NULL UNIQUE,
    invited_at timestamptz NOT NULL DEFAULT now(),
    last_invitation_sent_at timestamptz,
    accepted_at timestamptz,
    revoked_at timestamptz,
    UNIQUE (blog_id, account_id)
);

CREATE TABLE post_access_grants (
    id uuid PRIMARY KEY,
    post_id uuid NOT NULL REFERENCES posts(id),
    account_id uuid NOT NULL REFERENCES accounts(id),
    granted_by uuid NOT NULL REFERENCES accounts(id),
    invite_token_hash text NOT NULL UNIQUE,
    invited_at timestamptz NOT NULL DEFAULT now(),
    last_invitation_sent_at timestamptz,
    accepted_at timestamptz,
    revoked_at timestamptz,
    UNIQUE (post_id, account_id)
);

CREATE TABLE media_assets (
    id uuid PRIMARY KEY,
    blog_id uuid NOT NULL REFERENCES blogs(id),
    uploaded_by uuid NOT NULL REFERENCES accounts(id),
    media_type text NOT NULL CHECK (media_type IN ('image', 'video')),
    state text NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'uploaded', 'processing', 'ready', 'failed')),
    original_object_key text NOT NULL UNIQUE,
    original_filename text NOT NULL,
    mime_type text NOT NULL,
    size_bytes bigint NOT NULL CHECK (size_bytes > 0 AND size_bytes <= 104857600),
    checksum text,
    width integer CHECK (width > 0),
    height integer CHECK (height > 0),
    duration_ms integer CHECK (duration_ms >= 0 AND duration_ms <= 30000),
    failure_reason text,
    created_at timestamptz NOT NULL DEFAULT now(),
    ready_at timestamptz,
    deleted_at timestamptz,
    UNIQUE (id, blog_id)
);

CREATE TABLE media_variants (
    id uuid PRIMARY KEY,
    asset_id uuid NOT NULL REFERENCES media_assets(id),
    variant text NOT NULL CHECK (variant IN ('thumbnail', 'display', 'original', 'video', 'poster')),
    object_key text NOT NULL UNIQUE,
    mime_type text NOT NULL,
    size_bytes bigint NOT NULL CHECK (size_bytes > 0),
    width integer CHECK (width > 0),
    height integer CHECK (height > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (asset_id, variant)
);

CREATE TABLE post_media (
    blog_id uuid NOT NULL REFERENCES blogs(id),
    post_id uuid NOT NULL,
    asset_id uuid NOT NULL,
    PRIMARY KEY (post_id, asset_id),
    FOREIGN KEY (post_id, blog_id) REFERENCES posts(id, blog_id),
    FOREIGN KEY (asset_id, blog_id) REFERENCES media_assets(id, blog_id)
);

CREATE TABLE comments (
    id uuid PRIMARY KEY,
    post_id uuid NOT NULL REFERENCES posts(id),
    author_account_id uuid NOT NULL REFERENCES accounts(id),
    body text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 10000),
    status text NOT NULL DEFAULT 'visible'
        CHECK (status IN ('visible', 'hidden', 'deleted')),
    created_at timestamptz NOT NULL DEFAULT now(),
    edited_at timestamptz,
    moderated_by uuid REFERENCES accounts(id),
    moderated_at timestamptz
);

CREATE INDEX comments_post_created ON comments (post_id, created_at);

CREATE TABLE jobs (
    id uuid PRIMARY KEY,
    kind text NOT NULL,
    payload jsonb NOT NULL,
    run_after timestamptz NOT NULL DEFAULT now(),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    locked_at timestamptz,
    completed_at timestamptz,
    last_error text
);

CREATE INDEX jobs_available ON jobs (run_after)
    WHERE completed_at IS NULL AND locked_at IS NULL;

CREATE TABLE email_outbox (
    id uuid PRIMARY KEY,
    recipient_account_id uuid NOT NULL REFERENCES accounts(id),
    template text NOT NULL,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    sent_at timestamptz
);

CREATE INDEX email_outbox_unsent ON email_outbox (created_at)
    WHERE sent_at IS NULL;

CREATE TABLE audit_events (
    id uuid PRIMARY KEY,
    blog_id uuid REFERENCES blogs(id),
    actor_account_id uuid REFERENCES accounts(id),
    action text NOT NULL,
    subject_type text NOT NULL,
    subject_id uuid NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_blog_created ON audit_events (blog_id, created_at DESC);
