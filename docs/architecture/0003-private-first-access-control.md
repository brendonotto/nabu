# ADR 0003: Private-first publishing and email-code access

- Status: Accepted
- Date: 2026-09-12

## Context

Authors need to share an entire blog or one post with specific email addresses while optionally publishing individual posts to the open web.

## Decision

Publication state (`draft` or `published`) and visibility (`private` or `public`) are independent. New posts are private drafts. A published post is readable when it is public, the account has an active blog grant, the account has an active post grant, or the account is a blog member.

Only owners and authors issue reader invitations. An opaque invitation token identifies the destination but is not authorization. The reader proves control of the invited address using a short-lived, single-use, attempt-limited email code. Grants are checked on every protected request, and inaccessible private resources return `404`.

Switching a post from private to public requires explicit acknowledgement that third parties may index, cache, or copy it. Returning it to private removes it from Nabu's feeds and sitemaps but cannot retract external copies.

## Consequences

Revocation affects subsequent authorization checks without requiring session deletion. Private media URLs can remain valid for their short signing lifetime. Nabu prevents anonymous access and indexing but cannot prevent an authorized reader from copying content.
