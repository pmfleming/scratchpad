//! Repository maintenance tasks, independent of the application build.
use std::{env, fs, path::Path, process::ExitCode};

use serde::Deserialize;

// Engine upgrades require matching app/probe locks and contract review.
const EXPECTED: &[(&str, &str)] = &[
    ("turso", "0.8.0"),
    ("turso_core", "0.8.3"),
    ("turso_ext", "0.8.3"),
    ("turso_macros", "0.8.3"),
    ("turso_parser", "0.8.3"),
    ("turso_sdk_kit", "0.8.3"),
    ("turso_sdk_kit_macros", "0.8.3"),
    ("turso_sync_engine", "0.8.3"),
    ("turso_sync_sdk_kit", "0.8.3"),
];

#[derive(Deserialize)]
struct Lockfile {
    package: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    version: String,
    source: String,
    checksum: String,
}

fn engine_identity(contents: &str, path: &Path) -> Result<Vec<Identity>, String> {
    let lock: Lockfile =
        toml::from_str(contents).map_err(|error| format!("{}: {error}", path.display()))?;
    EXPECTED
        .iter()
        .map(|&(name, version)| {
            let mut matches = lock.package.iter().filter(|package| package.name == name);
            let package = match (matches.next(), matches.next()) {
                (Some(package), None) if package.version == version => package,
                _ => {
                    return Err(format!(
                        "{}: expected exactly {name} {version}; review the engine contract before upgrading",
                        path.display()
                    ));
                }
            };
            match (&package.source, &package.checksum) {
                (Some(source), Some(checksum))
                    if source.starts_with("registry+") && !checksum.is_empty() =>
                {
                    Ok(Identity {
                        version: package.version.clone(),
                        source: source.clone(),
                        checksum: checksum.clone(),
                    })
                }
                _ => Err(format!(
                    "{}: {name} must identify a checksummed registry artifact",
                    path.display()
                )),
            }
        })
        .collect()
}

fn matching_identities(app: &[Identity], probe: &[Identity]) -> Result<(), String> {
    if app != probe {
        return Err("Application and contract probe engine sources/checksums differ".into());
    }
    Ok(())
}

fn check_turso_locks(root: &Path) -> Result<(), String> {
    let read_identity = |relative: &str| {
        let path = root.join(relative);
        let contents =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        engine_identity(&contents, &path)
    };
    let app = read_identity("Cargo.lock")?;
    let probe = read_identity("tools/turso-contract/Cargo.lock")?;
    matching_identities(&app, &probe)?;
    println!("Turso bindings 0.8.0, engine/SDK 0.8.3: matching lockfile identities");
    Ok(())
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (Some(command), None) if command == "check-turso-locks" => {
            check_turso_locks(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").as_path())
        }
        _ => Err("Usage: cargo xtask check-turso-locks".into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        let mut contents = "version = 4\n".to_owned();
        for (name, version) in EXPECTED {
            contents.push_str(&format!(
                "\n[[package]]\nname = {name:?}\nversion = {version:?}\n\
                 source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
                 checksum = \"test-checksum-{name}\"\n"
            ));
        }
        contents
    }

    fn identity(contents: &str) -> Result<Vec<Identity>, String> {
        engine_identity(contents, Path::new("fixture/Cargo.lock"))
    }

    #[test]
    fn accepts_matching_locks_and_ignores_unrelated_packages() {
        let app = identity(&fixture()).unwrap();
        let probe = identity(&format!(
            "{}\n[[package]]\nname = \"unrelated\"\nversion = \"1.0.0\"\n",
            fixture()
        ))
        .unwrap();
        assert_eq!(app.len(), EXPECTED.len());
        assert!(matching_identities(&app, &probe).is_ok());
    }

    #[test]
    fn package_order_does_not_matter() {
        let input = fixture();
        let packages: Vec<_> = input.split("\n[[package]]").skip(1).collect();
        let reordered = packages
            .into_iter()
            .rev()
            .map(|package| format!("\n[[package]]{package}"))
            .collect::<String>();
        assert_eq!(identity(&input).unwrap(), identity(&reordered).unwrap());
    }

    #[test]
    fn rejects_missing_or_upgraded_packages() {
        for (name, version) in EXPECTED {
            let missing = fixture().replace(&format!("name = {name:?}"), "name = \"other\"");
            let upgraded = fixture().replace(
                &format!("name = {name:?}\nversion = {version:?}"),
                &format!("name = {name:?}\nversion = \"99.0.0\""),
            );
            for input in [missing, upgraded] {
                let error = identity(&input).unwrap_err();
                assert!(error.contains(&format!("expected exactly {name} {version}")));
                assert!(error.contains("fixture/Cargo.lock"));
            }
        }
    }

    #[test]
    fn rejects_duplicate_packages_even_with_different_versions() {
        for (name, version) in EXPECTED {
            for duplicate_version in [*version, "99.0.0"] {
                let input = format!(
                    "{}\n[[package]]\nname = {name:?}\nversion = {duplicate_version:?}\n",
                    fixture()
                );
                assert!(identity(&input).unwrap_err().contains("expected exactly"));
            }
        }
    }

    #[test]
    fn requires_registry_sources_and_nonempty_checksums() {
        let source = "source = \"registry+https://github.com/rust-lang/crates.io-index\"";
        let checksum = "checksum = \"test-checksum-turso\"";
        for input in [
            fixture().replace(source, ""),
            fixture().replace(source, "source = \"\""),
            fixture().replace(source, "source = \"git+https://example.com/turso\""),
            fixture().replace(checksum, ""),
            fixture().replace(checksum, "checksum = \"\""),
        ] {
            assert!(
                identity(&input)
                    .unwrap_err()
                    .contains("checksummed registry artifact")
            );
        }
    }

    #[test]
    fn rejects_different_sources_or_checksums_between_locks() {
        let app = identity(&fixture()).unwrap();
        for input in [
            fixture().replace("test-checksum-turso", "different-checksum"),
            fixture().replace(
                "registry+https://github.com/rust-lang/crates.io-index",
                "registry+https://example.com/index",
            ),
        ] {
            let probe = identity(&input).unwrap();
            assert!(matching_identities(&app, &probe).is_err());
        }
    }

    #[test]
    fn rejects_malformed_lockfiles_with_path_context() {
        for input in ["not toml", "version = 4", "[[package]]\nname = 123"] {
            assert!(identity(input).unwrap_err().contains("fixture/Cargo.lock"));
        }
    }
}
