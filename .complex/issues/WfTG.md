## Goal

Three small follow-ups discovered while reviewing the registry round-trip arc (hnkX). Two are test-debt cleanups; one is a real production gap (LocalBlobBackend doesn't persist bytes — purely a stub today).

## What's in scope

1. **LocalBlobBackend serves blob bytes** — the production type currently just tracks an "uploaded" `HashSet<String>` and generates URLs that point at nothing. Make it a real backend: persist bytes, serve PUT and GET via an axum router, suitable for dev/CI/self-host. Once this lands, the round-trip test can drop its inline `InMemoryBlobBackend` and use the real backend.

2. **Retrofit `client_publish.rs` to build via `elu_author::build`** instead of hand-seeding TOML. Closes the regression-finding gap that hid the slice-1 manifest-format bug — once the test goes through the real build path, future "wire format mismatches stored format" bugs surface immediately.

3. **Remove TOML fallback from publish parsers** in `elu-registry/src/{client,server}/publish.rs`. The `parse_manifest_bytes` helper currently tries JSON, falls back to TOML, purely so test (1) keeps passing. Once (2) lands the TOML branch becomes dead code; strip both helpers down to a one-line JSON parse.

## Slice breakdown

- **Slice 1 — LocalBlobBackend serves blob bytes**. Independent. Big-ish — touches one production file plus an axum mount path, plus a test simplification.
- **Slice 2 — client_publish test goes through elu_author::build**. Independent of slice 1.
- **Slice 3 — Strip TOML fallback from publish parsers**. Blocked by slice 2.

## Out of scope (already documented elsewhere)

- `add`/`remove`/`lock` CLI dispatch
- Multi-ref / transitive registry resolution in `install`
- Publish signatures (PRD optional)

## Recommendation

Probably ~30 min total once decisions are made. Slice 1 is the user-facing one (real local blob storage works). Slices 2+3 together remove ~20 lines of test-driven compat from production code.
