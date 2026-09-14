# Nabu implementation plan

This is the durable resume point for implementation. Update it when a milestone starts or finishes, when a consequential decision changes, and when verification reveals a blocker.

## Current milestone

### M5 — Reader invitations and private post access

Status: Ready to start

Outcome: Authors can invite readers by email to a whole blog or individual posts, and verified readers can return on the same device without requesting a code each time.

Work:

- [ ] Add author-managed blog and post invitation APIs with email delivery and revocation.
- [ ] Verify invited addresses with short-lived, attempt-limited email codes on the tenant host.
- [ ] Persist host-only reader sessions and check active grants on every protected request.
- [ ] Render authorized private posts without exposing them through metadata, feeds, or sitemaps.
- [ ] Return indistinguishable `404` responses for missing, unauthorized, and revoked content.
- [ ] Add tenant, grant-scope, revocation, session-persistence, and browser integration coverage.

## Completed milestones

### M0 — Architecture and scaffold

Status: Complete

- Accepted six architecture decision records.
- Created the Rust workspace, Axum server and worker, React application, PostgreSQL schema, CI, and orb setup.
- Verified Rust checks, frontend checks, initial migration, tenant-safe media references, and responsive rendering.

### M1 — Author authentication and blog onboarding

Status: Complete

- Classifies app and single-label tenant hosts before API routing.
- Requests 10-minute email codes with five attempts and one-minute account throttling.
- Derives codes with a server HMAC secret; PostgreSQL stores no plaintext code.
- Persists 30-day opaque sessions in host-only `HttpOnly`, `SameSite=Lax` cookies and stores only token digests.
- Requires a session-derived CSRF header for blog creation and logout.
- Delivers codes from a retryable transactional outbox through a narrow SMTP sender boundary; local Compose includes Mailpit.
- Creates one blog and its owner membership atomically with reserved and unique slug validation.
- Implements responsive React states for sign-in, code verification, onboarding, persisted completion, and logout.
- Verification: Rust formatting and Clippy passed; 7 Rust tests passed against PostgreSQL; frontend lint and production build passed; both migrations applied; the SMTP worker delivered a code without storing it in outbox payload; browser acceptance completed the entire flow, reloaded the session, and signed out; desktop and 390px captures were inspected with no layout defects.

### M2 — Post CRUD and optimistic autosaving

Status: Complete

- Added tenant-authorized APIs to create, list, open, update, and soft-delete private drafts.
- Stores a versioned ProseMirror-style JSON document plus backend-escaped HTML projection.
- Requires the last known revision for updates and deletes; stale requests return `409 Conflict` with the current revision.
- Preserves deleted slugs and hides posts across blog boundaries.
- Added a responsive React draft list and plain-text authoring surface with debounced autosave, save state, conflict recovery, and deletion.
- Verification: Rust formatting and Clippy passed; 10 Rust tests passed against PostgreSQL; frontend lint and production build passed; browser acceptance covered create, autosave, reload, simulated stale-tab conflict, recovery, and delete; desktop and 390px captures were inspected.

### M3 — Rich block editor and sanitized HTML projection

Status: Complete

- Documented a constrained version-1 ProseMirror schema for prose, headings, lists, quotes, code, links, and formatting marks.
- Replaced the transitional plain-text payload with canonical structured JSON while retaining optimistic revision checks.
- Validates document structure, nesting, node placement, marks, attributes, URL protocols, and text limits in Axum.
- Generates escaped, safe HTML exclusively from validated backend content; client HTML is never accepted.
- Added an accessible Tiptap editor and formatting toolbar while preserving autosave and conflict recovery.
- Verification: Rust formatting and Clippy passed; 11 Rust tests passed against PostgreSQL; frontend lint and production build passed; malformed content, unsafe URLs, unknown attributes, HTML escaping, rich-content persistence, and browser formatting/autosave were exercised; the rendered editor was inspected.

### M4 — Axum-rendered public posts, RSS, and sitemaps

Status: Complete

- Added revision-checked publication transitions that deliberately opt a post into public, indexable access while preserving the publication timestamp when it returns to private.
- Added clear authoring status and controls, including a warning that removal from Nabu's public surfaces cannot immediately remove search-engine caches.
- Rendered public blog indexes and post pages through escaped Askama templates selected by tenant subdomain, with canonical and Open Graph metadata.
- Added per-blog RSS, sitemap, and robots responses; blogs with no public posts expose no blog title or public index and instruct crawlers not to enter.
- Public queries require an active blog and an undeleted post with both `published` and `public` state; tenant, draft, private, stale-revision, and escaping boundaries have integration coverage.
- Verification: Rust formatting and Clippy passed; 12 Rust tests passed against PostgreSQL; frontend lint and production build passed; browser acceptance covered publish, public SSR, the public-to-private warning, immediate `404`, and republishing; desktop and 390px author views and the public post were inspected without layout defects.

## Next milestones

1. M6 — Image and short-video processing
2. M7 — Comments and moderation

## Production gates

- Put the email-code endpoints behind trusted-edge per-IP rate limiting. The application already
  throttles each account and challenge scope, but safe client-IP enforcement requires a deployment
  boundary that strips untrusted forwarding headers.

## Deferred

- Platform-wide public directory and search
- Custom domains
- Scheduled publication
- Concurrent document editing and automatic merge
- Nested comments
- Managed video transcoding
