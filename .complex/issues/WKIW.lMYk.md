## Output formats (tar, dir, qcow2)

### tar (default)
- Produce a single .tar.zst of the stacked filesystem
- Portable, can be unpacked anywhere

### dir
- Write stacked filesystem to a target directory
- Useful for inspection, testing, chroot

### qcow2
- Take a base qcow2 image, boot it, unpack layers inside, shut down, compact
- Or: mount qcow2 via nbd, unpack directly, unmount
- This is the seguro integration path

### Acceptance
- `elu build --output tar` produces a valid tarball
- `elu build --output dir --target ./out` produces a directory
- `elu build --output qcow2 --base base.qcow2` produces a new image
