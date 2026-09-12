# ADR 0006: Comments and public discovery

- Status: Accepted
- Date: 2026-09-12

## Context

Readers should be able to respond without exposing private posts or creating an anonymous-spam system. Public posts should be discoverable, but a platform directory would create immediate moderation obligations.

## Decision

Reading comments follows post authorization. Commenting requires a verified, blog-scoped email session, including on public posts. Initial comments are flat plain text. Authors may disable comments per post and hide or soft-delete comments.

Public posts are discoverable through complete server-rendered HTML, canonical metadata, RSS, and sitemaps. A platform-wide directory, feed, and search experience are deferred.

## Consequences

Verified identities reduce spam and reuse the reader authentication system. Moderation remains necessary but bounded. Search engines can discover public posts without Nabu operating a recommendation or discovery surface.
