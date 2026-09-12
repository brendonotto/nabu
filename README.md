# Nabu

Nabu is a private-first blogging platform for sharing writing, images, and short videos with invited readers. Authors may opt individual posts into public, indexable publishing.

The repository is a modular monolith:

- `server/` — Axum HTTP server, PostgreSQL migrations, and background worker
- `web/` — React and TypeScript authoring application
- `docs/architecture/` — accepted architecture decision records
- `docs/PLAN.md` — current implementation status and durable resume point

## Prerequisites

- Rust 1.98.1 (pinned by `rust-toolchain.toml`)
- Node.js 24 and pnpm 12
- Docker
- FFmpeg and PostgreSQL client tools

## Local development

```bash
cp .env.example .env
docker compose up -d postgres mailpit
cargo run --bin server        # terminal 1
cargo run --bin worker        # terminal 2
pnpm dev
```

The server listens on `127.0.0.1:3000` and the Vite application on `127.0.0.1:5173` by default. Vite proxies `/api` to Axum.
Development sign-in emails appear in Mailpit at `http://127.0.0.1:8025`.

Run the checks with:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
pnpm lint
pnpm build
```

## Database

The server applies embedded SQLx migrations at startup. To discard local data and start over:

```bash
docker compose down -v
```

## Configuration

Copy `.env.example` to `.env`. Secrets and deployment-specific values belong in environment variables and must not be committed.

`AUTH_SECRET` protects sign-in codes and CSRF tokens. Replace the development value with at least 32 random bytes and set `COOKIE_SECURE=true` in any HTTPS deployment.

The project is not yet licensed for redistribution. Choose a license before publishing it as an open-source, self-hostable project.
