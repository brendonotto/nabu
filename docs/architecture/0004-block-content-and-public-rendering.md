# ADR 0004: Block content and public rendering

- Status: Accepted
- Date: 2026-09-12

## Context

The editor must be approachable for non-technical authors, while public posts need complete indexable HTML without operating a Node server-rendering tier.

## Decision

Use Tiptap on ProseMirror with a constrained schema for prose, links, lists, quotes, code, dividers, images, galleries, videos, captions, and alt text.

Persist the canonical ProseMirror JSON, a content-schema version, and a sanitized HTML projection. On save, Axum validates the document and referenced media, sanitizes the submitted projection with a strict allowlist, and stores both atomically. Public pages, metadata, feeds, and sitemaps are rendered by Axum using Askama templates.

Posts use optimistic revisions. A stale save receives `409 Conflict`; automatic merging, concurrent editing, and scheduled publication are deferred.

## Consequences

Editing remains structured and migratable while public rendering requires no JavaScript runtime. The backend must maintain document validation, HTML sanitization, and schema migrations. Generated HTML is never trusted solely because it came from the first-party frontend.
