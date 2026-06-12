# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Firefly is a cross-platform desktop chat client (Tauri 2 + Svelte 5 + Rust). It is
primarily a **client**: inference runs on remote tiers (Firefly home base, the device's
NPU/GPU, or cloud via LiteLLM), not in this app. The app routes each request to a tier,
streams the reply, and (later phases) syncs history through Firefly over Tailscale.

`PLAN-app-build.md` is the authoritative build plan: phases, the routing spec (§6), the
on-device readiness spec (§6.1), the sync spec (§7), and the **Guardrails (§4)**. Read the
Guardrails before changing routing, secrets, or storage. Work is tracked by phase; the
current branch is `phase-2-router`.

`DESIGN.md` is the authoritative **visual** reference ("Velvet Glow": dark violet
surfaces, one violet gradient accent, soft pills, a rationed amber firefly spark, glow
effects, Baloo 2 + Nunito). Read it before any UI/CSS change or new screen. The design
system is vendored in `src/lib/styles/` (tokens + `.ff-*` classes, loaded once via
`src/routes/+layout.svelte`); fonts are bundled in `static/fonts/`. Always style with the
`--ff-*` tokens and `.ff-*` classes rather than hardcoded values.

## Workflow

All feature development happens on a feature branch, never directly on `main`. Branch off
`main` before starting work and open a PR to merge back; keep `main` clean and releasable.

## Commands

Run frontend/Tauri commands from the repo root; Rust commands from `src-tauri/`.

- `npm run tauri dev` — run the full app (spawns Vite, then the Tauri shell)
- `npm run tauri build` — build distributables (`.AppImage`/`.deb` on Arch, `.dmg`/`.app` on macOS)
- `npm run dev` — Vite frontend only (no Rust core; most commands will fail)
- `npm run check` — `svelte-check` type checking (TS is `strict`, `checkJs` on)
- `cd src-tauri && cargo test` — Rust unit tests (router logic is fully unit-tested in `router.rs`)
- `cd src-tauri && cargo test resolve_route` — run a single test by name
- `cd src-tauri && cargo clippy` — Rust lints

## Architecture

The Rust core owns all network, storage, routing, and secrets. The Svelte webview talks to
it **only** through Tauri commands and `Channel`s — it never calls LLMs and never holds
secrets. This boundary is a hard guardrail, not a convention.

**Frontend (`src/`)** — Svelte 5 runes (`$state`, `$props`), SPA mode (`adapter-static`,
`ssr = false` in `+layout.ts`).
- `src/lib/api.ts` — the single typed bridge to the core. Every Tauri command is wrapped
  here; TS types mirror the Rust `serde` shapes (camelCase). Streaming commands
  (`sendMessage`, `pullOnDeviceModel`) create a `Channel` and forward events to a callback.
- `src/lib/Chat.svelte`, `ConversationList.svelte`, `src/routes/+page.svelte` — UI.

**Rust core (`src-tauri/src/`)**
- `lib.rs` — all `#[tauri::command]` definitions, `AppState` (SQLite pool + reachability
  cache), settings load/save with defaults, and the command registry in `run()`. Adding a
  command means: write the fn, add it to `generate_handler!`, and wrap it in `api.ts`.
- `router.rs` — **pure** tier-selection logic (`resolve_route`), reachability TTL helpers,
  and endpoint normalization. No IO in `resolve_route` — it is unit-tested. Reachability
  probing (`check_reachable`) is the only IO here.
- `llm.rs` — OpenAI-compatible SSE streaming client. Parses `delta.content` and
  `delta.reasoning_content`, emits `StreamEvent`s over a `Channel`, returns accumulated text.
- `store.rs` — SQLite via `sqlx`. Schema is created/migrated in `init_pool` keyed off
  `PRAGMA user_version` against `SCHEMA_VERSION`; add a new `if version < N` block to migrate.
- `secrets.rs` — OS keychain access (`keyring` crate, per-OS native backends). The device/
  LiteLLM token is read here and **only** for home-base/cloud routes.
- `sync.rs` — placeholder for the Phase 3 sync client.
- `error.rs` — `AppError` + `Result`; serializes to a string for the frontend.

### Request flow (`send_message`)

1. Persist the user message to SQLite immediately.
2. Load settings, check Firefly reachability (cached ~5s in `ReachabilityCache`).
3. `router::resolve_route(task, inputs)` picks tier + endpoint + logical model + whether a
   token is needed.
4. Read the keychain token **only if** `route.use_token`.
5. Emit `Started` then `Routed{tier,model,degraded}` (drives the UI badge), insert an empty
   assistant message, record its routing, then stream tokens and finalize its content.

## Invariants (from PLAN-app-build.md §4 — do not violate)

- **Secrets never reach the webview or JS bundle.** Tokens live in the OS keychain, read only
  by the Rust core. Never return a token from a command or log it.
- **No browser storage** (`localStorage`/`sessionStorage`) for app data — use SQLite via the core.
- **Endpoints are settings, not hardcoded.** Defaults (Tailscale MagicDNS names) live as
  `DEFAULT_*` consts in `lib.rs`; real values come from the `settings` table.
- **The client picks the *tier*; LiteLLM picks the *model* for home-base/cloud.** Send a
  *logical* model name (`code`/`chat-heavy`/`frontier`) to home base; only on-device routes
  carry a concrete model ID (it is a per-device setting).
- **`private` and `quick` never leave the device**, and `private` must stay on-device even
  when Firefly is reachable. This is enforced in `router.rs`, not just the UI. Keep routing
  decisions in the router and keep `resolve_route` pure so they stay testable.
- **On-device readiness detects and pulls; it never installs.** Privileged installs are an
  explicit non-goal — show copy-paste commands instead. Pulls go through the server API
  (Ollama `POST /api/pull`, streamed), never a shell.
