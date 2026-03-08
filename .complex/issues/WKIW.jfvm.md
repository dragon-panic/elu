## CLI (clap)

### Commands
```
elu init                        # create elu.toml in current dir
elu add <name> [--version X]    # add dep to elu.toml
elu remove <name>               # remove dep
elu lock                        # resolve deps → elu.lock
elu build [--output tar|dir|qcow2]  # build stack from lock
elu import <ecosystem> <pkg>    # apt|npm|pip → elu package
elu store ls                    # list store contents
elu store gc                    # garbage collect
elu publish <pkg>               # push to registry
elu info <name>                 # show package metadata
```

### Acceptance
- All commands wired up with clap derive
- Helpful error messages for missing manifests, unknown packages
- `--json` flag on list/info commands
