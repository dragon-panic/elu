## Vision

Every package manager (apt, npm, pip, cargo, brew, pacman) does the same thing:
content-addressed blobs, dependency resolution, layer unpacking, and shell hooks.
elu unifies this into one tool. A package is a tarball + manifest + optional hooks,
regardless of whether it came from Ubuntu's repos, npm, or PyPI.

## Core model

- **Package**: manifest.toml + tarball + optional hooks (pre/post unpack scripts)
- **Layer**: unpacked package on disk, identified by sha256 of its contents
- **Stack**: ordered list of layers → produces a filesystem
- **Store**: ~/.local/share/elu/store/ — content-addressed, deduplicated
- **Importer**: wraps existing ecosystem packages (apt, npm, pip) as elu packages

## Integration with seguro

seguro's VM profiles define what goes in an image. elu builds the image layers.
`elu build --output qcow2` replaces seguro's build-image.sh with something
reproducible and cacheable.
