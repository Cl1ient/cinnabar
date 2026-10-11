use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use proc_macro2::{TokenStream, TokenTree};
use syn::{
    LitStr, Macro,
    visit::{self, Visit},
};

use crate::{
    ArchitectureError,
    paths::{cinnabar_list, crate_roots, owning_crate, relative_slash, within},
    policy::Policy,
    read,
    sources::wildcard_match,
};

/// Requires `[package.metadata.cinnabar] reads` to list every file a crate reads from another
/// crate, so CI selects the reader when that file changes.
///
/// A read is an `include_str!`/`include_bytes!` path, resolved from the source file, or a
/// literal starting with `../` or `/../`, resolved from the crate root as
/// `env!("CARGO_MANIFEST_DIR")` paths are. A bare repository-relative literal such as
/// `"crates/assets/data"` only keeps a declaration current: tests name such paths as data.
pub(super) fn check_reads(
    root: &Path,
    policy: &Policy,
    files: &[PathBuf],
    diagnostics: &mut Vec<String>,
) -> Result<(), ArchitectureError> {
    let roots = crate_roots(policy);
    let mut required = BTreeMap::<&str, BTreeMap<String, String>>::new();
    let mut mentioned = BTreeMap::<&str, BTreeSet<String>>::new();
    for path in files {
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = relative_slash(root, path);
        let Some((crate_root, name)) = owning_crate(&roots, &relative) else {
            continue;
        };
        // Unparseable sources are reported by the module checks.
        let Ok(file) = syn::parse_file(&read(path)?) else {
            continue;
        };
        let mut scan = Literals::default();
        scan.visit_file(&file);
        let directory = relative
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory);
        let foreign = |target: Option<String>| {
            target.filter(|target| {
                owning_crate(&roots, target).is_some_and(|(owner, _)| owner != crate_root)
            })
        };
        for literal in &scan.included {
            if let Some(target) = foreign(resolve(directory, literal)) {
                required
                    .entry(name)
                    .or_default()
                    .entry(target)
                    .or_insert_with(|| relative.clone());
            }
        }
        for literal in &scan.literals {
            let relative_to_crate = literal
                .strip_prefix('/')
                .unwrap_or(literal)
                .starts_with("../");
            if relative_to_crate {
                if let Some(target) = foreign(resolve(crate_root, literal.trim_start_matches('/')))
                {
                    required
                        .entry(name)
                        .or_default()
                        .entry(target)
                        .or_insert_with(|| relative.clone());
                }
            } else if let Some(target) = foreign(resolve("", literal)) {
                mentioned.entry(name).or_default().insert(target);
            }
        }
    }
    for (crate_root, name) in &roots {
        let manifest_path = root.join(crate_root).join("Cargo.toml");
        let manifest = toml::from_str::<toml::Value>(&read(&manifest_path)?).map_err(|source| {
            ArchitectureError::Policy {
                path: manifest_path.clone(),
                source,
            }
        })?;
        let declared = cinnabar_list(&manifest, "reads")
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>();
        let reads = required.remove(name).unwrap_or_default();
        for (target, source) in &reads {
            if !declared.iter().any(|pattern| covers(pattern, target)) {
                diagnostics.push(format!(
                    "{name}: {source} reads `{target}` from another crate; list it in `reads` under [package.metadata.cinnabar]"
                ));
            }
        }
        let mentions = mentioned.remove(name).unwrap_or_default();
        for pattern in declared {
            if !reads
                .keys()
                .chain(&mentions)
                .any(|target| covers(pattern, target))
            {
                diagnostics.push(format!(
                    "{name}: declared read `{pattern}` matches nothing its sources read"
                ));
            }
        }
    }
    Ok(())
}

/// Whether a declared pattern names `target`, a directory containing it, or a file inside it.
fn covers(pattern: &str, target: &str) -> bool {
    wildcard_match(pattern.as_bytes(), target.as_bytes())
        || within(target, pattern)
        || within(pattern, target)
}

/// Joins `path` to a repository-relative `base`, returning `None` when it leaves the repository.
fn resolve(base: &str, path: &str) -> Option<String> {
    let mut parts = base
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Collects string literals, keeping `include_str!`/`include_bytes!` paths apart.
#[derive(Default)]
struct Literals {
    included: Vec<String>,
    literals: Vec<String>,
}

impl Literals {
    /// Macro bodies are not parsed, so their string literals are read from the tokens.
    fn tokens(&mut self, tokens: TokenStream) {
        for tree in tokens {
            match tree {
                TokenTree::Group(group) => self.tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    if let Ok(literal) = syn::parse_str::<LitStr>(&literal.to_string()) {
                        self.literals.push(literal.value());
                    }
                }
                TokenTree::Ident(_) | TokenTree::Punct(_) => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Literals {
    fn visit_lit_str(&mut self, literal: &'ast LitStr) {
        self.literals.push(literal.value());
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        let include = mac.path.is_ident("include_str") || mac.path.is_ident("include_bytes");
        match mac.parse_body::<LitStr>() {
            Ok(path) if include => self.included.push(path.value()),
            _ => self.tokens(mac.tokens.clone()),
        }
        visit::visit_macro(self, mac);
    }
}
