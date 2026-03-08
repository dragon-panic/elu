## Dependency resolver

Takes a stack's deps and produces an ordered list of concrete package hashes.

### Design
- Version constraints: semver ranges (>=1.0, ^2.1, =3.0.0)
- Resolution algorithm: PubGrub or similar SAT-based solver
- Input: dep map from elu.toml + available packages in store/registry
- Output: elu.lock — ordered list of (name, version, hash) tuples
- Conflict detection: clear error when two deps require incompatible versions

### Acceptance
- Resolves a simple diamond dependency (A→B, A→C, B→D, C→D)
- Detects and reports version conflicts
- Lock file is deterministic (same input = same lock)
- `elu lock` generates/updates elu.lock
