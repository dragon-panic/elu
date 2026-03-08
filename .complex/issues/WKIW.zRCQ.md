## Layer unpacking and stacking

Takes resolved packages from the store and produces a filesystem.

### Design
- Each package tarball is a layer
- Layers are applied in dependency order (bottom-up)
- Later layers overwrite earlier ones (like Docker)
- Hooks run in a chroot/namespace after the layer is unpacked
- Intermediate results can be cached (hash of layer stack prefix)

### Operations
- `unpack(hash, target_dir)` — extract tarball into target
- `stack(layers[], target_dir)` — apply layers in order
- `run_hooks(target_dir, hooks)` — execute post_unpack scripts in chroot

### Acceptance
- Two layers with overlapping files: later wins
- Hooks execute and can modify the unpacked filesystem
- Layer application is idempotent (same inputs = same output)
