//! Offline apply process copied outside the installed app before the old client exits.
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use update_installer::startup::{self, Monitor, Progress};

/// Accepts only local inputs and reports a failed apply in the installer's log.
fn main() {
    if let Err(error) = run() {
        eprintln!("Update was not installed: {error:#}");
        std::process::exit(1);
    }
}

/// Waits for the exact parent process before verifying, applying, and launching the release.
fn run() -> Result<()> {
    let _copy = update_installer::helper::hold(&std::env::current_exe()?)?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg == update_installer::recovery::RECOVER_FLAG)
    {
        ensure!(
            args.len() == 4,
            "expected --recover UPDATE_DIR TARGET PARENT_PID"
        );
        let (root, target) = (PathBuf::from(&args[1]), PathBuf::from(&args[2]));
        let pid = args[3].to_str().context("invalid parent PID")?.parse()?;
        update_installer::parent::wait(pid)?;
        if update_installer::recovery::restore(&target, &root)? {
            launch(&target, &root)?;
        }
        return Ok(());
    }
    ensure!(
        args.len() == 5,
        "expected STAGE TARGET PARENT_PID CHANNEL SWITCH_NOW"
    );
    let (stage, target) = (PathBuf::from(&args[0]), PathBuf::from(&args[1]));
    let pid = args[2].to_str().context("invalid parent PID")?.parse()?;
    let channel =
        update_manifest::channel::Channel::parse(args[3].to_str().context("invalid channel")?)
            .context("unknown channel")?;
    let switch_now = args[4].to_str().context("invalid switch choice")?.parse()?;
    let platform = update_manifest::platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported platform")?;
    let keys = update_manifest::parse_keys(update_installer::TRUSTED_KEYS)?;
    update_installer::parent::wait(pid)?;
    let root = stage.parent().context("missing update directory")?;
    let mut started = false;
    let result = update_installer::apply::after_exit(
        update_installer::apply::Request {
            stage_dir: &stage,
            target: &target,
            keys: &keys,
            check: update_manifest::select::Check {
                channel: channel.name(),
                platform: &platform,
                current: env!("CARGO_PKG_VERSION"),
                current_build: update_manifest::build_number(),
                switch_now,
            },
            now: time::OffsetDateTime::now_utc(),
        },
        || Ok(()),
        |target| {
            let result = launch(target, root);
            started = result.is_ok();
            result
        },
    );
    if let Err(error) = result {
        if !started {
            update_installer::recovery::restore(&target, root)?;
            launch(&target, root).context("restart the retained installation")?;
        }
        return Err(error);
    }
    Ok(())
}

/// Confirms the new client reached a complete frame, restoring its predecessor on early failure.
/// Asset preparation heartbeats restart the stall timeout, so a long first-run download is not a failure.
fn launch(target: &Path, root: &Path) -> Result<()> {
    let directory = tempfile::Builder::new().prefix("start-").tempdir_in(root)?;
    #[cfg(target_os = "macos")]
    let executable = target.join("Contents/MacOS/cinnabar");
    #[cfg(windows)]
    let executable = target.join("cinnabar.exe");
    #[cfg(not(any(target_os = "macos", windows)))]
    let executable = target.to_owned();
    let mut command = Command::new(executable);
    command
        .env(startup::STARTUP_ENV, startup::marker(directory.path()))
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    #[cfg(target_os = "linux")]
    {
        command.arg("--appimage-extract-and-run");
        for name in ["APPIMAGE", "APPDIR", "OWD", "LD_LIBRARY_PATH", "LD_PRELOAD"] {
            command.env_remove(name);
        }
    }
    let mut client = startup::Tree::spawn(&mut command).context("launch updated client")?;
    let mut monitor = Monitor::new(Instant::now());
    loop {
        let failure = match monitor.poll(directory.path(), Instant::now()) {
            Progress::Started => return Ok(()),
            Progress::Stalled => Some("updated client did not complete startup".to_owned()),
            Progress::Waiting => client
                .try_wait()?
                .map(|status| format!("updated client exited before startup: {status}")),
        };
        if let Some(failure) = failure {
            // Rollback restores files only after the client and its setup process have exited.
            client.stop()?;
            anyhow::bail!(failure);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
