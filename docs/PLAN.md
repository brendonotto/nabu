# Nabu implementation plan

This is the durable resume point for implementation. Update it when a milestone starts or finishes, when a consequential decision changes, and when verification reveals a blocker.

## Current milestone

### M2 — Post CRUD and optimistic autosaving

Status: Ready to start

Outcome: An author can create, list, open, edit, autosave, and delete private draft posts without a stale browser overwriting a newer revision.

Work:

- [ ] Define post request/response contracts and authorization queries.
- [ ] Create and list private drafts for the authenticated author's blog.
- [ ] Save title, slug, summary, and initial document content with revision checks.
- [ ] Return `409 Conflict` with the current revision for stale saves.
- [ ] Soft-delete drafts while preserving slug reservations.
- [ ] Build the authoring post list and draft shell in React.
- [ ] Add debounced autosave state and conflict recovery UX.
- [ ] Add integration and browser tests for the complete draft lifecycle.

The rich block editor remains M3. M2 should use the same versioned JSON and HTML fields with a minimal text surface so the persistence and conflict contract is settled first.

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

## Next milestones

1. M3 — Tiptap block editor and sanitized HTML projection
2. M4 — Axum-rendered public posts, RSS, and sitemaps
3. M5 — Reader invitations and private post access
4. M6 — Image and short-video processing
5. M7 — Comments and moderation

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
