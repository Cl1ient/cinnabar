use crate::{
    Package, Selection,
    selection::{changed_packages, is_within, normalize, owner, select_packages},
};

/// Workflow definitions; a change to them can change what any job, macOS included, runs.
const CI_INPUTS: &[&str] = &[".github/workflows", ".github/actions"];

/// The workspace-wide architecture gate: it runs every test but needs no macOS runner.
const ARCHITECTURE_GATE: &str = "tools/architecture";

/// Crates whose tests exercise OS-specific behaviour: cfg(windows)/cfg(target_os) code, paths and
/// install layout, processes and IPC, GPU, input and windowing. Add a crate to opt it in.
pub const PLATFORM_TEST_PACKAGES: &[&str] = &[
    "asset-compiler",
    "assets",
    "bridge",
    "chunk-pipeline",
    "client-shell",
    "developer-control",
    "dist-local",
    "experience-runtime",
    "launcher",
    "launcher-host",
    "first-run",
    "mod-host",
    "pack-compiler",
    "protocol",
    "render",
    "rich-presence",
    "semantic-input",
    "server-experience",
];

/// Crates whose GPU witnesses only macOS runs in full on pull requests.
const MACOS_PACKAGES: &[&str] = &["render"];

/// The client's asset-carrier integration tests skip without local carriers, so platform jobs
/// run only its library tests.
const CLIENT_PACKAGE: &str = "bedrock-client";

/// Case-insensitive `git grep -E` pattern for code only macOS runners compile or exercise: macOS
/// and Apple cfgs, arm64, Objective-C bindings, and the GPU and windowing APIs Metal backs.
pub const MACOS_MARKERS: &str =
    r#"macos|darwin|aarch64|objc2|appkit|wgpu|winit|target_vendor *= *"apple""#;

/// Rust work a pull request needs, and whether it needs macOS runners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiPlan {
    pub selection: Selection,
    pub macos: bool,
}

/// Plans pull-request checks; `macos_marked` lists changed files that matched [`MACOS_MARKERS`]
/// before or after the change. Only package sources count: prose about macOS needs no runner.
#[must_use]
pub fn plan_ci(changed_paths: &[&str], macos_marked: &[&str], packages: &[Package]) -> CiPlan {
    let touches = |root: &str| {
        changed_paths
            .iter()
            .any(|path| is_within(&normalize(path), root))
    };
    let ci_input = CI_INPUTS.iter().any(|&root| touches(root));
    let selection = if ci_input || touches(ARCHITECTURE_GATE) {
        Selection::Workspace
    } else {
        select_packages(changed_paths, packages)
    };
    let macos = selection != Selection::NoPackages
        && (ci_input
            || macos_marked.iter().any(|path| {
                let path = normalize(path);
                !path.ends_with(".md") && owner(&path, packages).is_some()
            })
            || changed_paths.iter().any(|path| path.ends_with(".wgsl"))
            || changed_packages(changed_paths, packages).is_none_or(|owners| {
                owners
                    .iter()
                    .any(|name| MACOS_PACKAGES.contains(&name.as_str()))
            }));
    CiPlan { selection, macos }
}

impl CiPlan {
    /// Renders `key=value` lines for `$GITHUB_OUTPUT`. Tests are selected with nextest
    /// filtersets rather than `-p`, because Cargo unifies features only across the selected
    /// packages and a subset would miss the cache built for the whole workspace.
    #[must_use]
    pub fn outputs(&self, packages: &[Package]) -> String {
        let (scope, affected): (_, Vec<&str>) = match &self.selection {
            Selection::Workspace => (
                "workspace",
                packages
                    .iter()
                    .map(|package| package.name.as_str())
                    .collect(),
            ),
            Selection::Packages(names) => ("packages", names.iter().map(String::as_str).collect()),
            Selection::NoPackages => ("none", Vec::new()),
        };
        let tests = if self.selection == Selection::Workspace {
            "all()".into()
        } else {
            filterset(affected.iter().copied())
        };
        let mut platform = filterset(
            PLATFORM_TEST_PACKAGES
                .iter()
                .copied()
                .filter(|name| affected.contains(name)),
        );
        if affected.contains(&CLIENT_PACKAGE) {
            let client = format!("(package(={CLIENT_PACKAGE}) & kind(lib))");
            platform = if platform.is_empty() {
                client
            } else {
                format!("{platform} | {client}")
            };
        }
        format!(
            "scope={scope}\nmacos={}\nnames={}\ntests={tests}\nplatform={platform}\n",
            self.macos,
            affected.join(" "),
        )
    }
}

/// Joins nextest `package(=name)` matchers; empty when there are no names.
fn filterset<'a>(names: impl Iterator<Item = &'a str>) -> String {
    names
        .map(|name| format!("package(={name})"))
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::Path, process::Command};

    use super::{CLIENT_PACKAGE, MACOS_PACKAGES, PLATFORM_TEST_PACKAGES, plan_ci};
    use crate::{Package, Selection, packages_from_metadata};

    fn workspace() -> Vec<Package> {
        vec![
            Package::new("world", "crates/world", &[]),
            Package::new("render", "crates/render", &["world"]),
            Package::new("bedrock-client", "app", &["render"]),
            Package::new("devtool", "tools/devtool", &[]),
            Package::new("architecture", "tools/architecture", &[]),
        ]
    }

    #[test]
    fn workflow_changes_check_everything_on_every_platform() {
        for path in [
            ".github/workflows/ci.yml",
            ".github/actions/build-cache/cache.py",
        ] {
            let plan = plan_ci(&[path], &[], &workspace());
            assert_eq!(plan.selection, Selection::Workspace, "{path}");
            assert!(plan.macos, "{path}");
        }
    }

    #[test]
    fn architecture_gate_changes_check_everything_without_macos() {
        let plan = plan_ci(&["tools/architecture/policy.toml"], &[], &workspace());
        assert_eq!(plan.selection, Selection::Workspace);
        assert!(!plan.macos);
    }

    #[test]
    fn unclassified_and_workspace_inputs_need_macos() {
        for path in ["Cargo.lock", "unexpected/input.txt"] {
            let plan = plan_ci(&[path], &[], &workspace());
            assert_eq!(plan.selection, Selection::Workspace, "{path}");
            assert!(plan.macos, "{path}");
        }
    }

    #[test]
    fn macos_runs_only_for_macos_specific_changes() {
        let packages = workspace();
        let leaf = plan_ci(&["crates/world/src/lib.rs"], &[], &packages);
        assert_eq!(
            leaf.selection,
            Selection::Packages(vec![
                "bedrock-client".into(),
                "render".into(),
                "world".into()
            ])
        );
        assert!(!leaf.macos, "reverse dependencies alone do not need macOS");
        assert!(plan_ci(&["app/src/app.rs"], &["app/src/app.rs"], &packages).macos);
        assert!(plan_ci(&["crates/render/src/lib.rs"], &[], &packages).macos);
        assert!(plan_ci(&["crates/world/shaders/sky.wgsl"], &[], &packages).macos);
    }

    #[test]
    fn rust_free_changes_need_no_rust_jobs() {
        let changed = ["docs/guide.md", "packaging/macos/Info.plist.in"];
        let plan = plan_ci(&changed, &changed, &workspace());
        assert_eq!(plan.selection, Selection::NoPackages);
        assert!(!plan.macos);
        assert!(plan.outputs(&workspace()).contains("scope=none\n"));
    }

    #[test]
    fn prose_about_macos_needs_no_runner() {
        let changed = [
            "plan.md",
            "crates/world/README.md",
            "crates/world/src/lib.rs",
        ];
        let plan = plan_ci(&changed, &changed[..2], &workspace());
        assert!(matches!(plan.selection, Selection::Packages(_)));
        assert!(!plan.macos);
    }

    #[test]
    fn outputs_select_affected_tests_by_package() {
        let packages = workspace();
        let outputs = plan_ci(&["crates/render/src/lib.rs"], &[], &packages).outputs(&packages);
        assert_eq!(
            outputs,
            "scope=packages\nmacos=true\nnames=bedrock-client render\n\
             tests=package(=bedrock-client) | package(=render)\n\
             platform=package(=render) | (package(=bedrock-client) & kind(lib))\n"
        );
        let full = plan_ci(&["Cargo.toml"], &[], &packages).outputs(&packages);
        assert!(
            full.contains("\ntests=all()\nplatform=package(=render) | (package(=bedrock-client) & kind(lib))\n"),
            "{full}"
        );
        assert!(full.contains("\nnames=world render bedrock-client devtool architecture\n"));
        let leaf = plan_ci(&["tools/devtool/src/ci.rs"], &[], &packages).outputs(&packages);
        assert!(
            leaf.ends_with("\nmacos=false\nnames=devtool\ntests=package(=devtool)\nplatform=\n"),
            "{leaf}"
        );
    }

    /// Named packages must exist, or a workspace plan would silently drop them.
    #[test]
    fn named_packages_are_workspace_members() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new(env!("CARGO"))
            .current_dir(root)
            .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
            .output()
            .expect("run cargo metadata");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let packages = packages_from_metadata(&String::from_utf8(output.stdout).unwrap()).unwrap();
        let names: BTreeSet<_> = packages
            .iter()
            .map(|package| package.name.as_str())
            .collect();
        for name in PLATFORM_TEST_PACKAGES
            .iter()
            .chain(MACOS_PACKAGES)
            .chain([&CLIENT_PACKAGE])
        {
            assert!(names.contains(name), "{name} is not a workspace package");
        }
    }
}
