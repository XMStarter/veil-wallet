//! Finding the programs that speak the transports bridges need.

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

/// The programs for the transports in `needed`: the ones the user named,
/// and for the rest whatever is installed on this machine.
pub fn programs_for(
    needed: &[String],
    named: Vec<TransportProgram>,
) -> Result<Vec<TransportProgram>, String> {
    programs_for_in(needed, named, &search_path())
}

fn programs_for_in(
    needed: &[String],
    named: Vec<TransportProgram>,
    directories: &[PathBuf],
) -> Result<Vec<TransportProgram>, String> {
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
            .ok_or_else(|| {
                if names.is_empty() {
                    format!("the bridges need the {transport} transport; name its program with --transport {transport}=FILE")
                } else {
                    format!(
                        "the bridges need the {transport} transport, and none of its programs ({}) is installed; install one, or name it with --transport {transport}=FILE",
                        names.join(", ")
                    )
                }
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
    let file_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    directories
        .iter()
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(&file_name))
        .find(|candidate| candidate.is_file())
}

/// Parses `--transport obfs4,webtunnel=/path/to/lyrebird`.
pub fn parse_named(text: &str) -> Result<TransportProgram, String> {
    let wrong = || {
        format!(
            "`{text}` is not a transport; write name=FILE, for example obfs4=/usr/bin/obfs4proxy"
        )
    };
    let (names, path) = text.split_once('=').ok_or_else(wrong)?;
    let protocols: Vec<String> = names
        .split(',')
        .map(|name| name.trim().to_owned())
        .collect();
    if path.is_empty() || protocols.iter().any(String::is_empty) {
        return Err(wrong());
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(format!(
            "the transport program `{}` must be a full path",
            path.display()
        ));
    }
    Ok(TransportProgram {
        protocols,
        path: path.to_path_buf(),
        arguments: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding empty files with the given program names.
    struct Programs(PathBuf);

    impl Programs {
        fn new(test: &str, names: &[&str]) -> Programs {
            let dir = std::env::temp_dir()
                .join(format!("veil-cli-transports-{test}-{}", std::process::id()));
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
        let programs = programs_for_in(
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
        let programs = programs_for_in(
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
    fn a_named_program_is_used_instead_of_looking() {
        let installed = Programs::new("named", &["obfs4proxy"]);
        let named = parse_named(&format!("obfs4={}", installed.path("mine").display())).unwrap();
        let programs = programs_for_in(
            &needed(&["obfs4"]),
            vec![named.clone()],
            std::slice::from_ref(&installed.0),
        )
        .unwrap();
        assert_eq!(programs, vec![named]);
    }

    #[test]
    fn a_missing_program_is_explained() {
        let nothing = Programs::new("missing", &[]);
        let dirs = std::slice::from_ref(&nothing.0);
        let error = programs_for_in(&needed(&["snowflake"]), Vec::new(), dirs).unwrap_err();
        assert!(error.contains("snowflake-client"), "{error}");
        assert!(error.contains("--transport snowflake=FILE"), "{error}");
        let error = programs_for_in(&needed(&["conjure"]), Vec::new(), dirs).unwrap_err();
        assert!(error.contains("--transport conjure=FILE"), "{error}");
    }

    #[test]
    fn nothing_needed_means_nothing_looked_for() {
        assert!(programs_for_in(&[], Vec::new(), &[]).unwrap().is_empty());
    }

    #[test]
    fn named_transports_are_names_and_a_full_path() {
        let root = if cfg!(windows) {
            "C:\\pt\\lyrebird.exe"
        } else {
            "/opt/pt/lyrebird"
        };
        let program = parse_named(&format!("obfs4, webtunnel={root}")).unwrap();
        assert_eq!(program.protocols, vec!["obfs4", "webtunnel"]);
        assert_eq!(program.path, PathBuf::from(root));
        for wrong in [
            "obfs4",
            "=/opt/pt/lyrebird",
            "obfs4=",
            "obfs4,=x",
            "obfs4=lyrebird",
        ] {
            assert!(parse_named(wrong).is_err(), "{wrong:?} must be refused");
        }
    }
}
