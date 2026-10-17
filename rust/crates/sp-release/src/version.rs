//! The one version file a Rust release has: `[workspace.package] version`
//! in rust/Cargo.toml, which every crate inherits (the window's About line,
//! `method-reader --version`, the bundle's Info.plist and the package metadata
//! all come from it). release.yml refuses a tag it does not name, before
//! anything is built; the TypeScript era needed four files to agree.

/// The `version` of the `[workspace.package]` table and nothing else: the
/// table is cut at the next header first, so a dependency's version can
/// never be mistaken for it (build-app-bundle.ts's `cargoPackageVersion`
/// rule).
pub fn workspace_version(cargo_toml: &str) -> Option<String> {
    let mut in_table = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_table = t == "[workspace.package]";
            continue;
        }
        if in_table && let Some(rest) = t.strip_prefix("version") {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix('=') {
                let v = v.trim();
                if let Some(inner) = v.strip_prefix('"').and_then(|s| s.split('"').next()) {
                    return Some(inner.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_workspace_package_table_counts() {
        let toml = "[workspace]\nmembers = []\n\n[workspace.dependencies]\nfoo = { version = \"2\" }\n\n[workspace.package]\nedition = \"2024\"\nversion = \"0.12.0\"\n";
        assert_eq!(workspace_version(toml).as_deref(), Some("0.12.0"));
        assert_eq!(
            workspace_version("[workspace.package]\nedition = \"2024\"\n[x]\nversion = \"9\"\n"),
            None
        );
    }

    #[test]
    fn the_real_workspace_has_one() {
        let text =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
                .unwrap();
        assert_eq!(
            workspace_version(&text).as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }
}
