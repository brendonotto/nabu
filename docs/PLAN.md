# Nabu implementation plan

This is the durable resume point for implementation. Update it when a milestone starts or finishes, when a consequential decision changes, and when verification reveals a blocker.

## Current milestone

### M3 — Rich block editor and sanitized HTML projection

Status: Ready to start

Outcome: Authors can compose structured posts with a friendly Tiptap editor while the backend validates canonical JSON and owns the safe HTML projection used for rendering.

Work:

- [ ] Choose the initial Tiptap extension allowlist and document it as the version 1 content schema.
- [ ] Replace the plain-text API field with canonical structured JSON while retaining revision checks.
- [ ] Validate document shape, node types, marks, link protocols, and content-size limits in Axum.
- [ ] Generate sanitized HTML exclusively on the backend from validated content.
- [ ] Build the rich editor toolbar, keyboard interactions, empty state, and accessible labels.
- [ ] Preserve debounced autosave and explicit stale-revision recovery in the rich editor.
- [ ] Add round-trip, malformed-document, XSS, integration, and browser coverage.

Images and video blocks remain M6; M3 should establish extensible content boundaries without exposing storage concerns yet. Existing M2 documents already use compatible `doc` and `paragraph` nodes.

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

## Next milestones

1. M4 — Axum-rendered public posts, RSS, and sitemaps
2. M5 — Reader invitations and private post access
3. M6 — Image and short-video processing
4. M7 — Comments and moderation

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
