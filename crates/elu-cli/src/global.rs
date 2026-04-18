use camino::{Utf8Path, Utf8PathBuf};
use elu_store::fs_store::FsStore;

use crate::cli::{GlobalArgs, HookMode};
use crate::error::CliError;

#[derive(Debug, Clone)]
#[allow(dead_code)] // locked/hooks/verbose/quiet wired by future verbs (resolver+stacker+UX)
pub struct GlobalCtx {
    pub store_path: Utf8PathBuf,
    pub registry: Option<String>,
    pub offline: bool,
    pub locked: bool,
    pub hooks: Option<HookMode>,
    pub json: bool,
    pub verbose: u8,
    pub quiet: bool,
}

impl GlobalCtx {
    pub fn from_args(args: &GlobalArgs) -> Self {
        let store_path = args.store.clone().unwrap_or_else(default_store_path);
        Self {
            store_path,
            registry: args.registry.clone(),
            offline: args.offline,
            locked: args.locked,
            hooks: args.hooks,
            json: args.json,
            verbose: args.verbose,
            quiet: args.quiet,
        }
    }

    pub fn open_store(&self) -> Result<FsStore, CliError> {
        // FsStore::init is idempotent (create_dir_all on each subdir), so we always
        // call it. Lazy-init means a fresh --store path Just Works.
        FsStore::init(self.store_path.clone()).map_err(CliError::from)
    }

    pub fn store_path(&self) -> &Utf8Path {
        &self.store_path
    }
}

fn default_store_path() -> Utf8PathBuf {
    if let Some(dir) = dirs::data_dir() {
        let p = dir.join("elu");
        if let Some(s) = p.to_str() {
            return Utf8PathBuf::from(s);
        }
    }
    Utf8PathBuf::from(".elu-store")
}

pub fn config_dir() -> Utf8PathBuf {
    if let Some(dir) = dirs::config_dir() {
        let p = dir.join("elu");
        if let Some(s) = p.to_str() {
            return Utf8PathBuf::from(s);
        }
    }
    Utf8PathBuf::from(".elu-config")
}

pub const DEFAULT_REGISTRY: &str = "https://registry.elu.dev";
