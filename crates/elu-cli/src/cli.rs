use camino::Utf8PathBuf;
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "elu",
    bin_name = "elu",
    version,
    about = "elu — package manager for the ring model",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, clap::Args)]
pub struct GlobalArgs {
    /// Override the store root. Default: $ELU_STORE or ~/.local/share/elu.
    #[arg(long, value_name = "PATH", env = "ELU_STORE", global = true)]
    pub store: Option<Utf8PathBuf>,

    /// Override the registry. Comma-separated for fallback chain.
    #[arg(long, value_name = "URL", env = "ELU_REGISTRY", global = true)]
    pub registry: Option<String>,

    /// Never contact a registry. Fail if resolution needs one.
    #[arg(long, global = true)]
    pub offline: bool,

    /// Refuse to proceed if the lockfile would need to change.
    #[arg(long, global = true)]
    pub locked: bool,

    /// Override hook policy: off, safe, ask, trust.
    #[arg(long, value_name = "MODE", global = true)]
    pub hooks: Option<HookMode>,

    /// Machine-readable output on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Verbose logging (-v / -vv).
    #[arg(short, long, action = ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Quiet: suppress progress output.
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum HookMode {
    Off,
    Safe,
    Ask,
    Trust,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Resolve and stack the referenced packages.
    Install(InstallArgs),
    /// Add a reference to the project manifest.
    Add(AddArgs),
    /// Remove a reference from the project manifest.
    Remove(RemoveArgs),
    /// Resolve the project manifest and write elu.lock.
    Lock(LockArgs),
    /// Re-resolve ignoring lockfile pins.
    Update(UpdateArgs),
    /// Resolve, fetch, and materialize at -o <path>.
    Stack(StackArgs),
    /// Scaffold a new elu.toml.
    Init(InitArgs),
    /// Build the local elu.toml into stored layers.
    Build(BuildArgs),
    /// Validate elu.toml without producing layers.
    Check(CheckArgs),
    /// Render a plain-English summary of a package.
    Explain(ExplainArgs),
    /// Emit the JSON Schema for elu.toml.
    Schema(SchemaArgs),
    /// Push a package from local store to registry.
    Publish(PublishArgs),
    /// Run an importer (apt, npm, pip).
    Import(ImportArgs),
    /// Query the registry's search index.
    Search(SearchArgs),
    /// Show a package's manifest, deps, layers, hook ops.
    Inspect(InspectArgs),
    /// Scan the lockfile for packages needing review.
    Audit(AuditArgs),
    /// Manage hook policy.
    Policy(PolicyArgs),
    /// List packages in the local store.
    Ls(LsArgs),
    /// Run garbage collection on the store.
    Gc(GcArgs),
    /// Re-hash every object in the store.
    Fsck(FsckArgs),
    /// Low-level ref operations.
    Refs(RefsArgs),
    /// Print or edit user configuration.
    Config(ConfigArgs),
    /// Generate a shell completion script.
    Completion(CompletionArgs),
}

#[derive(Debug, clap::Args)]
pub struct InstallArgs {
    /// Package references to install.
    pub refs: Vec<String>,
    /// Target output directory (default: ./elu-out).
    #[arg(short = 'o', long)]
    pub out: Option<Utf8PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct AddArgs {
    /// Package references to add.
    #[arg(required = true)]
    pub refs: Vec<String>,
}

#[derive(Debug, clap::Args)]
pub struct RemoveArgs {
    /// Package name to remove.
    #[arg(required = true)]
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct LockArgs {}

#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
    /// Optional names to update; empty = all.
    pub names: Vec<String>,
}

#[derive(Debug, clap::Args)]
pub struct StackArgs {
    /// Package references to stack.
    #[arg(required = true)]
    pub refs: Vec<String>,
    /// Output path.
    #[arg(short = 'o', long, required = true)]
    pub out: Utf8PathBuf,
    /// Override format (otherwise inferred from path).
    #[arg(long, value_enum)]
    pub format: Option<OutputFormat>,
    /// Base image for qcow2/raw outputs.
    #[arg(long)]
    pub base: Option<String>,
    /// dir: replace a pre-existing target.
    #[arg(long)]
    pub force: bool,
    /// dir: `uid:gid` to rewrite ownership after materializing.
    #[arg(long, value_name = "UID:GID")]
    pub owner: Option<String>,
    /// dir: octal mode mask applied to all entries (e.g. 755).
    #[arg(long, value_name = "OCTAL")]
    pub mode: Option<String>,
    /// tar: compression applied to the stream.
    #[arg(long, value_enum)]
    pub compress: Option<CompressionKind>,
    /// tar: compression level (format-specific).
    #[arg(long)]
    pub level: Option<i32>,
    /// tar: disable deterministic tar (keeps real mtime/uid/gid).
    #[arg(long = "no-deterministic", action = ArgAction::SetTrue)]
    pub no_deterministic: bool,
    /// qcow2: target disk size in bytes (default: fit + 20%).
    #[arg(long)]
    pub size: Option<u64>,
    /// qcow2: on-disk qcow2 version (default: 3).
    #[arg(long)]
    pub format_version: Option<u32>,
    /// qcow2: skip guest finalization (image may not boot).
    #[arg(long)]
    pub no_finalize: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum OutputFormat {
    Dir,
    Tar,
    Qcow2,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum CompressionKind {
    None,
    Gzip,
    Zstd,
    Xz,
}

#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Where to scaffold (default: current directory).
    #[arg(long, default_value = ".")]
    pub path: Utf8PathBuf,
    /// Builtin kind: native, ox-skill, ox-persona, ox-runtime.
    #[arg(long)]
    pub kind: Option<BuiltinKind>,
    /// Package name for the new project.
    #[arg(long)]
    pub name: Option<String>,
    /// Namespace.
    #[arg(long, default_value = "local")]
    pub namespace: String,
    /// Infer a starter from an existing project tree.
    #[arg(long)]
    pub from: Option<Utf8PathBuf>,
    /// Use a registry template (namespace/name[@version]).
    #[arg(long)]
    pub template: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum BuiltinKind {
    Native,
    OxSkill,
    OxPersona,
    OxRuntime,
}

#[derive(Debug, clap::Args)]
pub struct BuildArgs {
    /// Path to elu.toml (default: ./elu.toml in current directory).
    #[arg(long)]
    pub manifest: Option<Utf8PathBuf>,
    /// Validate only; do not produce layers.
    #[arg(long)]
    pub check: bool,
    /// Rebuild on file changes.
    #[arg(long)]
    pub watch: bool,
    /// Promote warnings to errors.
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
    /// Path to project root (default: current directory).
    #[arg(long, default_value = ".")]
    pub path: Utf8PathBuf,
    /// Promote warnings to errors.
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, clap::Args)]
pub struct ExplainArgs {
    /// Package reference (or hash) to explain.
    pub reference: Option<String>,
    /// Diff form: explain --diff <old> <new>.
    #[arg(long, num_args = 2, value_names = ["OLD", "NEW"])]
    pub diff: Option<Vec<String>>,
}

#[derive(Debug, clap::Args)]
pub struct SchemaArgs {
    /// Stored-form schema only.
    #[arg(long, conflicts_with_all = ["source", "yaml"])]
    pub stored: bool,
    /// Source-form schema only.
    #[arg(long, conflicts_with_all = ["stored", "yaml"])]
    pub source: bool,
    /// Emit YAML Schema (not yet implemented).
    #[arg(long)]
    pub yaml: bool,
}

#[derive(Debug, clap::Args)]
pub struct PublishArgs {
    /// Package reference to publish (namespace/name@version).
    #[arg(required = true)]
    pub reference: String,
    /// Bearer token used to authenticate the publish.
    #[arg(long, value_name = "TOKEN", env = "ELU_PUBLISH_TOKEN")]
    pub token: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct ImportArgs {
    /// Importer type: apt, npm, pip.
    #[arg(value_enum)]
    pub kind: ImportKind,
    /// Package names.
    #[arg(required = true)]
    pub names: Vec<String>,
    /// Transitively import dependencies.
    #[arg(long)]
    pub closure: bool,
    /// apt: distribution.
    #[arg(long)]
    pub dist: Option<String>,
    /// pip: platform tag.
    #[arg(long)]
    pub target: Option<String>,
    /// Specific version (single-name imports).
    #[arg(long)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum ImportKind {
    Apt,
    Npm,
    Pip,
}

#[derive(Debug, clap::Args)]
pub struct SearchArgs {
    /// Free-text query.
    pub query: Option<String>,
    /// Filter by kind.
    #[arg(long)]
    pub kind: Option<String>,
    /// Filter by tag.
    #[arg(long)]
    pub tag: Option<String>,
    /// Filter by namespace.
    #[arg(long)]
    pub namespace: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct InspectArgs {
    /// Package reference (or hash).
    #[arg(required = true)]
    pub reference: String,
}

#[derive(Debug, clap::Args)]
pub struct AuditArgs {
    /// Path to lockfile (default: ./elu.lock).
    #[arg(long, default_value = "elu.lock")]
    pub lockfile: Utf8PathBuf,
    /// Rules that, if matched, exit non-zero.
    #[arg(long, value_name = "RULE", action = ArgAction::Append)]
    pub fail_on: Vec<String>,
}

#[derive(Debug, clap::Args)]
pub struct PolicyArgs {
    /// Operate on per-project policy file instead of user file.
    #[arg(long, global = true)]
    pub project: bool,
    #[command(subcommand)]
    pub action: PolicyAction,
}

#[derive(Debug, Subcommand)]
pub enum PolicyAction {
    /// Show effective policy.
    Show,
    /// Report how policy would handle a package.
    Check { reference: String },
    /// Add an allow rule.
    Allow {
        #[arg(long)]
        publisher: Option<String>,
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        reads: Option<String>,
        #[arg(long)]
        writes: Option<String>,
        #[arg(long)]
        network: Option<bool>,
    },
    /// Add a deny rule.
    Deny {
        #[arg(long)]
        publisher: Option<String>,
    },
    /// Remove approval from lockfile.
    Revoke { reference: String },
    /// Set a policy field.
    Set { key: String, value: String },
}

#[derive(Debug, clap::Args)]
pub struct LsArgs {
    /// Optional namespace filter.
    pub namespace: Option<String>,
    /// Filter by kind.
    #[arg(long)]
    pub kind: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct GcArgs {
    /// Report what would be freed without removing anything.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, clap::Args)]
pub struct FsckArgs {
    /// Delete bad objects (they will be re-fetched).
    #[arg(long)]
    pub repair: bool,
}

#[derive(Debug, clap::Args)]
pub struct RefsArgs {
    #[command(subcommand)]
    pub action: RefsAction,
}

#[derive(Debug, Subcommand)]
pub enum RefsAction {
    /// List refs.
    Ls,
    /// Set a ref.
    Set {
        /// `<ns>/<name>/<version>`.
        spec: String,
        /// Manifest hash.
        hash: String,
    },
    /// Remove a ref.
    Rm {
        /// `<ns>/<name>/<version>`.
        spec: String,
    },
}

#[derive(Debug, clap::Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print current configuration.
    Show,
    /// Set a configuration value.
    Set { key: String, value: String },
}

#[derive(Debug, clap::Args)]
pub struct CompletionArgs {
    /// Shell to generate completions for.
    pub shell: Shell,
}

pub fn parse() -> Result<Cli, clap::Error> {
    Cli::try_parse()
}
