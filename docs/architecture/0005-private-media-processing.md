# ADR 0005: Private media processing

- Status: Accepted
- Date: 2026-09-12

## Context

Images and videos must follow post authorization. The initial product also needs to remain portable rather than requiring a hosted transcoding vendor.

## Decision

Store originals and variants in private S3-compatible object storage. Browsers upload directly using short-lived signed upload credentials. A worker validates actual type and size before producing variants.

Videos are limited to 100 MB and 30 seconds, verified with `ffprobe`, then normalized with FFmpeg to H.264 video and AAC audio with a poster image. Media documents contain opaque asset identifiers, never storage URLs. Publication is blocked while referenced media is not ready.

Media downloads are authorized by Axum and redirected to short-lived signed URLs. A media asset is publicly reachable while referenced by any public post.

## Consequences

The default works with self-hosted S3-compatible storage and FFmpeg. Hosted deployments may later add a managed transcoding adapter. Signed private downloads create a bounded revocation delay; immediate byte-level revocation would require proxying media through Axum.
