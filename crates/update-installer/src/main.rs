//! Offline helper copied outside the installation before the client exits.
use anyhow::{Context, Result, ensure};
use std::path::PathBuf;

/// Runs only the apply operation; malformed arguments never enter package handling.
fn main() {
    if let Err(error) = run() {
        eprintln!("Update was not installed: {error:#}");
        std::process::exit(1);
    }
}

/// Resolves local inputs and waits for the client before revalidating the signed artifact.
fn run() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 3, "expected STAGE TARGET PARENT_PID");
    let stage = PathBuf::from(&args[0]);
    let target = PathBuf::from(&args[1]);
    let pid = args[2].to_str().context("invalid parent PID")?.parse()?;
    let platform = update_manifest::platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported platform")?;
    let keys = update_manifest::parse_keys(update_installer::TRUSTED_KEYS)?;
    update_installer::parent::wait(pid)?;
    update_installer::apply::after_exit(
        update_installer::apply::Request {
            stage_dir: &stage,
            target: &target,
            keys: &keys,
            platform_key: &platform,
            current: env!("CARGO_PKG_VERSION"),
            now: update_installer::OffsetDateTime::now_utc(),
        },
        || Ok(()),
    )
}
