//! Offline apply process copied outside the installed app before the old client exits.
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Accepts only local inputs and reports a failed apply in the installer's log.
fn main() {
    if let Err(error) = run() {
        eprintln!("Update was not installed: {error:#}");
        std::process::exit(1);
    }
}

/// Waits for the exact parent process before verifying, applying, and launching the release.
fn run() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
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
    update_installer::apply::after_exit(
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
        |target| launch(target, root),
    )
}

/// Confirms the new client reached a complete frame, restoring its predecessor on early failure.
fn launch(target: &Path, root: &Path) -> Result<()> {
    let directory = tempfile::Builder::new().prefix("start-").tempdir_in(root)?;
    let marker = directory.path().join("started");
    #[cfg(target_os = "macos")]
    let executable = target.join("Contents/MacOS/cinnabar");
    #[cfg(windows)]
    let executable = target.join("cinnabar.exe");
    #[cfg(not(any(target_os = "macos", windows)))]
    let executable = target.to_owned();
    let mut command = Command::new(executable);
    command
        .env(update_installer::STARTUP_ENV, &marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    #[cfg(target_os = "linux")]
    {
        command.arg("--appimage-extract-and-run");
        for name in ["APPIMAGE", "APPDIR", "OWD", "LD_LIBRARY_PATH", "LD_PRELOAD"] {
            command.env_remove(name);
        }
    }
    let mut child = command.spawn().context("launch updated client")?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if marker.is_file() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("updated client exited before startup: {status}");
        }
        if Instant::now() >= deadline {
            // Only this exact child was started by the updater. Wait before restoring its files.
            child.kill()?;
            child.wait()?;
            anyhow::bail!("updated client did not complete startup");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
