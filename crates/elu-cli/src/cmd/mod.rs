use crate::cli::{Cli, Command};
use crate::error::CliError;
use crate::global::GlobalCtx;
use crate::output::emit_error;

pub mod build;
pub mod check;
pub mod completion;
pub mod config;
pub mod explain;
pub mod fsck;
pub mod gc;
pub mod import;
pub mod init;
pub mod inspect;
pub mod ls;
pub mod publish;
pub mod refs;
pub mod schema;
pub mod search;
pub mod stub;

pub fn dispatch(cli: Cli) -> Result<(), CliError> {
    let ctx = GlobalCtx::from_args(&cli.global);
    let result = match cli.command {
        Command::Install(a) => stub::run("install", "WKIW.wX0h", "resolver", &ctx, a),
        Command::Add(a) => stub::run("add", "WKIW.wX0h", "resolver", &ctx, a),
        Command::Remove(a) => stub::run("remove", "WKIW.wX0h", "resolver", &ctx, a),
        Command::Lock(a) => stub::run("lock", "WKIW.wX0h", "resolver", &ctx, a),
        Command::Update(a) => stub::run("update", "WKIW.wX0h", "resolver", &ctx, a),
        Command::Stack(a) => stub::run("stack", "WKIW.zRCQ", "stacker", &ctx, a),
        Command::Audit(a) => stub::run("audit", "WKIW.wX0h", "resolver", &ctx, a),
        Command::Policy(a) => stub::run("policy", "TBD", "policy crate", &ctx, a),
        Command::Init(a) => init::run(&ctx, a),
        Command::Build(a) => build::run(&ctx, a),
        Command::Check(a) => check::run(&ctx, a),
        Command::Explain(a) => explain::run(&ctx, a),
        Command::Schema(a) => schema::run(&ctx, a),
        Command::Publish(a) => publish::run(&ctx, a),
        Command::Import(a) => import::run(&ctx, a),
        Command::Search(a) => search::run(&ctx, a),
        Command::Inspect(a) => inspect::run(&ctx, a),
        Command::Ls(a) => ls::run(&ctx, a),
        Command::Gc(a) => gc::run(&ctx, a),
        Command::Fsck(a) => fsck::run(&ctx, a),
        Command::Refs(a) => refs::run(&ctx, a),
        Command::Config(a) => config::run(&ctx, a),
        Command::Completion(a) => completion::run(&ctx, a),
    };
    if let Err(e) = &result {
        emit_error(&ctx, e);
    }
    result
}
