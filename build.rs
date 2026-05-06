use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=Cargo.toml");

    if let Some(version) = locked_package_version("Cargo.lock", "eframe")
        .or_else(|| manifest_dependency_version("Cargo.toml", "eframe"))
    {
        println!("cargo:rustc-env=SCRATCHPAD_EFRAME_VERSION={version}");
    }
}

fn locked_package_version(path: impl AsRef<Path>, package_name: &str) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let mut in_package = false;
    let mut name_matches = false;

    for line in contents.lines().map(str::trim) {
        if line == "[[package]]" {
            in_package = true;
            name_matches = false;
            continue;
        }

        if !in_package {
            continue;
        }

        if let Some(name) = quoted_value(line, "name") {
            name_matches = name == package_name;
            continue;
        }

        if name_matches && let Some(version) = quoted_value(line, "version") {
            return Some(version.to_owned());
        }
    }

    None
}

fn manifest_dependency_version(path: impl AsRef<Path>, dependency_name: &str) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;

    for line in contents.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix(dependency_name) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        return rest
            .trim()
            .trim_matches('"')
            .split('"')
            .next()
            .map(str::to_owned);
    }

    None
}

fn quoted_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?
        .trim()
        .strip_prefix('"')?
        .split('"')
        .next()
}
