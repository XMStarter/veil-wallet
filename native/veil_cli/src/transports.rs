//! The programs that speak the transports bridges need: the ones named on
//! the command line, and for the rest what `veil_core` finds installed.

use std::path::Path;

use veil_core::transports::MissingTransport;
use veil_tor::TransportProgram;

/// The programs for the transports in `needed`: the ones the user named,
/// and for the rest whatever is installed on this machine.
pub fn programs_for(
    needed: &[String],
    named: Vec<TransportProgram>,
) -> Result<Vec<TransportProgram>, String> {
    veil_core::transports::programs_for(needed, named).map_err(|missing| explain(&missing))
}

/// What to do about a transport without a program, in this tool's terms.
fn explain(missing: &MissingTransport) -> String {
    let transport = &missing.transport;
    if missing.programs.is_empty() {
        format!("{missing}; name its program with --transport {transport}=FILE")
    } else {
        format!("{missing}; install one, or name it with --transport {transport}=FILE")
    }
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
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_missing_program_is_explained() {
        let error = explain(&MissingTransport {
            transport: "snowflake".into(),
            programs: vec!["snowflake-client", "lyrebird"],
        });
        assert!(error.contains("snowflake-client"), "{error}");
        assert!(error.contains("--transport snowflake=FILE"), "{error}");
        let error = explain(&MissingTransport {
            transport: "conjure".into(),
            programs: Vec::new(),
        });
        assert!(error.contains("--transport conjure=FILE"), "{error}");
    }

    #[test]
    fn nothing_needed_means_nothing_looked_for() {
        assert!(programs_for(&[], Vec::new()).unwrap().is_empty());
    }

    #[test]
    fn a_named_program_is_used_instead_of_looking() {
        let named = parse_named(if cfg!(windows) {
            "obfs4=C:\\pt\\mine.exe"
        } else {
            "obfs4=/opt/pt/mine"
        })
        .unwrap();
        let needed = vec!["obfs4".to_owned()];
        assert_eq!(
            programs_for(&needed, vec![named.clone()]).unwrap(),
            vec![named]
        );
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
