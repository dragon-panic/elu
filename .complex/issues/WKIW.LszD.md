## Package manifest format

Defines what a package is. Two manifest types:

### package.toml (single package)
```toml
[package]
name = "claude-code"
version = "2.1.71"
hash = "sha256:abc123..."

[source]
type = "npm"                        # apt, npm, pip, url, local
ref = "@anthropic-ai/claude-code"   # ecosystem-specific identifier

[deps]
nodejs = ">=18"

[hooks]
post_unpack = "./hooks/setup.sh"
```

### elu.toml (stack definition — what to build)
```toml
[stack]
name = "claude-sandbox"

[deps]
ubuntu-base = "24.04"
dev-tools = "1.0"
claude-code = "2.1"

[output]
format = "tar"   # tar, dir, qcow2
```

### Acceptance
- Parse both manifest types with serde
- Validate required fields, version constraints
- Lock file: elu.lock records resolved hashes for reproducibility
