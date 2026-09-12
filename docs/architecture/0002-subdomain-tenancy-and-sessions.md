# ADR 0002: Subdomain tenancy and host-scoped sessions

- Status: Accepted
- Date: 2026-09-12

## Context

Each blog should feel like an independent space. A parent-domain authentication cookie would simplify login but would expose a powerful credential to every tenant host.

## Decision

Blogs launch at `{slug}.<root-domain>`. The authoring application runs at `app.<root-domain>`, and Axum resolves and validates the request host before routing.

Use opaque, server-side sessions in `HttpOnly`, `Secure`, `SameSite=Lax` cookies. Author sessions are host-only for the app host. Reader and commenter sessions are host-only for one blog. Credentials are never stored in browser local storage.

Blog slugs are case-insensitively unique, reserve system names, and are not reassigned after deletion. Wildcard DNS and TLS are deployment prerequisites.

## Consequences

Tenant identity and security isolation are stronger, and future custom domains fit the host-routing model. Readers may need to verify their email separately on different blogs. Development, TLS, analytics, and search-engine configuration are more involved than path-based tenancy.
