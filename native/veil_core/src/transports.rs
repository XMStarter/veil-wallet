//! Finding the programs that speak the transports bridges need.
//!
//! Tor does not speak obfs4, Snowflake or WebTunnel itself; a separate
//! program does, and Tor starts it. On a desktop those programs are not
//! part of Veil yet: they are looked for among the programs installed on
//! the machine. An app that brings one along says where it lies
//! ([`programs_with`]); the app for Android does, because a phone has
//! none installed and lets an app start only what came in its package.

use std::fmt;
use std::path::{Path, PathBuf};

use veil_tor::TransportProgram;

/// Program names to look for, by transport, in order of preference.
/// `lyrebird` is the Tor Project's current program for obfs4 and WebTunnel;
/// `obfs4proxy` is its predecessor, still what Debian and Ubuntu package.
const KNOWN_PROGRAMS: &[(&str, &[&str])] = &[
    ("obfs4", &["lyrebird", "obfs4proxy"]),
    ("meek_lite", &["lyrebird", "obfs4proxy"]),
    ("webtunnel", &["lyrebird", "webtunnel-client"]),
    ("snowflake", &["snowflake-client", "lyrebird"]),
];

/// A transport that bridges need and that no program found speaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingTransport {
    /// The transport, as bridge lines name it.
    pub transport: String,
    /// The programs that were looked for. Empty if none is known for it.
    pub programs: Vec<&'static str>,
}

impl fmt::Display for MissingTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let transport = &self.transport;
        if self.programs.is_empty() {
            write!(
                f,
                "the bridges need the {transport} transport, and no program is known for it"
            )
        } else {
            write!(
                f,
                "the bridges need the {transport} transport, and none of its programs ({}) is installed",
                self.programs.join(", ")
            )
        }
    }
}

/// The programs for the transports in `needed`: the ones in `named`, and
/// for the rest whatever is installed on this machine.
pub fn programs_for(
    needed: &[String],
    named: Vec<TransportProgram>,
) -> Result<Vec<TransportProgram>, MissingTransport> {
    programs_in(needed, named, &search_path())
}

/// Like [`programs_for`], looking first in `shipped`, the folder an app
/// keeps the programs it brings along in, if it has one.
///
/// There a program may lie under its own name or as `lib<name>.so`: the
/// only kind of file Android unpacks from a package into a folder whose
/// files may be started, so a program that travels in one is named like
/// a library.
pub fn programs_with(
    needed: &[String],
    named: Vec<TransportProgram>,
    shipped: Option<&Path>,
) -> Result<Vec<TransportProgram>, MissingTransport> {
    let mut directories: Vec<PathBuf> = shipped.map(Path::to_path_buf).into_iter().collect();
    directories.extend(search_path());
    programs_in(needed, named, &directories)
}

/// Like [`programs_for`], looking in `directories` only.
pub fn programs_in(
    needed: &[String],
    named: Vec<TransportProgram>,
    directories: &[PathBuf],
) -> Result<Vec<TransportProgram>, MissingTransport> {
    let mut programs = named;
    for transport in needed {
        if programs
            .iter()
            .any(|program| program.protocols.contains(transport))
        {
            continue;
        }
        let names = KNOWN_PROGRAMS
            .iter()
            .find(|(known, _)| known == transport)
            .map(|(_, names)| *names)
            .unwrap_or_default();
        let found = names
            .iter()
            .find_map(|name| find_program(name, directories))
            .ok_or_else(|| MissingTransport {
                transport: transport.clone(),
                programs: names.to_vec(),
            })?;
        match programs.iter_mut().find(|program| program.path == found) {
            Some(program) => program.protocols.push(transport.clone()),
            None => programs.push(TransportProgram {
                protocols: vec![transport.clone()],
                path: found,
                arguments: Vec::new(),
            }),
        }
    }
    Ok(programs)
}

/// The directories programs are looked for in.
fn search_path() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

fn find_program(name: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    // Its own name, and the name it has inside a package for Android.
    let file_names = [
        format!("{name}{}", std::env::consts::EXE_SUFFIX),
        format!("lib{name}.so"),
    ];
    directories
        .iter()
        .filter(|directory| directory.is_absolute())
        .flat_map(|directory| file_names.iter().map(move |file| directory.join(file)))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding empty files with the given program names.
    struct Programs(PathBuf);

    impl Programs {
        fn new(test: &str, names: &[&str]) -> Programs {
            let dir = std::env::temp_dir().join(format!(
                "veil-core-transports-{test}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            for name in names {
                let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
                std::fs::write(dir.join(file), b"").unwrap();
            }
            Programs(dir)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0
                .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        }
    }

    impl Drop for Programs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn needed(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn installed_programs_are_found_by_transport() {
        let installed = Programs::new("found", &["obfs4proxy", "snowflake-client"]);
        let programs = programs_in(
            &needed(&["obfs4", "snowflake"]),
            Vec::new(),
            std::slice::from_ref(&installed.0),
        )
        .unwrap();
        assert_eq!(programs.len(), 2);
        assert_eq!(programs[0].protocols, vec!["obfs4"]);
        assert_eq!(programs[0].path, installed.path("obfs4proxy"));
        assert_eq!(programs[1].protocols, vec!["snowflake"]);
        assert_eq!(programs[1].path, installed.path("snowflake-client"));
    }

    #[test]
    fn one_program_serves_all_the_transports_it_speaks() {
        let installed = Programs::new("shared", &["lyrebird", "obfs4proxy"]);
        let programs = programs_in(
            &needed(&["obfs4", "webtunnel"]),
            Vec::new(),
            std::slice::from_ref(&installed.0),
        )
        .unwrap();
        assert_eq!(programs.len(), 1, "{programs:?}");
        assert_eq!(programs[0].path, installed.path("lyrebird"));
        assert_eq!(programs[0].protocols, vec!["obfs4", "webtunnel"]);
    }

    #[test]
    fn a_program_given_by_name_is_used_instead_of_looking() {
        let installed = Programs::new("named", &["obfs4proxy"]);
        let named = TransportProgram {
            protocols: vec!["obfs4".to_owned()],
            path: installed.path("mine"),
            arguments: Vec::new(),
        };
        let programs = programs_in(
            &needed(&["obfs4"]),
            vec![named.clone()],
            std::slice::from_ref(&installed.0),
        )
        .unwrap();
        assert_eq!(programs, vec![named]);
    }

    #[test]
    fn a_missing_program_is_named_with_what_was_looked_for() {
        let nothing = Programs::new("missing", &[]);
        let dirs = std::slice::from_ref(&nothing.0);
        let missing = programs_in(&needed(&["snowflake"]), Vec::new(), dirs).unwrap_err();
        assert_eq!(missing.transport, "snowflake");
        assert!(missing.programs.contains(&"snowflake-client"));
        assert!(
            missing.to_string().contains("snowflake-client"),
            "{missing}"
        );
        let missing = programs_in(&needed(&["conjure"]), Vec::new(), dirs).unwrap_err();
        assert!(missing.programs.is_empty());
        assert!(missing.to_string().contains("conjure"), "{missing}");
    }

    #[test]
    fn a_program_that_came_in_a_package_is_found_under_its_library_name() {
        // What an app for Android has in the folder its package was
        // unpacked into.
        let shipped = Programs::new("shipped", &[]);
        let packaged = shipped.0.join("liblyrebird.so");
        std::fs::write(&packaged, b"").unwrap();
        let programs = programs_in(
            &needed(&["obfs4", "snowflake", "webtunnel"]),
            Vec::new(),
            std::slice::from_ref(&shipped.0),
        )
        .unwrap();
        assert_eq!(programs.len(), 1, "{programs:?}");
        assert_eq!(programs[0].path, packaged);
        assert_eq!(
            programs[0].protocols,
            vec!["obfs4", "snowflake", "webtunnel"]
        );
    }

    #[test]
    fn what_the_app_brought_along_comes_before_what_is_installed() {
        let shipped = Programs::new("shipped-first", &[]);
        let packaged = shipped.0.join("liblyrebird.so");
        std::fs::write(&packaged, b"").unwrap();
        let installed = Programs::new("installed-second", &["lyrebird", "obfs4proxy"]);
        let programs = programs_in(
            &needed(&["obfs4"]),
            Vec::new(),
            &[shipped.0.clone(), installed.0.clone()],
        )
        .unwrap();
        assert_eq!(programs[0].path, packaged);

        // And the folder named by the app is looked in at all.
        let programs = programs_with(&needed(&["obfs4"]), Vec::new(), Some(&shipped.0)).unwrap();
        assert_eq!(programs[0].path, packaged);
    }

    #[test]
    fn a_folder_that_is_not_a_full_path_is_not_looked_in() {
        let here = PathBuf::from("no-such-folder-of-transports");
        let missing = programs_in(&needed(&["obfs4"]), Vec::new(), &[here]).unwrap_err();
        assert_eq!(missing.transport, "obfs4");
    }

    #[test]
    fn nothing_needed_means_nothing_looked_for() {
        assert!(programs_in(&[], Vec::new(), &[]).unwrap().is_empty());
    }
}
