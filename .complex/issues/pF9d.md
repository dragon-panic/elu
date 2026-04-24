## Scope (vertical slice)

Add `tests/common/mod.rs` helper and `tests/e2e_happy_path.rs` with a single test that threads `init → build → inspect → stack (dir)` through the real CLI.

## Files

- `crates/elu-cli/tests/common/mod.rs` (new)
- `crates/elu-cli/tests/e2e_happy_path.rs` (new)

## Helper API (sketch, not final)

```rust
pub struct Env { project: TempDir, store: TempDir }
impl Env {
    pub fn new() -> Self;
    pub fn project_path(&self) -> &Utf8Path;
    pub fn store_path(&self) -> &Utf8Path;
    pub fn elu(&self, args: &[&str]) -> assert_cmd::assert::Assert;
    pub fn elu_in_project(&self, args: &[&str]) -> assert_cmd::assert::Assert;
    pub fn elu_json_done(&self, args: &[&str]) -> serde_json::Value;
}
pub fn tiny_fixture(env: &Env);  // writes elu.toml + one layer file
```

## Test assertions

1. `init --kind native --name demo --namespace ns --path <project>` succeeds; `elu.toml` exists with expected fields.
2. `build --json` emits a `done` event with a non-empty `manifest_hash`.
3. `inspect --json ns/demo@0.1.0` returns a manifest that round-trips the package identity.
4. `stack ns/demo@0.1.0 -o <out>` materializes the layer file at `<out>/hello.txt` with expected contents.

## Red/green

- Red: write the test with `todo!()`/stub helper bodies so it compiles but fails on the first assertion. Commit `red — …`.
- Green: implement helper. Any pipeline-level bug discovered gets its own cx task, not bundled here. Commit `green — …`.

## Out of scope

- Determinism checks (slice 2)
- qcow2, registry, snapshots (future slices)
- Windows (`#[cfg(unix)]`)
