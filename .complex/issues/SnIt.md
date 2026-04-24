## Scope (vertical slice 3 of 3 — the acceptance of this whole arc)

Wire `elu install` end-to-end: parse refs → resolve → fetch-and-verify blobs → populate local store → stack to `--out`. The test for this slice is the round-trip that motivated the entire parent.

## Files

- `crates/elu-cli/src/cmd/install.rs` (new)
- `crates/elu-cli/src/cmd/mod.rs` (edit — rewire `Command::Install(a)` from `stub::run` to `install::run`)
- `crates/elu-cli/tests/roundtrip.rs` (new — the headline test)

## Install dispatch — target shape

```rust
// crates/elu-cli/src/cmd/install.rs
pub fn run(ctx: &GlobalCtx, args: InstallArgs) -> Result<(), CliError> {
    // 1. parse args.refs into PackageRef roots
    // 2. construct RegistryClient from ctx.registry (or offline-error)
    // 3. open store
    // 4. tokio rt; block_on resolver::resolve(roots, ...)
    // 5. for each FetchItem in resolution.fetch_plan:
    //      bytes = client.fetch_bytes(item.url?).await?
    //      verify::verify_manifest | verify_layer (by kind)
    //      store.write(bytes, expected_hash)
    // 6. stack the resolution to args.out (reuse stack::run's internal flow, or factor it)
}
```

If factoring out stack's guts proves intrusive, alternative is to call stack by reference after populating the store (less clean but shippable).

## Test — `tests/roundtrip.rs` (the original goal)

Uses `common::Env`. Extend `common/mod.rs` if needed with helpers for:
- Spinning up an in-process axum registry against `LocalBlobBackend` + in-memory DB, returning its URL
- A `publish_fixture(env, registry_url)` that does the tiny_fixture + build + publish dance

Test body:
```rust
#[test]
fn publish_then_install_reproduces_original() {
    let registry = spawn_test_registry();     // returns Url + handle
    let pub_env = Env::new();
    publish_fixture(&pub_env, &registry.url); // build + publish

    // Independent consumer with a fresh store:
    let sub_env = Env::new();
    let out = sub_env.project_path().join("installed");
    sub_env.elu(&[
        "--registry", registry.url.as_str(),
        "install", "ns/demo@0.1.0",
        "-o", out.to_str().unwrap(),
    ]).assert().success();

    assert_eq!(fs::read_to_string(out.join("hello.txt")).unwrap(), "hi");
}
```

## Red/green

- **Red:** write `install.rs` with `todo!()`, rewire `mod.rs`, write the round-trip test. Runs and fails at `todo!()`. Commit `red — …`.
- **Green:** implement fetch + verify + store writeback + stack. Commit `green — …`.

## Dependency

Blocked by 7u2u (CLI publish must work for the round-trip test to set up its state).

## Acceptance

- `elu install <ref> --registry <url> -o <dir>` produces the materialized tree
- `tests/roundtrip.rs::publish_then_install_reproduces_original` passes
- No regressions elsewhere in the suite; clippy clean with `-D warnings`

## Scope cap

Do NOT implement `add`, `remove`, `lock` here. They remain stubbed. Separate cx tasks for those.
