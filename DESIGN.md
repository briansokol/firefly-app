# DESIGN.md — Firefly visual system ("Velvet Glow")

This is the authoritative reference for Firefly's look and feel. Read it before
changing anything visual (CSS, component markup, new screens). It was derived from
the `firefly-design-system` handoff bundle (Claude Design export). The bundle's
source HTML/CSS prototypes are not in this repo; their tokens and component styles
are vendored here under `src/lib/styles/`.

## TL;DR

Dark only. Deep violet-tinted surfaces, one violet gradient accent, soft pill
shapes, and a rationed warm amber "firefly spark". Light is the brand: primary
actions and active states emit a soft glow. Warm, helpful, lightly playful tone.
No emoji in UI chrome.

## Where the system lives

| File | Purpose |
| --- | --- |
| `src/lib/styles/firefly.css` | Global entry point. Imports everything below + base reset. Imported once in `src/routes/+layout.svelte`. |
| `src/lib/styles/colors.css` | Surface ramp, accent, text, line, semantic color tokens. |
| `src/lib/styles/typography.css` | Font tokens, type scale, `.ff-root` base, `.ff-display`, `.ff-overline`. |
| `src/lib/styles/spacing.css` | Spacing scale, radii, control sizing. |
| `src/lib/styles/effects.css` | Glows, shadows, focus ring, motion easings/durations. |
| `src/lib/styles/components.css` | Drop-in `.ff-*` component classes. |
| `src/lib/styles/fonts.css` | Local `@font-face` for Baloo 2 + Nunito. |
| `static/fonts/*.woff2` | Bundled webfonts (latin + latin-ext, served at `/fonts/...`). |

The `<body>` carries `class="ff-root"` (`src/app.html`) so tokens and base type
apply app-wide. Components use the `--ff-*` custom properties and the global
`.ff-*` classes directly in markup; per-component `<style>` blocks handle only
layout.

## Rules (do not violate)

- **Always use tokens.** Never hardcode a hex color, px radius, glow, or easing
  that a token already covers. If you reach for a raw value, check `colors.css`,
  `effects.css`, and `spacing.css` first. New values get added as tokens, not
  inlined.
- **Dark only.** There is no light mode. Do not add `prefers-color-scheme` light
  variants.
- **One accent.** The violet gradient (`--ff-grad-accent`) is the only accent.
  Use it on primary actions, user bubbles, checked states, and the user avatar.
- **Ration the amber.** `--ff-firefly` (warm amber) is reserved for the brand
  mark (`.ff-spark`), the AI avatar dot, and the adult profile badge
  (`.ff-badge--warm`). Do not use amber as a general accent.
- **Borderless first.** Elevation comes from the surface ramp + glows + soft
  black shadows, not borders. Where a line is unavoidable, use the translucent
  violet hairlines (`--ff-line-1` / `--ff-line-2`), never gray.
- **No transparency/blur surfaces.** Surfaces are opaque.
- **Backgrounds stay flat.** At most one huge, very faint off-canvas radial
  violet bloom per screen (see chat + settings + onboarding). No images,
  patterns, or noise.
- **No emoji in UI chrome.** Personality lives in light and soft shapes, not in
  jokey copy or emoji. (The `＋` in "New conversation" is a glyph, not emoji.)
- **Secrets/storage guardrails are unaffected by design.** Styling never moves
  data into the webview, `localStorage`, or hardcoded endpoints (see
  `CLAUDE.md` / `PLAN-app-build.md` §4).

## Color

Four-step violet-dark surface ramp, plus a separate darker titlebar:

- `--ff-bg-0` `#131021` app background
- `--ff-bg-1` `#1a1530` panels, sidebar, cards, AI bubbles
- `--ff-bg-2` `#241d40` controls, inputs, chips
- `--ff-bg-3` `#2d2450` active / hover surfaces
- `--ff-bg-titlebar` `#0e0b1a`

Accents: `--ff-violet-500` `#8b5cf6` → `--ff-violet-300` `#c084fc` (gradient ends);
`--ff-firefly` `#ffb86b`; `--ff-green` `#6ee7b7` (online/success);
`--ff-red` `#ff8d9a` (danger). Text: `--ff-text-1/2/3` (primary/secondary/dim).
Prefer the semantic aliases (`--ff-surface-*`, `--ff-text-body/muted/dim`,
`--ff-accent*`, `--ff-status-*`) in app code.

## Type

- Display: **Baloo 2** (`--ff-font-display`, weights 400-800) for headings,
  buttons, brand, overlines, badges.
- Body: **Nunito** (`--ff-font-body`, weights 200-1000) for body, labels,
  messages. Body default is 14px / line-height 1.65 at weight 600-700.
- Scale (`--ff-text-*`): xs 11, sm 12, base 14, md 15 (chat messages), lg 18
  (section headings), xl 24 (page titles), 2xl 32 (hero/empty states).
- **Sentence case everywhere** (buttons "Save changes", labels, headings).
  UPPERCASE is reserved for tiny overlines (`.ff-overline`) and badges
  (`.ff-badge`, mode badges, the "ADULT" badge) with `--ff-tracking-caps`.

## Shape

Very round. Radii (`--ff-radius-*`): pill 999 (buttons/inputs/selects/chips, the
default control shape), card 24 (cards/panels), bubble 22 (chat bubbles, with one
8px tail corner toward the speaker), md 16 (list items), sm 9 (checkboxes/tiny
chips). Controls are 48px tall (`--ff-control-h`); compact icon buttons 38px
(`--ff-control-h-sm`); never shrink a hit target below 44px (`--ff-tap-min`).

## Glow, shadow, focus, motion

- **Glows are the firefly signature.** Primary buttons, the send button, checked
  radios/checkboxes, and focused fields emit light (`--ff-glow-accent*`). The
  amber spark glows warm (`--ff-glow-firefly`); online dots glow green
  (`--ff-glow-online`).
- **Shadows** for elevation: `--ff-shadow-card`, `--ff-shadow-pop` (soft, large,
  black). AI bubbles use a 1px violet hairline ring instead of a shadow.
- **Focus** = `--ff-ring-focus` (4px translucent violet ring + glow). Keep it on
  interactive elements; do not remove focus styling.
- **Motion**: 150ms (`--ff-dur-fast`) ease for hovers/focus. Playful elements
  (primary/send buttons) may use `--ff-ease-bounce`, lift 1px on hover, and press
  to scale 0.98 (`scale(0.95-0.98)`). Glows intensify on hover; colors do not
  change dramatically. Hover: brightness +~12% on filled controls; surface
  step-up on list items.

## Component classes (`components.css`)

Use these in markup rather than re-implementing:

- Buttons: `.ff-btn`, `.ff-btn--primary` (gradient + glow), `.ff-btn--ghost`,
  `.ff-btn--danger`, `.ff-btn--sm`, `.ff-btn--icon`.
- Fields: `.ff-field`, `.ff-label`, `.ff-help`, `.ff-input`, `.ff-select`,
  `.ff-check`, `.ff-radios` / `.ff-radio` (large vertical radios).
- Chrome: `.ff-chip`, `.ff-dot-online`, `.ff-badge`, `.ff-badge--warm`,
  `.ff-spark`, `.ff-card`, `.ff-avatar`, `.ff-conv` (+ `.is-active`).
- Chat: `.ff-bubble-user`, `.ff-bubble-ai`.

## How the app maps to the system

- **`src/routes/+layout.svelte`** imports `firefly.css` (the only place the
  system is loaded).
- **`src/lib/ConversationList.svelte`** is the `.side` sidebar: gradient
  "New conversation" button, `.ff-conv` items, and a `.side-foot` showing the
  active profile avatar (`.ff-avatar`) + sync microcopy.
- **`src/lib/Chat.svelte`** is the chat pane: faint radial bloom background,
  `.ff-bubble-user` (right) and AI rows with an amber-dot `.ai-avatar`, a mode
  `.ff-badge`, and `.ff-bubble-ai`. The composer is a `.pillbar` holding the task
  `.model` select, the message input, and a glowing circular `.send` button.
  A degraded reply uses `.ff-badge--warm`.
- **`src/routes/+page.svelte`** owns the shell, header (brand spark, online +
  sync `.ff-chip`s, profile pill + `.ff-badge`, `.icon-btn` sync/settings), the
  settings overlay (centered 620px stack of `.ff-card` sections with `.ff-field`
  controls), and the onboarding `.ff-card`.

## Iconography

Minimal rounded-stroke SVG icons (2px stroke, round caps) drawn inline to match
the soft shape language; sized 15-16px inside 38px round `.icon-btn`s, colored
`--ff-text-muted`. The recommended set is [Lucide](https://lucide.dev) (2px
rounded strokes). Status is shown with glowing dots, not icons. No icon fonts, no
emoji. The brand mark is the `.ff-spark` (amber dot + warm glow) left of the
"Firefly" wordmark in Baloo 2.

## Microcopy

Warm, helpful, lightly playful; first person, addresses the user as "you", and
offers a next step at the end of answers. Examples: "Message Firefly…",
"＋ New conversation", "Synced · just now", "Stored locally on this device."

## Fonts: offline bundling

Baloo 2 + Nunito are bundled locally as variable woff2 (latin + latin-ext) in
`static/fonts/`, wired through `@font-face` in `src/lib/styles/fonts.css`. This
keeps the Tauri app fully offline. To refresh a font, re-download the latin and
latin-ext subsets from Google Fonts and replace the woff2 files (the `@font-face`
weight ranges already cover every weight the system uses). Do not reintroduce the
Google Fonts `@import` (it breaks offline builds).
