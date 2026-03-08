## Seguro integration

Replace seguro's build-image.sh with elu-powered image builds.

### Design
- seguro profiles map to elu stacks
- `[profiles.browser]` in seguro config → `elu.toml` with browser deps
- `seguro images build --profile X` calls elu to build the image layers
- elu produces a qcow2 (or unpacks into a base image via chroot)
- Cached layers mean rebuilding a profile only reapplies changed layers

### Migration path
1. elu works standalone first
2. seguro gains optional elu integration (falls back to build-image.sh)
3. Eventually elu becomes the default image builder

### Acceptance
- `elu build --output qcow2` produces a bootable image seguro can use
- Profile changes only rebuild affected layers (not the whole image)
- seguro can call elu as a library (not just CLI)
