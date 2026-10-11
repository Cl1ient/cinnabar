use std::{
    path::Path,
    process::{Command, Stdio},
};

use crate::{
    CommandSpec, DevtoolError, MACOS_MARKERS, Package, Selection, TestRunner, extra_commands,
    go_modules, packages_from_metadata, plan_ci, select_extra_checks, select_packages,
    selection::normalize, verification_commands,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subcommand {
    /// Runs the checks a change needs.
    VerifyAffected,
    /// Prints the pull-request CI plan as `$GITHUB_OUTPUT` lines.
    CiPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub subcommand: Subcommand,
    pub base: String,
    pub dry_run: bool,
}

pub fn parse_args<I, S>(arguments: I) -> Result<Options, DevtoolError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut arguments = arguments.into_iter().map(Into::into);
    let subcommand = match arguments.next().as_deref() {
        Some("verify-affected") => Subcommand::VerifyAffected,
        Some("ci-plan") => Subcommand::CiPlan,
        _ => {
            return Err(DevtoolError::Usage(
                "expected `verify-affected` or `ci-plan` subcommand".into(),
            ));
        }
    };
    let mut base = None;
    let mut dry_run = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--base" => {
                let value = arguments.next().ok_or_else(|| {
                    DevtoolError::Usage("`--base` requires a Git reference".into())
                })?;
                if value.is_empty() {
                    return Err(DevtoolError::Usage(
                        "`--base` requires a nonempty Git reference".into(),
                    ));
                }
                base = Some(value);
            }
            "--dry-run" if subcommand == Subcommand::VerifyAffected => dry_run = true,
            unknown => {
                return Err(DevtoolError::Usage(format!("unknown argument `{unknown}`")));
            }
        }
    }
    Ok(Options {
        subcommand,
        base: base.ok_or_else(|| DevtoolError::Usage("missing required `--base`".into()))?,
        dry_run,
    })
}

pub fn run(options: &Options) -> Result<(), DevtoolError> {
    let metadata = capture(CommandSpec::cargo(&[
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--locked",
    ]))?;
    let packages = packages_from_metadata(&metadata)?;
    let changed = changed_paths(Path::new("."), &options.base)?;
    let changed_refs = changed.iter().map(String::as_str).collect::<Vec<_>>();
    match options.subcommand {
        Subcommand::VerifyAffected => verify_affected(options, &changed_refs, &packages),
        Subcommand::CiPlan => ci_plan(&options.base, &changed_refs, &packages),
    }
}

fn verify_affected(
    options: &Options,
    changed_refs: &[&str],
    packages: &[Package],
) -> Result<(), DevtoolError> {
    let go_mod_paths = go_manifests(Path::new("."))?;
    let go_work = std::fs::read_to_string("go.work").ok();
    let go_modules = go_modules(
        &go_mod_paths.iter().map(String::as_str).collect::<Vec<_>>(),
        go_work.as_deref(),
    );
    let selection = select_packages(changed_refs, packages);
    match &selection {
        Selection::Workspace => println!("affected: workspace"),
        Selection::Packages(packages) => println!("affected: {}", packages.join(", ")),
        Selection::NoPackages => println!("affected: no Rust packages"),
    }
    let extra = select_extra_checks(changed_refs, &go_modules);
    if !extra.go_modules.is_empty() {
        let dirs: Vec<_> = extra
            .go_modules
            .iter()
            .map(|module| module.dir.as_str())
            .collect();
        println!("affected Go modules: {}", dirs.join(", "));
    }
    if extra.packaging {
        println!("affected: packaging tests");
    }
    let runner = detect_test_runner();
    match runner {
        TestRunner::Nextest => println!("test runner: cargo-nextest"),
        TestRunner::Cargo => {
            println!("test runner: cargo test (install cargo-nextest for faster local tests)");
        }
    }
    let mut commands = verification_commands(&selection, runner, packages);
    commands.extend(extra_commands(&extra));
    for command in commands {
        println!("$ {command}");
        if !options.dry_run {
            execute(command)?;
        }
    }
    Ok(())
}

/// Prints the plan on stdout and the macOS-specific files that drove it on stderr.
fn ci_plan(base: &str, changed_refs: &[&str], packages: &[Package]) -> Result<(), DevtoolError> {
    let mut marked = macos_marked(None, changed_refs)?;
    marked.extend(macos_marked(Some(base), changed_refs)?);
    marked.sort();
    marked.dedup();
    for path in &marked {
        eprintln!("mentions macOS-specific code: {path}");
    }
    let marked_refs = marked.iter().map(String::as_str).collect::<Vec<_>>();
    let plan = plan_ci(changed_refs, &marked_refs, packages);
    eprintln!("selection: {:?}", plan.selection);
    print!("{}", plan.outputs(packages));
    Ok(())
}

/// Returns the changed text files matching [`MACOS_MARKERS`] at `revision`, or in the working tree.
fn macos_marked(revision: Option<&str>, changed: &[&str]) -> Result<Vec<String>, DevtoolError> {
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    let display = format!("git grep -E '{MACOS_MARKERS}'");
    let output = Command::new("git")
        .args([
            "--literal-pathspecs",
            "grep",
            "-I",
            "-i",
            "-l",
            "-z",
            "-E",
            MACOS_MARKERS,
        ])
        .args(revision)
        .arg("--")
        .args(changed)
        .output()
        .map_err(|source| DevtoolError::Spawn {
            command: display.clone(),
            source,
        })?;
    // `git grep` exits with 1 when nothing matches.
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(DevtoolError::Command {
            command: display,
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
        });
    }
    let prefix = revision.map(|revision| format!("{revision}:"));
    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| {
            prefix
                .as_deref()
                .and_then(|prefix| path.strip_prefix(prefix))
                .unwrap_or(path)
                .to_owned()
        })
        .collect())
}

/// Returns paths under `root` that differ from `base`, plus non-ignored untracked files, sorted.
/// Renames count as both paths, so a move also selects the package it left.
fn changed_paths(root: &Path, base: &str) -> Result<Vec<String>, DevtoolError> {
    let root = root.to_string_lossy().into_owned();
    let mut changed = nul_paths(&capture(CommandSpec::new(
        "git",
        vec![
            "-C".into(),
            root.clone(),
            "diff".into(),
            "--name-only".into(),
            "--no-renames".into(),
            "-z".into(),
            base.into(),
            "--".into(),
        ],
    ))?);
    changed.extend(nul_paths(&capture(CommandSpec::new(
        "git",
        vec![
            "-C".into(),
            root,
            "ls-files".into(),
            "--others".into(),
            "--exclude-standard".into(),
            "-z".into(),
        ],
    ))?));
    changed.sort();
    changed.dedup();
    Ok(changed)
}

/// Returns tracked and non-ignored untracked `go.mod` paths under `root`, sorted.
fn go_manifests(root: &Path) -> Result<Vec<String>, DevtoolError> {
    let mut paths = nul_paths(&capture(CommandSpec::new(
        "git",
        vec![
            "-C".into(),
            root.to_string_lossy().into_owned(),
            "ls-files".into(),
            "-z".into(),
            "--cached".into(),
            "--others".into(),
            "--exclude-standard".into(),
            "--".into(),
            ":(glob)**/go.mod".into(),
        ],
    ))?);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn detect_test_runner() -> TestRunner {
    if Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        TestRunner::Nextest
    } else {
        TestRunner::Cargo
    }
}

fn nul_paths(output: &str) -> Vec<String> {
    output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(normalize)
        .collect()
}

fn capture(command: CommandSpec) -> Result<String, DevtoolError> {
    let display = command.to_string();
    let output = Command::new(&command.program)
        .args(&command.args)
        .envs(command.env.iter().map(|(key, value)| (key, value)))
        .output()
        .map_err(|source| DevtoolError::Spawn {
            command: display.clone(),
            source,
        })?;
    if !output.status.success() {
        return Err(DevtoolError::Command {
            command: display,
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
        });
    }
    String::from_utf8(output.stdout).map_err(|_| DevtoolError::NonUtf8 { command: display })
}

fn execute(command: CommandSpec) -> Result<(), DevtoolError> {
    let display = command.to_string();
    let status = Command::new(&command.program)
        .args(&command.args)
        .envs(command.env.iter().map(|(key, value)| (key, value)))
        .status()
        .map_err(|source| DevtoolError::Spawn {
            command: display.clone(),
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(DevtoolError::Command {
            command: display,
            status: status.to_string(),
            stderr: "see command output above".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::{Subcommand, changed_paths, go_manifests, parse_args};

    #[test]
    fn go_manifests_include_untracked_modules_but_not_ignored_ones() {
        let root =
            std::env::temp_dir().join(format!("devtool-go-manifests-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for dir in ["core", "tools/new", "ignored"] {
            fs::create_dir_all(root.join(dir)).unwrap();
            fs::write(root.join(dir).join("go.mod"), "module example\n").unwrap();
        }
        fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .current_dir(&root)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "--quiet"]);
        git(&["add", "core/go.mod"]);
        let manifests = go_manifests(&root);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(manifests.unwrap(), ["core/go.mod", "tools/new/go.mod"]);
    }

    #[test]
    fn moved_files_report_both_paths() {
        let root =
            std::env::temp_dir().join(format!("devtool-changed-paths-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("old")).unwrap();
        fs::write(root.join("old/lib.rs"), "pub fn moved() {}\n").unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .current_dir(&root)
                .args([
                    "-c",
                    "user.name=devtool",
                    "-c",
                    "user.email=devtool@example.invalid",
                ])
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "--quiet"]);
        git(&["add", "old/lib.rs"]);
        git(&["commit", "--quiet", "-m", "base"]);
        fs::create_dir_all(root.join("new")).unwrap();
        git(&["mv", "old/lib.rs", "new/lib.rs"]);
        let changed = changed_paths(&root, "HEAD");
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(changed.unwrap(), ["new/lib.rs", "old/lib.rs"]);
    }

    #[test]
    fn command_line_requires_an_explicit_base_and_supports_dry_run() {
        let options = parse_args(["verify-affected", "--base", "origin/main", "--dry-run"])
            .expect("parse options");
        assert_eq!(options.subcommand, Subcommand::VerifyAffected);
        assert_eq!(options.base, "origin/main");
        assert!(options.dry_run);

        assert!(parse_args(["verify-affected", "--dry-run"]).is_err());
        assert!(parse_args(["unknown", "--base", "HEAD"]).is_err());
    }

    #[test]
    fn ci_plan_takes_a_base_but_never_runs_checks() {
        let options = parse_args(["ci-plan", "--base", "HEAD^1"]).expect("parse options");
        assert_eq!(options.subcommand, Subcommand::CiPlan);
        assert_eq!(options.base, "HEAD^1");
        assert!(parse_args(["ci-plan", "--base", "HEAD^1", "--dry-run"]).is_err());
    }
}
