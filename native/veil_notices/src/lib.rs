//! The licence texts of the packages Veil's core is built from.
//!
//! The core is one library made of some four hundred Rust packages, and
//! nearly every one asks that its licence text goes along with whatever
//! is built from it. This crate finds the packages that end up in the
//! core, by what `cargo metadata` says, reads the licence texts that lie
//! in each of them, and writes them into one file that the app carries
//! and shows (`app/assets/licences/core.txt`).
//!
//! What it writes holds entries one after the other, divided by a line of
//! eighty dashes. An entry is the packages it is about, one to a line with
//! their versions, then an empty line, then the text. A text that several
//! packages carry word for word is there once, with all of them named.
//! The app reads the file by the same rule (`app/lib/core/notices.dart`).
//!
//! It reads no licence and judges none. Whether the texts are the right
//! ones is the packages' own business; this only does not lose them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// What divides one entry from the next: a line of eighty dashes.
pub const DIVIDER: &str =
    "--------------------------------------------------------------------------------";

/// The licences this crate has a text of, for packages that name a licence
/// and carry no text.
const TEXTS_BY_NAME: &[(&str, &str)] = &[
    ("Apache-2.0", include_str!("../texts/Apache-2.0.txt")),
    ("MIT", include_str!("../texts/MIT.txt")),
];

/// A package whose code is in the core.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Package {
    pub name: String,
    pub version: String,
    /// The licence as the package names it, an SPDX expression as a rule.
    pub licence: String,
    pub authors: Vec<String>,
    pub repository: String,
    /// The folder the package's source lies in.
    pub folder: PathBuf,
}

impl Package {
    fn title(&self) -> String {
        format!("{} {}", self.name, self.version)
    }
}

#[derive(Debug)]
pub enum Problem {
    /// What `cargo metadata` printed is not what this expects.
    Metadata(String),
    /// A package names no licence and carries no text.
    NoLicence { package: String },
    /// A package carries no text and names a licence there is no text of
    /// here. A text has to be added to `texts/` and to `TEXTS_BY_NAME`.
    NoText { package: String, licence: String },
    File {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Metadata(what) => write!(out, "cargo metadata: {what}"),
            Problem::NoLicence { package } => {
                write!(out, "{package} names no licence and has no licence text")
            }
            Problem::NoText { package, licence } => write!(
                out,
                "{package} has no licence text of its own and names {licence}, \
                 of which veil_notices has no text"
            ),
            Problem::File { path, source } => write!(out, "{}: {source}", path.display()),
        }
    }
}

/// The packages whose code goes into `root`, from what `cargo metadata
/// --format-version 1` printed: `root` and everything it depends on to be
/// built from, and so on down. Left out are packages needed only to build
/// or to test another, packages that only write code while the compiler
/// runs (procedural macros), and the packages of this workspace, which are
/// Veil itself.
pub fn packages_in(metadata: &str, root: &str) -> Result<BTreeSet<Package>, Problem> {
    let wrong = |what: &str| Problem::Metadata(what.to_owned());
    let metadata: Value =
        serde_json::from_str(metadata).map_err(|error| Problem::Metadata(error.to_string()))?;

    let mut by_id = BTreeMap::new();
    for package in metadata["packages"]
        .as_array()
        .ok_or_else(|| wrong("no packages"))?
    {
        let id = package["id"]
            .as_str()
            .ok_or_else(|| wrong("a package without an id"))?;
        by_id.insert(id, package);
    }
    let mut depends_on: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in metadata["resolve"]["nodes"]
        .as_array()
        .ok_or_else(|| wrong("no resolved packages"))?
    {
        let id = node["id"]
            .as_str()
            .ok_or_else(|| wrong("a node without an id"))?;
        let mut needed = Vec::new();
        for dependency in node["deps"]
            .as_array()
            .ok_or_else(|| wrong("a node without deps"))?
        {
            let kinds = dependency["dep_kinds"]
                .as_array()
                .ok_or_else(|| wrong("a dependency without kinds"))?;
            // The kind is null for what a package is built from; "build"
            // and "dev" are for building and for testing it.
            if kinds.iter().any(|kind| kind["kind"].is_null()) {
                needed.push(
                    dependency["pkg"]
                        .as_str()
                        .ok_or_else(|| wrong("a dependency without a package"))?,
                );
            }
        }
        depends_on.insert(id, needed);
    }

    let start = by_id
        .iter()
        .find(|(_, package)| package["name"] == root && package["source"].is_null())
        .map(|(id, _)| *id)
        .ok_or_else(|| Problem::Metadata(format!("no package {root} in the workspace")))?;

    let mut seen = BTreeSet::new();
    let mut to_visit = vec![start];
    let mut found = BTreeSet::new();
    while let Some(id) = to_visit.pop() {
        if !seen.insert(id) {
            continue;
        }
        let package = by_id
            .get(id)
            .ok_or_else(|| wrong("a package that is not listed"))?;
        let writes_code_only = package["targets"].as_array().is_some_and(|targets| {
            targets.iter().any(|target| {
                target["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|kind| kind == "proc-macro"))
            })
        });
        if writes_code_only {
            // Neither it nor what it is built from is in the core.
            continue;
        }
        to_visit.extend(depends_on.get(id).into_iter().flatten().copied());
        if package["source"].is_null() {
            // A package of this workspace.
            continue;
        }
        let text = |field: &str| package[field].as_str().unwrap_or_default().to_owned();
        let manifest = package["manifest_path"]
            .as_str()
            .ok_or_else(|| wrong("a package without a manifest"))?;
        found.insert(Package {
            name: text("name"),
            version: text("version"),
            licence: text("license"),
            authors: package["authors"]
                .as_array()
                .map(|authors| {
                    authors
                        .iter()
                        .filter_map(|author| author.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            repository: text("repository"),
            folder: Path::new(manifest)
                .parent()
                .ok_or_else(|| wrong("a manifest without a folder"))?
                .to_path_buf(),
        });
    }
    Ok(found)
}

/// Whether a file of this name, at the top of a package, is a licence
/// text or a notice that has to go along with one.
fn is_licence_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "licence",
        "license",
        "copying",
        "notice",
        "unlicense",
        "copyright",
        "patents",
    ]
    .iter()
    .any(|start| name.starts_with(start))
}

/// A text as it is written into the notices: Unix line ends, nothing at
/// the end of a line, no empty lines around it, and no line that could be
/// taken for the divider.
fn tidy(text: &str) -> String {
    let lines: Vec<String> = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| {
            let line = line.trim_end();
            if line.len() >= DIVIDER.len() && line.bytes().all(|byte| byte == b'-') {
                // One dash short, so that it divides nothing.
                DIVIDER[1..].to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect();
    lines.join("\n").trim_matches('\n').to_owned()
}

/// The licence texts that lie at the top of a package's folder, by file
/// name.
fn texts_of(package: &Package) -> Result<Vec<String>, Problem> {
    let failed = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Problem::File { path, source }
    };
    let mut names = Vec::new();
    for entry in fs::read_dir(&package.folder).map_err(failed(&package.folder))? {
        let entry = entry.map_err(failed(&package.folder))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_licence_file(&name) && entry.path().is_file() {
            names.push(name);
        }
    }
    names.sort();
    let mut texts = Vec::new();
    for name in names {
        let path = package.folder.join(name);
        let bytes = fs::read(&path).map_err(failed(&path))?;
        let text = tidy(&String::from_utf8_lossy(&bytes));
        if !text.is_empty() {
            texts.push(text);
        }
    }
    Ok(texts)
}

/// The licences an expression names: `MIT OR Apache-2.0` names two.
fn named_in(expression: &str) -> Vec<&str> {
    expression
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+'))
        .filter(|word| !word.is_empty() && !["OR", "AND", "WITH"].contains(word))
        .collect()
}

/// The notices for these packages, as described at the top of this file.
pub fn notices(packages: &BTreeSet<Package>) -> Result<String, Problem> {
    // A text, and the packages that carry exactly it.
    let mut carried: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // A licence expression, and the packages that name it and carry no
    // text.
    let mut named_only: BTreeMap<String, Vec<&Package>> = BTreeMap::new();

    for package in packages {
        let texts = texts_of(package)?;
        if texts.is_empty() {
            if package.licence.is_empty() {
                return Err(Problem::NoLicence {
                    package: package.title(),
                });
            }
            named_only
                .entry(package.licence.clone())
                .or_default()
                .push(package);
        }
        for text in texts {
            carried.entry(text).or_default().insert(package.title());
        }
    }

    // Entries in the order of their first package, so that a change of
    // one package moves little.
    let mut entries: Vec<(Vec<String>, String)> = carried
        .into_iter()
        .map(|(text, titles)| (titles.into_iter().collect(), text))
        .collect();

    for (expression, packages) in named_only {
        let mut text = format!(
            "These packages come without a licence text of their own. Each names its\n\
             licence as: {expression}\n\n\
             Who wrote each, and where its source is:\n"
        );
        for package in &packages {
            let authors = if package.authors.is_empty() {
                "its authors".to_owned()
            } else {
                package.authors.join(", ")
            };
            text.push_str(&format!("\n{}: {authors}", package.title()));
            if !package.repository.is_empty() {
                text.push_str(&format!("; {}", package.repository));
            }
        }
        for name in named_in(&expression) {
            let Some((_, licence)) = TEXTS_BY_NAME.iter().find(|(known, _)| *known == name) else {
                return Err(Problem::NoText {
                    package: packages[0].title(),
                    licence: name.to_owned(),
                });
            };
            text.push_str(&format!("\n\nThe text of {name}:\n\n{}", tidy(licence)));
        }
        entries.push((
            packages.iter().map(|package| package.title()).collect(),
            text,
        ));
    }
    Ok(written(entries))
}

/// The notices for things that are no Rust packages: each a name and the
/// files that hold its licence texts. Texts that several of them carry
/// word for word are there once.
pub fn notices_of(parts: &[(String, Vec<PathBuf>)]) -> Result<String, Problem> {
    let mut carried: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, files) in parts {
        let mut any = false;
        for path in files {
            let bytes = fs::read(path).map_err(|source| Problem::File {
                path: path.clone(),
                source,
            })?;
            let text = tidy(&String::from_utf8_lossy(&bytes));
            if !text.is_empty() {
                carried.entry(text).or_default().insert(name.clone());
                any = true;
            }
        }
        if !any {
            return Err(Problem::NoLicence {
                package: name.clone(),
            });
        }
    }
    Ok(written(
        carried
            .into_iter()
            .map(|(text, names)| (names.into_iter().collect(), text))
            .collect(),
    ))
}

/// Entries as one text, in the order of the names they begin with.
fn written(mut entries: Vec<(Vec<String>, String)>) -> String {
    entries.sort();
    let mut out = String::new();
    for (number, (titles, text)) in entries.iter().enumerate() {
        if number > 0 {
            out.push_str(DIVIDER);
            out.push('\n');
        }
        for title in titles {
            out.push_str(title);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(text);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("veil-notices-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn package(root: &Path, name: &str, licence: &str, files: &[(&str, &str)]) -> Package {
        let folder = root.join(name);
        fs::create_dir_all(&folder).unwrap();
        for (file, text) in files {
            fs::write(folder.join(file), text).unwrap();
        }
        Package {
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            licence: licence.to_owned(),
            authors: vec![format!("the author of {name}")],
            repository: format!("https://example.invalid/{name}"),
            folder,
        }
    }

    /// What `cargo metadata` prints, cut down to what is read of it: the
    /// core is built from `plain`, which is built from `deep`; it is
    /// tested with `tester`, built with `builder`, and uses `macros`,
    /// which writes code and is itself built from `inside-macros`.
    const METADATA: &str = r#"{
      "packages": [
        {"id": "core", "name": "veil_core", "version": "0.1.0", "source": null,
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/w/veil_core/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        {"id": "tor", "name": "veil_tor", "version": "0.1.0", "source": null,
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/w/veil_tor/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        {"id": "plain", "name": "plain", "version": "1.2.3", "source": "registry",
         "license": "MIT", "authors": ["A"], "repository": "https://example.invalid/plain",
         "manifest_path": "/r/plain-1.2.3/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        {"id": "deep", "name": "deep", "version": "0.4.0", "source": "registry",
         "license": "Apache-2.0", "authors": [], "repository": null,
         "manifest_path": "/r/deep-0.4.0/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        {"id": "tester", "name": "tester", "version": "1.0.0", "source": "registry",
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/r/tester-1.0.0/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        {"id": "builder", "name": "builder", "version": "1.0.0", "source": "registry",
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/r/builder-1.0.0/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        {"id": "macros", "name": "macros", "version": "1.0.0", "source": "registry",
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/r/macros-1.0.0/Cargo.toml", "targets": [{"kind": ["proc-macro"]}]},
        {"id": "inside", "name": "inside-macros", "version": "1.0.0", "source": "registry",
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/r/inside-1.0.0/Cargo.toml", "targets": [{"kind": ["lib"]}]},
        {"id": "other", "name": "veil_cli", "version": "0.1.0", "source": null,
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/w/veil_cli/Cargo.toml", "targets": [{"kind": ["bin"]}]},
        {"id": "cli-only", "name": "cli-only", "version": "1.0.0", "source": "registry",
         "license": "MIT", "authors": [], "repository": null,
         "manifest_path": "/r/cli-only-1.0.0/Cargo.toml", "targets": [{"kind": ["lib"]}]}
      ],
      "resolve": {"nodes": [
        {"id": "core", "deps": [
          {"pkg": "tor", "dep_kinds": [{"kind": null}]},
          {"pkg": "tester", "dep_kinds": [{"kind": "dev"}]},
          {"pkg": "builder", "dep_kinds": [{"kind": "build"}]},
          {"pkg": "macros", "dep_kinds": [{"kind": null}]}]},
        {"id": "tor", "deps": [{"pkg": "plain", "dep_kinds": [{"kind": null}, {"kind": "dev"}]}]},
        {"id": "plain", "deps": [{"pkg": "deep", "dep_kinds": [{"kind": null}]}]},
        {"id": "deep", "deps": []},
        {"id": "tester", "deps": []},
        {"id": "builder", "deps": []},
        {"id": "macros", "deps": [{"pkg": "inside", "dep_kinds": [{"kind": null}]}]},
        {"id": "inside", "deps": []},
        {"id": "other", "deps": [{"pkg": "cli-only", "dep_kinds": [{"kind": null}]}]},
        {"id": "cli-only", "deps": []}
      ]}
    }"#;

    #[test]
    fn the_packages_in_the_core_are_what_it_is_built_from_and_no_more() {
        let found = packages_in(METADATA, "veil_core").unwrap();
        let names: Vec<&str> = found.iter().map(|package| package.name.as_str()).collect();
        assert_eq!(names, ["deep", "plain"]);
        let plain = found
            .iter()
            .find(|package| package.name == "plain")
            .unwrap();
        assert_eq!(plain.version, "1.2.3");
        assert_eq!(plain.licence, "MIT");
        assert_eq!(plain.authors, ["A"]);
        assert_eq!(plain.folder, Path::new("/r/plain-1.2.3"));
    }

    #[test]
    fn metadata_that_is_something_else_is_said_to_be() {
        assert!(matches!(
            packages_in("{", "veil_core"),
            Err(Problem::Metadata(_))
        ));
        assert!(matches!(
            packages_in("{}", "veil_core"),
            Err(Problem::Metadata(_))
        ));
        assert!(matches!(
            packages_in(METADATA, "no_such_package"),
            Err(Problem::Metadata(_))
        ));
    }

    #[test]
    fn a_text_several_packages_carry_is_there_once_with_all_of_them() {
        let root = folder("shared");
        let same = "Permission is granted.\r\nNo warranty.  \n\n";
        let packages = BTreeSet::from([
            package(&root, "one", "MIT", &[("LICENSE", same)]),
            package(
                &root,
                "two",
                "MIT",
                &[("LICENSE-MIT", "Permission is granted.\nNo warranty.")],
            ),
            package(
                &root,
                "three",
                "MIT OR Apache-2.0",
                &[
                    ("LICENSE-MIT", "Another text."),
                    ("COPYING", same),
                    ("README.md", "not one"),
                ],
            ),
        ]);
        let written = notices(&packages).unwrap();
        assert_eq!(
            written,
            format!(
                "one 1.0.0\nthree 1.0.0\ntwo 1.0.0\n\nPermission is granted.\nNo warranty.\n\
                 {DIVIDER}\nthree 1.0.0\n\nAnother text.\n"
            )
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_package_without_a_text_gets_the_text_of_what_it_names() {
        let root = folder("named");
        let packages = BTreeSet::from([
            package(&root, "bare", "MIT OR Apache-2.0", &[]),
            package(&root, "bare-too", "MIT OR Apache-2.0", &[("src.rs", "")]),
        ]);
        let written = notices(&packages).unwrap();
        assert!(written.starts_with("bare 1.0.0\nbare-too 1.0.0\n\nThese packages come without"));
        assert!(written.contains("licence as: MIT OR Apache-2.0"));
        assert!(written.contains("bare 1.0.0: the author of bare; https://example.invalid/bare"));
        assert!(written.contains("The text of MIT:\n\nMIT License"));
        assert!(written.contains("The text of Apache-2.0:\n\n"));
        assert!(written.contains("TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION"));
        assert!(
            !written.contains(DIVIDER),
            "one entry, and no divider in a text"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_package_that_cannot_be_given_a_text_stops_everything() {
        let root = folder("none");
        let unknown = BTreeSet::from([package(&root, "odd", "Odd-1.0", &[])]);
        assert!(matches!(
            notices(&unknown),
            Err(Problem::NoText { licence, .. }) if licence == "Odd-1.0"
        ));
        let nameless = BTreeSet::from([package(&root, "silent", "", &[])]);
        assert!(matches!(notices(&nameless), Err(Problem::NoLicence { .. })));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_line_of_dashes_in_a_text_divides_nothing() {
        let root = folder("dashes");
        let long = "-".repeat(100);
        let text = format!("Above.\n{DIVIDER}\n{long}\nBelow.");
        let packages = BTreeSet::from([package(&root, "ruled", "MIT", &[("LICENSE", &text)])]);
        let written = notices(&packages).unwrap();
        assert_eq!(written.lines().filter(|line| *line == DIVIDER).count(), 0);
        assert_eq!(written.lines().filter(|line| line.len() == 79).count(), 2);
        assert!(written.ends_with("Below.\n"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parts_that_are_no_packages_are_written_the_same_way() {
        let root = folder("parts");
        fs::write(root.join("a.txt"), "Text A.\r\n").unwrap();
        fs::write(root.join("b.txt"), "Text B.").unwrap();
        fs::write(root.join("empty.txt"), "\n\n").unwrap();
        let parts = vec![
            (
                "Second".to_owned(),
                vec![root.join("a.txt"), root.join("b.txt")],
            ),
            ("First 1.0".to_owned(), vec![root.join("a.txt")]),
        ];
        assert_eq!(
            notices_of(&parts).unwrap(),
            format!("First 1.0\nSecond\n\nText A.\n{DIVIDER}\nSecond\n\nText B.\n")
        );
        // A part without any text, and a file that is not there.
        let nothing = vec![("Bare".to_owned(), vec![root.join("empty.txt")])];
        assert!(matches!(
            notices_of(&nothing),
            Err(Problem::NoLicence { .. })
        ));
        let missing = vec![("Gone".to_owned(), vec![root.join("no-such-file")])];
        assert!(matches!(notices_of(&missing), Err(Problem::File { .. })));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn licences_are_read_out_of_an_expression() {
        assert_eq!(named_in("MIT OR Apache-2.0"), ["MIT", "Apache-2.0"]);
        assert_eq!(named_in("MIT/Apache-2.0"), ["MIT", "Apache-2.0"]);
        assert_eq!(
            named_in("(MIT OR Apache-2.0) AND Unicode-3.0"),
            ["MIT", "Apache-2.0", "Unicode-3.0"]
        );
        assert_eq!(
            named_in("Apache-2.0 WITH LLVM-exception"),
            ["Apache-2.0", "LLVM-exception"]
        );
    }
}
