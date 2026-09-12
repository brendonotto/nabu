# ADR 0001: Modular monolith and application stack

- Status: Accepted
- Date: 2026-09-12

## Context

Nabu needs an authoring application, server-rendered public blogs, authenticated private reading, media processing, email delivery, and background jobs. It must support both a hosted service and self-hosted installations without maintaining two products.

## Decision

Build a modular monolith with:

- Rust and Axum for HTTP, server rendering, authorization, and application services;
- React and TypeScript for the authoring application and interactive reader features;
- PostgreSQL for durable application data and the initial job queue;
- one shared Rust library exposed through an HTTP server binary and a worker binary.

Modules own identity, blogs, posts, access, media, comments, rendering, and jobs. They may share one deployment and database, but authorization and domain behavior must remain in their owning module rather than route handlers.

## Consequences

The system is straightforward to deploy and transact across. Rust increases implementation effort compared with batteries-included web frameworks, but provides strong type and concurrency guarantees. Microservices and a dedicated queue are deferred until measured operating needs justify them.
