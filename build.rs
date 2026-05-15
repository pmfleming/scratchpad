use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src/assets/Scratchpad.ico");

    if let Some(version) = locked_package_version("Cargo.lock", "eframe")
        .or_else(|| manifest_dependency_version("Cargo.toml", "eframe"))
    {
        println!("cargo:rustc-env=SCRATCHPAD_EFRAME_VERSION={version}");
    }

    embed_windows_app_icon();
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

fn embed_windows_app_icon() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let Some(rc_exe) = find_windows_resource_compiler() else {
        println!(
            "cargo:warning=Could not find rc.exe; scratchpad.exe will not have an embedded Windows icon resource."
        );
        return;
    };

    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        println!(
            "cargo:warning=CARGO_MANIFEST_DIR is unset; scratchpad.exe will not have an embedded Windows icon resource."
        );
        return;
    };
    let Ok(out_dir) = env::var("OUT_DIR") else {
        println!(
            "cargo:warning=OUT_DIR is unset; scratchpad.exe will not have an embedded Windows icon resource."
        );
        return;
    };

    let icon_path = Path::new(&manifest_dir).join("src/assets/Scratchpad.ico");
    let rc_path = Path::new(&out_dir).join("scratchpad_icon.rc");
    let res_path = Path::new(&out_dir).join("scratchpad_icon.res");
    let rc_contents = format!("1 ICON \"{}\"\n", icon_path.display());

    if let Err(error) = fs::write(&rc_path, rc_contents) {
        println!("cargo:warning=Could not write Windows icon resource script: {error}");
        return;
    }

    let output = Command::new(&rc_exe)
        .arg("/nologo")
        .arg(format!("/fo{}", res_path.display()))
        .arg(&rc_path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("cargo:rustc-link-arg-bin=scratchpad={}", res_path.display());
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!(
                "cargo:warning=Could not compile Windows icon resource with {}: {}",
                rc_exe.display(),
                stderr.trim()
            );
        }
        Err(error) => {
            println!(
                "cargo:warning=Could not run Windows resource compiler {}: {error}",
                rc_exe.display()
            );
        }
    }
}

fn find_windows_resource_compiler() -> Option<PathBuf> {
    if let Ok(path) = env::var("PATH") {
        for directory in env::split_paths(&path) {
            let candidate = directory.join("rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let kits_root = Path::new("C:/Program Files (x86)/Windows Kits/10/bin");
    let arch = match env::var("HOST").unwrap_or_default().as_str() {
        host if host.contains("aarch64") => "arm64",
        host if host.contains("i686") => "x86",
        _ => "x64",
    };

    fs::read_dir(kits_root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(arch).join("rc.exe"))
        .filter(|path| path.is_file())
        .max()
}
