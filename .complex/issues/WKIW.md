# elu — universal content-addressed layer engine

Store file trees as hashed layers, stack them into materialized outputs
(dir, tar, qcow2), ship them through a lightweight registry. The
substrate underneath any system that needs reproducible, composable,
shareable bundles of files — agent skills, runtime images, system
package sets, or anything else expressible as "these files, in this
order, with this metadata."

## Design docs

Full PRD lives in [`docs/prd/`](../docs/prd/). Start with
[`docs/prd/README.md`](../docs/prd/README.md) for the mental model,
ring structure, and component index.

## Principles

- Content addressing is the only identity that matters; tags are sugar.
- `kind` is opaque to elu — consumers dispatch on it.
- One post-unpack hook per package, host-side, against the staging dir.
- Importers produce ordinary packages — no second format.
- Registry is a lookup service, not a blob host.
- No plugin system anywhere.

## Children

See child issues for individual components. Each child has a
corresponding `docs/prd/<name>.md` that is authoritative.
