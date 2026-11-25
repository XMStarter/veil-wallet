//! Reaching Tor on a network that blocks it.
//!
//! A bridge is a Tor relay that is not in the public list. Most bridges are
//! reached through a pluggable transport, which disguises the connection:
//! obfs4, Snowflake, WebTunnel. Tor does not speak those itself. A separate
//! program does, and Tor starts it when a bridge needs it.
//!
//! With bridges set, they are the only way into Tor. If none of them can be
//! reached, Tor does not connect; it never goes around them.

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use arti_client::config::pt::TransportConfigBuilder;
use arti_client::config::{BoolOrAuto, BridgeConfigBuilder, CfgPath, TorClientConfigBuilder};

use crate::service::TorError;

/// The bridges to connect through and the programs that speak their
/// transports. Empty means a direct connection to the Tor network.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Bridges {
    /// Bridge lines as Tor hands them out, one bridge each, for example
    /// `obfs4 192.0.2.55:38114 316E...5F cert=... iat-mode=0`.
    pub lines: Vec<String>,
    /// The programs for the transports those lines name.
    pub transports: Vec<TransportProgram>,
}

/// A program that speaks one or more pluggable transports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportProgram {
    /// The transports it provides, as named in bridge lines: `obfs4`,
    /// `snowflake`, `webtunnel`.
    pub protocols: Vec<String>,
    /// The program file.
    pub path: PathBuf,
    /// Arguments to start it with. Usually none.
    pub arguments: Vec<String>,
}

impl Bridges {
    /// No bridges: connect to the Tor network directly.
    pub fn none() -> Bridges {
        Bridges::default()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Reads bridge lines from text with one bridge per line. Empty lines
    /// and lines that start with `#` are skipped. The lines are not checked
    /// here; [`Bridges::transports_needed`] and starting Tor check them.
    pub fn lines_from_text(text: &str) -> Vec<String> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect()
    }

    /// The transports the bridge lines name, without duplicates, in
    /// alphabetical order. A bridge reached without a transport adds none.
    pub fn transports_needed(&self) -> Result<Vec<String>, TorError> {
        let mut needed = BTreeSet::new();
        for (bridge, _) in self.parsed()? {
            if let Some(transport) = bridge.get_transport() {
                // The parser stores an empty name for "no transport".
                if !transport.is_empty() {
                    needed.insert(transport.to_owned());
                }
            }
        }
        Ok(needed.into_iter().collect())
    }

    /// Parses every line, or says which one is wrong.
    fn parsed(&self) -> Result<Vec<(BridgeConfigBuilder, usize)>, TorError> {
        self.lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                line.parse::<BridgeConfigBuilder>()
                    .map(|bridge| (bridge, index + 1))
                    .map_err(|error| TorError::Bridge {
                        number: index + 1,
                        reason: error.to_string(),
                    })
            })
            .collect()
    }

    /// Writes the bridges into a Tor configuration. Does nothing when there
    /// are none.
    ///
    /// Stricter than Tor itself, which is content when one bridge is usable:
    /// every line must parse, every transport a line names must have a
    /// program, and every program must exist. A setting that is half wrong
    /// is a mistake the user should hear about now, not a bridge silently
    /// left out.
    pub(crate) fn apply(&self, builder: &mut TorClientConfigBuilder) -> Result<(), TorError> {
        if self.is_empty() {
            return Ok(());
        }
        let bridges = self.parsed()?;

        let provided: BTreeSet<&str> = self
            .transports
            .iter()
            .flat_map(|program| program.protocols.iter().map(String::as_str))
            .collect();
        for transport in self.transports_needed()? {
            if !provided.contains(transport.as_str()) {
                return Err(TorError::TransportMissing { transport });
            }
        }

        for program in &self.transports {
            if !program.path.is_file() {
                return Err(TorError::TransportProgram {
                    path: program.path.clone(),
                });
            }
            let mut protocols = Vec::with_capacity(program.protocols.len());
            for name in &program.protocols {
                protocols.push(
                    name.parse()
                        .map_err(|_| TorError::TransportName { name: name.clone() })?,
                );
            }
            let mut transport = TransportConfigBuilder::default();
            transport
                .protocols(protocols)
                // A literal path: nothing in it is expanded.
                .path(CfgPath::new_literal(program.path.clone()))
                .arguments(program.arguments.clone())
                .run_on_startup(false);
            builder.bridges().transports().push(transport);
        }

        for (bridge, _) in bridges {
            builder.bridges().bridges().push(bridge);
        }
        // Explicitly on: bridges are then the only way in, never an extra.
        builder.bridges().enabled(BoolOrAuto::Explicit(true));
        Ok(())
    }
}

/// Bridge lines are handed out one person at a time and are not for logs.
impl fmt::Debug for Bridges {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bridges")
            .field("lines", &format_args!("{} hidden", self.lines.len()))
            .field("transports", &self.transports)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made-up bridges on addresses reserved for documentation.
    const OBFS4: &str = "obfs4 192.0.2.55:38114 316E643333645F6D79216558614D3931657A5F5F cert=YXJlIGZyZXF1ZW50bHkgZnVsbCBvZiBsaXR0bGUgbWVzc2FnZXMgeW91IGNhbiBmaW5kLg iat-mode=0";
    const SNOWFLAKE: &str = "snowflake 192.0.2.3:80 2B280B23E1107BB62ABFC40DDCC8824814F80A72 fingerprint=2B280B23E1107BB62ABFC40DDCC8824814F80A72 url=https://broker.example/ fronts=front.example ice=stun:stun.example:3478 utls-imitate=hellorandomizedalpn";
    const WEBTUNNEL: &str = "webtunnel [2001:db8::1]:443 0123456789ABCDEF0123456789ABCDEF01234567 url=https://cdn.example/path ver=0.0.1";
    const PLAIN: &str = "192.0.2.83:80 0BAC39417268B96B9F514EF763FA6FBA1A788956";

    /// A file that exists, standing in for a transport program. Nothing in
    /// these tests starts it.
    fn existing_file() -> PathBuf {
        std::env::current_exe().expect("the test program has a path")
    }

    fn program(protocols: &[&str]) -> TransportProgram {
        TransportProgram {
            protocols: protocols.iter().map(|name| name.to_string()).collect(),
            path: existing_file(),
            arguments: Vec::new(),
        }
    }

    fn bridges(lines: &[&str], transports: Vec<TransportProgram>) -> Bridges {
        Bridges {
            lines: lines.iter().map(|line| line.to_string()).collect(),
            transports,
        }
    }

    /// Whether Tor accepts a configuration with these bridges. Builds the
    /// configuration only; no client is created and nothing is started.
    fn configure(bridges: &Bridges) -> Result<(), TorError> {
        let mut builder = TorClientConfigBuilder::from_directories("state", "cache");
        bridges.apply(&mut builder)?;
        builder
            .build()
            .map(|_| ())
            .map_err(|error| TorError::Config(error.to_string()))
    }

    #[test]
    fn no_bridges_is_a_direct_connection() {
        assert!(Bridges::none().is_empty());
        assert_eq!(
            Bridges::none().transports_needed().unwrap(),
            Vec::<String>::new()
        );
        configure(&Bridges::none()).unwrap();
    }

    #[test]
    fn the_three_transports_are_understood() {
        let all = bridges(
            &[OBFS4, SNOWFLAKE, WEBTUNNEL],
            vec![program(&["obfs4", "webtunnel"]), program(&["snowflake"])],
        );
        assert_eq!(
            all.transports_needed().unwrap(),
            vec!["obfs4", "snowflake", "webtunnel"]
        );
        configure(&all).unwrap();
    }

    #[test]
    fn a_bridge_without_a_transport_needs_no_program() {
        let plain = bridges(&[PLAIN], Vec::new());
        assert!(plain.transports_needed().unwrap().is_empty());
        configure(&plain).unwrap();
    }

    #[test]
    fn the_word_bridge_in_front_is_accepted() {
        let line = format!("Bridge {OBFS4}");
        configure(&bridges(&[&line], vec![program(&["obfs4"])])).unwrap();
    }

    #[test]
    fn a_wrong_line_is_named_by_its_number() {
        let wrong = bridges(&[OBFS4, "obfs4 not-an-address"], vec![program(&["obfs4"])]);
        assert!(matches!(
            configure(&wrong),
            Err(TorError::Bridge { number: 2, .. })
        ));
        assert!(matches!(
            wrong.transports_needed(),
            Err(TorError::Bridge { number: 2, .. })
        ));
    }

    #[test]
    fn every_transport_named_needs_a_program() {
        // Tor would accept this, because one of the two bridges is usable.
        let half = bridges(&[OBFS4, SNOWFLAKE], vec![program(&["obfs4"])]);
        match configure(&half) {
            Err(TorError::TransportMissing { transport }) => assert_eq!(transport, "snowflake"),
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn a_program_that_is_not_there_is_refused() {
        let mut missing = program(&["obfs4"]);
        missing.path = PathBuf::from("/nonexistent/veil/lyrebird");
        assert!(matches!(
            configure(&bridges(&[OBFS4], vec![missing])),
            Err(TorError::TransportProgram { .. })
        ));
    }

    #[test]
    fn text_is_split_into_lines_without_comments() {
        let text = format!("# from the bridge site\n\n  {OBFS4}  \n{SNOWFLAKE}\n#{PLAIN}\n");
        assert_eq!(Bridges::lines_from_text(&text), vec![OBFS4, SNOWFLAKE]);
        assert!(Bridges::lines_from_text("\n  \n# nothing\n").is_empty());
    }

    #[test]
    fn debug_output_hides_the_lines() {
        let shown = format!("{:?}", bridges(&[OBFS4], vec![program(&["obfs4"])]));
        assert!(!shown.contains("192.0.2.55"), "{shown}");
        assert!(!shown.contains("cert="), "{shown}");
        assert!(shown.contains("1 hidden"), "{shown}");
    }

    /// Whatever is typed or pasted where bridges go: it is read as
    /// bridges or refused with the number of the line, and it is never
    /// quietly a direct connection.
    #[test]
    fn made_up_bridge_lines_are_bridges_or_refused() {
        let file = format!("# from a friend\n\n{OBFS4}\n  {PLAIN}  \r\n{SNOWFLAKE}\n");
        let samples = [OBFS4, SNOWFLAKE, WEBTUNNEL, PLAIN, file.as_str()];
        veil_test_guard::fuzz::texts("bridges", &samples, |text| {
            let lines = Bridges::lines_from_text(text);
            // Every line that says something is kept, as it stands.
            let said: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .collect();
            assert_eq!(lines, said);
            let mut bridges = Bridges {
                lines,
                transports: Vec::new(),
            };
            assert_eq!(bridges.is_empty(), said.is_empty());
            // Nothing of a line where a log would get it.
            let shown = format!("{bridges:?}");
            assert!(said
                .iter()
                .all(|line| line.len() < 8 || !shown.contains(line)));

            match bridges.transports_needed() {
                Ok(needed) => {
                    // Without their programs, bridges that need some are
                    // not a configuration.
                    if !needed.is_empty() {
                        assert!(matches!(
                            configure(&bridges),
                            Err(TorError::TransportMissing { .. })
                        ));
                    }
                    // With them, Tor takes the lines or says why not.
                    let names: Vec<&str> = needed.iter().map(String::as_str).collect();
                    bridges.transports = vec![program(&names)];
                    if needed.is_empty() {
                        bridges.transports.clear();
                    }
                    match configure(&bridges) {
                        Ok(()) | Err(TorError::Config(_) | TorError::TransportName { .. }) => {}
                        Err(other) => panic!("an unexpected refusal: {other}"),
                    }
                }
                Err(TorError::Bridge { number, .. }) => {
                    assert!((1..=said.len()).contains(&number), "line {number}");
                    assert!(matches!(configure(&bridges), Err(TorError::Bridge { .. })));
                }
                Err(other) => panic!("an unexpected refusal: {other}"),
            }
        });
    }
}
