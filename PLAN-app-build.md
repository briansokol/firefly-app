# Unified AI Client App — Build Plan

> **For Claude Code.** This is an implementation plan for a new cross-platform desktop app (mobile later). Work the phases in order; each has acceptance criteria that must pass before moving on. Read **Guardrails** first. The server-side contract this app depends on is defined in `PLAN-firefly-upgrade.md` — treat that as the source of truth for endpoint shapes.

---

## 1. What We're Building

A single chat application for one family, running on every machine they own, that routes each request to the right model tier and syncs history + memories through **Firefly** (home base). It is primarily a **client**: inference happens on Firefly, the laptop's NPU, or the Mac — not in this app, except for an optional on-device fallback added much later.

---

## 2. Tech Stack (pinned)

| Layer | Choice |
|---|---|
| App framework | **Tauri 2** (2.10+) — desktop now, iOS/Android later from the same codebase |
| Frontend | **Svelte 5** (runes) + **Vite**; TypeScript |
| Backend | **Rust** (Tauri commands) — owns network, storage, routing, secrets |
| Local store | **SQLite** via `tauri-plugin-sql` |
| Secret storage | OS keychain (e.g. `tauri-plugin-keyring` or `tauri-plugin-stronghold`) — **never** plaintext |
| Transport | OpenAI-compatible `/v1/chat/completions` over Tailscale; SSE streaming |

Targets this phase: Arch Linux (`.AppImage` + `.deb`), macOS (`.dmg` + `.app`). Mobile is Phase 6 (deferred).

---

## 3. Architecture

```
┌──────────────────────── TAURI APP (per device) ────────────────────────┐
│  Svelte 5 frontend (webview)                                            │
│    • chat UI, conversation list, streaming render, model/tier indicator │
│    • talks to the Rust core ONLY via Tauri commands + events            │
│        (the webview never holds secrets and never calls LLMs directly)  │
│                                                                         │
│  Rust core (Tauri commands)                                             │
│    • router:   task hint + Tailscale reachability → tier                │
│    • llm:      stream from chosen endpoint, emit tokens as events       │
│    • store:    local SQLite (conversations, messages, sync cursor)      │
│    • sync:     push/pull deltas against Firefly sync service            │
│    • secrets:  read LiteLLM/device tokens from OS keychain              │
└─────────────────────────────────────────────────────────────────────────┘
        on-device tier          home-base tier            cloud tier
   (offline / privacy)       (default when on net)     (via LiteLLM only)
   NPU(Lemonade)/MLX        Firefly LiteLLM :4000      frontier fallback
```

**Why the Rust core owns LLM calls:** keeps the master/device tokens out of the JS bundle, enables proper SSE streaming, and lets routing/connectivity checks run natively.

---

## 4. Guardrails (read before coding)

- **Secrets never touch the webview or the JS bundle.** Tokens live in the OS keychain, read by the Rust core. The frontend asks the core to make calls; it never sees the key.
- **No browser storage APIs** (`localStorage`/`sessionStorage`) for app data — use SQLite via the Rust core.
- **Endpoints are configurable, not hardcoded.** Firefly/laptop/Mac addresses come from app settings (Tailscale hostnames), defaulting to MagicDNS names.
- **The client picks the *tier*; LiteLLM picks the *model*.** The app sends a **logical** model name (`fast`/`code`/`chat-heavy`/`frontier`) to the home-base tier and lets Firefly map it. Don't bake backend-specific model IDs into the app.
- **Privacy-critical requests must never reach the cloud tier** — enforce in the router, not just the UI.

---

## 5. Repo Layout

```
unified-ai-app/
├─ src/                      # Svelte 5 frontend
│  ├─ lib/
│  │  ├─ Chat.svelte
│  │  ├─ ConversationList.svelte
│  │  └─ api.ts              # thin wrapper over Tauri invoke()/listen()
│  └─ App.svelte
├─ src-tauri/
│  ├─ src/
│  │  ├─ lib.rs
│  │  ├─ router.rs           # tier selection
│  │  ├─ llm.rs              # SSE streaming client
│  │  ├─ store.rs            # SQLite access
│  │  ├─ sync.rs             # delta sync client
│  │  └─ secrets.rs          # keychain access
│  ├─ tauri.conf.json        # capabilities: http, sql, keychain — least privilege
│  └─ Cargo.toml
└─ package.json
```

---

## 6. Routing Spec

The Rust router maps a request to a **tier**, then a concrete endpoint. Inputs: a `task` hint from the UI and live **Tailscale reachability** to Firefly.

| Task hint | Preferred tier | Logical model | If Firefly unreachable |
|---|---|---|---|
| `quick` (one-liner, voice) | on-device | — (local model) | stay on-device |
| `code-complete` | on-device, else home-base | `code` | on-device |
| `write` / `explain-file` | home-base | `code` | on-device (degraded) |
| `agentic` | home-base | `chat-heavy` | on-device (degraded) |
| `private` | **on-device only** | — | on-device |
| `best` | cloud via home-base | `frontier` | refuse / queue |

Logic:
1. If `task == private` → on-device endpoint only; never anything else.
2. Else if Firefly reachable (ping its tailnet address) → home-base: POST the logical model to `http://firefly:4000/v1/chat/completions` with the device token. LiteLLM handles model choice + cloud fallback.
3. Else → on-device endpoint for this machine (per-device, editable in settings):
   - **Framework (Arch):** default iGPU Ollama `:11434/v1` running `qwen3.6:35b-a3b`; alternative NPU FastFlowLM `:52625/v1` running `gpt-oss:20b`. Both are selectable as the on-device endpoint; default to the Ollama path so on-device stays uniform with the Mac.
   - **Kohtaro (Mac):** Ollama (or MLX) `:11434/v1` running `qwen3.6:27b`.

   Unlike the home-base tier (where LiteLLM picks the model), on-device requests hit the runner directly, so the concrete model ID is part of the per-device setting. Surface a "degraded / local" badge in the UI.

On-device endpoints are configured per-device in settings. **Deferred (Phase 6):** an on-device tier *on the phone itself* via a small GGUF model through llama.cpp in the Rust core (runs on the GPU/Metal, not the ANE — do not attempt Neural Engine integration).

### 6.1 On-device Readiness Check

The app **detects and pulls**, it does **not install**. Installing inference servers and their drivers (Ollama, FastFlowLM; ROCm/Vulkan for the iGPU, the `amdxdna` NPU driver) is system-level, privileged, and OS/hardware-specific — it stays the user's responsibility and is an explicit **non-goal**. The app owns only the parts it can do safely as an unprivileged client: detecting server state and pulling models through the server's own API.

On first run and from Settings, probe the configured on-device endpoint and surface one of three states per device:

1. **Ready** — server reachable *and* the configured `on_device_model` is present → green; no action.
2. **Model missing** — server reachable but the model isn't installed → offer a one-click **Pull `<model>` (~size)** with a streamed progress bar and cancel. Drive it through the server API, never a shell install:
   - Ollama: `GET /api/tags` to list installed models; `POST /api/pull` (streamed) to fetch.
   - FastFlowLM: `flm list` / health on `:52625` to check; `flm pull <model>` to fetch.
3. **Server unreachable / not installed** — red; show copy-paste install + run commands for the detected OS and a link to prereqs. **Do not** auto-run installers or elevate privileges.

After a successful pull, set the context length explicitly (Ollama `num_ctx` via Modelfile/API options) so agentic requests aren't silently truncated by the 4096 default. The readiness check is advisory: it never blocks sending, and the router's normal degrade/badge behavior still applies if a probe is stale.

---

## 7. Sync Spec (client side)

- Local SQLite mirrors the Firefly schema (conversations, messages) plus a `sync_state(cursor)` row.
- **Write path:** new messages insert locally immediately (UI is instant); a background task flushes to `POST /sync/push` when Firefly is reachable.
- **Read path:** on launch and on reconnect, `GET /sync/pull?since=<cursor>`, merge rows (messages are append-only by UUID; conversations/memories last-write-wins by `updated_at`), advance the cursor.
- **Offline:** fully usable against local SQLite; queued pushes drain on reconnect.
- **Memory injection:** before sending a home-base request, call `GET /memories/search?user=&q=<latest user msg>&k=8` and prepend the returned memories to the system prompt. Skip for `private`/offline.

---

## 8. Phased Tasks

### Phase 1 — Skeleton + streaming chat
- [ ] `npm create tauri-app` with the Svelte + TypeScript template; add `tauri-plugin-sql`.
- [ ] Chat UI: message list, input box, streaming token render, conversation switcher.
- [ ] Rust `llm.rs`: POST to `firefly:4000` with `stream:true`, parse SSE, emit each token as a Tauri event the frontend listens on.
- [ ] Store LiteLLM/device token in the OS keychain; read it in the Rust core only.
- [ ] Persist conversations/messages to local SQLite.
- [ ] Build artifacts for Arch (`.AppImage`/`.deb`) and macOS (`.dmg`).
- **Accept:** real streaming chat against Firefly on both machines; history survives an app restart; the token is not present anywhere in the webview or JS bundle.

### Phase 2 — Client router
- [ ] Implement the tier table from §6; add a `task` selector (or heuristic) in the UI.
- [ ] Tailscale reachability check to Firefly with a short timeout; cache the result briefly.
- [ ] On-device endpoint settings per device; fallback path when Firefly is down.
- [ ] Enforce `private` → on-device-only in the router (not just UI).
- [ ] UI badge showing the tier/model that actually served each response.
- [ ] On-device readiness check per §6.1: probe the endpoint, classify ready / model-missing / unreachable, and surface state in Settings.
- [ ] One-click model pull (Ollama `POST /api/pull`, FastFlowLM `flm pull`) with streamed progress and cancel; set `num_ctx` after pull. No privileged installs — show manual commands when the server is absent.
- **Accept:** with Firefly up, `agentic` hits home-base; with Firefly unreachable, the same request degrades to on-device; `private` never leaves the device even when Firefly is up. The readiness check correctly reports all three states, and a missing on-device model can be pulled from Settings with visible progress.

### Phase 3 — Sync client
- [ ] Local schema + `sync_state`; device registration against `POST /devices/register`.
- [ ] Background push/pull per §7; idempotent merge keyed by UUID; cursor advance.
- [ ] Conflict handling: append-only messages; last-write-wins for conversation titles.
- **Accept:** start a conversation on the Mac, continue it on the laptop after a sync; offline edits reconcile on reconnect with no dupes.

### Phase 4 — Memory injection
- [ ] Before home-base sends, query `/memories/search` and prepend results to the system prompt.
- [ ] Setting to toggle memory use; never inject for `private`/offline requests.
- **Accept:** a fact stated on one device visibly informs a later answer on another device.

### Phase 5 — Multi-user
- [ ] Per-user device tokens mapped to LiteLLM virtual keys (see `PLAN-firefly-upgrade.md` F3).
- [ ] Profile switcher; per-user conversation list and memory scope.
- [ ] Kid profile uses a restricted key (`fast`/`chat-heavy` only); cloud/code unavailable to it.
- **Accept:** switching profiles shows only that user's history; a kid profile cannot invoke `frontier` or `code`.

### Phase 6 — Mobile (deferred)
- [ ] `tauri ios init` / Android build; verify the Tailscale-client flow on a phone.
- [ ] (Optional, much later) on-device GGUF via llama.cpp in the Rust core — **GPU/Metal only, not the ANE.**
- **Accept:** the same app runs on a phone as a thin Tailscale client.

---

## 9. Environment / Settings

- Tailscale hostnames for Firefly + this device's local endpoints (editable in app settings; default to MagicDNS names).
- Device token (issued by the sync service at registration), stored in the OS keychain.
- No `.env` with secrets shipped in the bundle.

---

## 10. Build & Distribution

- `npm run tauri build` per target. Sign the macOS build for Gatekeeper if distributing to family Macs.
- Keep bundles tiny (Tauri's webview model) — avoid pulling heavy JS deps into the frontend.
