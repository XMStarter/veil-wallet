//! Writes licence texts into the files the app carries, or says whether
//! those files are still right.
//!
//!     veil-notices [--check]
//!     veil-notices [--check] --parts LIST FILE
//!
//! Without `--parts` it is about the Rust packages in Veil's core and the
//! file `app/assets/licences/core.txt`. It asks `cargo metadata` once for
//! each system the core is built for, because a package can be in the core
//! on one system and not on another, and writes the texts of them all.
//! Cargo has to have the packages' source on the machine; it fetches what
//! is missing unless it is offline.
//!
//! With `--parts` it is about things that are no Rust packages: LIST has a
//! line for each licence file, the name of the part it belongs to, a tab,
//! and the file. `wallet_lib/notices.sh` and `transports/notices.sh` make
//! such lists.
//!
//! With `--check` nothing is written; it ends with 1 if the file would
//! change.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use veil_notices::{notices, notices_of, packages_in, Package};

/// The systems an app with the core inside is built for.
const TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-pc-windows-gnu",
    "aarch64-linux-android",
];

/// The package the app's library is built from.
const CORE: &str = "veil_core";

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate lies in the workspace")
        .to_path_buf()
}

fn packages() -> Result<BTreeSet<Package>, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut all = BTreeSet::new();
    for target in TARGETS {
        let output = Command::new(&cargo)
            .args(["metadata", "--format-version", "1", "--locked"])
            .args(["--filter-platform", target])
            .arg("--manifest-path")
            .arg(workspace().join("Cargo.toml"))
            .output()
            .map_err(|error| format!("cargo could not be started: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata for {target} failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let metadata = String::from_utf8_lossy(&output.stdout);
        all.extend(packages_in(&metadata, CORE).map_err(|problem| problem.to_string())?);
    }
    Ok(all)
}

/// The parts a list names, each with its files, in the list's order.
fn parts(list: &Path) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
    let text =
        std::fs::read_to_string(list).map_err(|error| format!("{}: {error}", list.display()))?;
    let mut parts: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Some((name, file)) = line.split_once('\t') else {
            return Err(format!(
                "{} line {}: a name, a tab and a file are expected",
                list.display(),
                number + 1
            ));
        };
        match parts.iter_mut().find(|(known, _)| known == name) {
            Some((_, files)) => files.push(file.into()),
            None => parts.push((name.to_owned(), vec![file.into()])),
        }
    }
    if parts.is_empty() {
        return Err(format!("{} names nothing", list.display()));
    }
    Ok(parts)
}

/// What is to be in the file, with what it is about in words.
fn wanted(list: Option<&Path>) -> Result<(String, String), String> {
    match list {
        None => {
            let all = packages()?;
            let text = notices(&all).map_err(|problem| problem.to_string())?;
            Ok((format!("{} packages", all.len()), text))
        }
        Some(list) => {
            let parts = parts(list)?;
            let text = notices_of(&parts).map_err(|problem| problem.to_string())?;
            Ok((format!("{} parts", parts.len()), text))
        }
    }
}

fn main() -> ExitCode {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let check = arguments.first().is_some_and(|first| first == "--check");
    if check {
        arguments.remove(0);
    }
    let (list, file) = match arguments.as_slice() {
        [] => (None, workspace().join("../app/assets/licences/core.txt")),
        [flag, list, file] if flag == "--parts" => (Some(PathBuf::from(list)), PathBuf::from(file)),
        _ => {
            eprintln!("usage: veil-notices [--check] [--parts LIST FILE]");
            return ExitCode::from(2);
        }
    };

    let (what, text) = match wanted(list.as_deref()) {
        Ok(wanted) => wanted,
        Err(problem) => {
            eprintln!("error: {problem}");
            return ExitCode::from(2);
        }
    };

    if check {
        // Compared without regard to line ends: a checkout on Windows may
        // have changed them.
        let kept = std::fs::read_to_string(&file)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if kept == text {
            println!(
                "{}: the licence texts of {what} are as kept",
                file.display()
            );
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "error: {} is not what the licence texts of {what} are now.\n\
             Write it again, without --check, and commit it.",
            file.display()
        );
        return ExitCode::from(1);
    }
    if let Err(error) = std::fs::write(&file, &text) {
        eprintln!("error: {}: {error}", file.display());
        return ExitCode::from(2);
    }
    println!("{}: the licence texts of {what}", file.display());
    ExitCode::SUCCESS
}
