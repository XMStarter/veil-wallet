//! The command line: what was asked for.

use std::path::PathBuf;

use veil_core::OnionNode;
use veil_tor::TransportProgram;
use veil_wallet::SendAmount;

use crate::transports;

pub const USAGE: &str = "\
veil: a command-line wallet for trying Veil on the Monero stagenet.
Stagenet coins have no value. This tool does not work with real funds.

usage: veil [options] <command>

commands:
  create                     make a new wallet and show its recovery phrase
  restore [--height N]       restore a wallet from its recovery phrase;
                             N is where scanning starts for a 25-word phrase
  address [--new]            show the address to receive on; --new issues
                             another one although the current one is unused
  seed                       show the recovery phrase
  balance                    show the balance as of the last sync
  history                    list transfers as of the last sync
  sync                       synchronise with the network through Tor
  send <address> <amount>    synchronise, then send; the amount is in XMR,
                             or `all` for the whole balance

options:
  --data-dir DIR             where the wallet and Tor's files live
                             (default: .veil/stagenet in the home directory)
  --wallet-lib FILE          the Monero wallet library
                             (default: $VEIL_WALLET_LIB, then next to veil)
  --node HOST.onion:PORT     for sync and send: use this stagenet node. Give
                             it twice to send through a different node than
                             the one synchronised with. Without it, public
                             stagenet nodes are tried in random order.
  --bridges FILE             for sync and send: connect to Tor through the
                             bridges in FILE, one bridge line per line, for
                             a network that blocks Tor
  --transport NAME=FILE      the program for a transport the bridges use,
                             for example obfs4=/usr/bin/obfs4proxy. Without
                             it, installed programs are looked for.
  -h, --help                 show this text
";

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub data_dir: Option<PathBuf>,
    pub wallet_lib: Option<PathBuf>,
    /// A file of bridge lines to connect to Tor through.
    pub bridges: Option<PathBuf>,
    /// Programs named for the transports those bridges use.
    pub transports: Vec<TransportProgram>,
    pub command: Command,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Create,
    Restore {
        height: u64,
    },
    Address {
        new: bool,
    },
    Seed,
    Balance,
    History,
    Sync {
        nodes: Vec<OnionNode>,
    },
    Send {
        to: String,
        amount: SendAmount,
        nodes: Vec<OnionNode>,
    },
}

/// Parses the arguments after the program name. The error is a sentence to
/// show the user.
pub fn parse<I>(arguments: I) -> Result<Options, String>
where
    I: IntoIterator<Item = String>,
{
    let mut data_dir = None;
    let mut wallet_lib = None;
    let mut nodes = Vec::new();
    let mut bridges = None;
    let mut transports = Vec::new();
    let mut height = None;
    let mut new = false;
    let mut help = false;
    let mut words = Vec::new();

    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match argument.as_str() {
            "-h" | "--help" | "help" => help = true,
            "--data-dir" => data_dir = Some(PathBuf::from(value("--data-dir")?)),
            "--wallet-lib" => wallet_lib = Some(PathBuf::from(value("--wallet-lib")?)),
            "--node" => {
                let text = value("--node")?;
                let node = text
                    .parse()
                    .map_err(|_| format!("`{text}` is not a node; write host.onion:port"))?;
                nodes.push(node);
            }
            "--bridges" => bridges = Some(PathBuf::from(value("--bridges")?)),
            "--transport" => transports.push(transports::parse_named(&value("--transport")?)?),
            "--height" => {
                let text = value("--height")?;
                height = Some(
                    text.parse()
                        .map_err(|_| format!("`{text}` is not a block height"))?,
                );
            }
            "--new" => new = true,
            option if option.starts_with('-') => return Err(format!("unknown option {option}")),
            _ => words.push(argument),
        }
    }

    let mut words = words.into_iter();
    let name = words.next();
    let command = match name.as_deref() {
        _ if help => Command::Help,
        None => Command::Help,
        Some("create") => Command::Create,
        Some("restore") => Command::Restore {
            height: height.take().unwrap_or(0),
        },
        Some("address") => Command::Address {
            new: std::mem::take(&mut new),
        },
        Some("seed") => Command::Seed,
        Some("balance") => Command::Balance,
        Some("history") => Command::History,
        Some("sync") => Command::Sync {
            nodes: std::mem::take(&mut nodes),
        },
        Some("send") => {
            let to = words.next().ok_or("send needs an address and an amount")?;
            let amount = words
                .next()
                .ok_or("send needs an amount after the address")?;
            let amount = if amount.eq_ignore_ascii_case("all") {
                SendAmount::Everything
            } else {
                SendAmount::Exact(
                    amount
                        .parse()
                        .map_err(|error| format!("`{amount}` is not an amount: {error}"))?,
                )
            };
            Command::Send {
                to,
                amount,
                nodes: std::mem::take(&mut nodes),
            }
        }
        Some(other) => return Err(format!("unknown command `{other}`")),
    };

    // Anything left over was given to a command that does not take it.
    if command != Command::Help {
        if let Some(extra) = words.next() {
            return Err(format!("unexpected argument `{extra}`"));
        }
        if !nodes.is_empty() {
            return Err("--node only applies to sync and send".into());
        }
        let online = matches!(command, Command::Sync { .. } | Command::Send { .. });
        if !online && (bridges.is_some() || !transports.is_empty()) {
            return Err("--bridges and --transport only apply to sync and send".into());
        }
        if bridges.is_none() && !transports.is_empty() {
            return Err("--transport is for the bridges given with --bridges".into());
        }
        if height.is_some() {
            return Err("--height only applies to restore".into());
        }
        if new {
            return Err("--new only applies to address".into());
        }
    }

    Ok(Options {
        data_dir,
        wallet_lib,
        bridges,
        transports,
        command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODE_A: &str = "plowsof3t5hogddwabaeiyrno25efmzfxyro2vligremt7sxpsclfaid.onion:38089";
    const NODE_B: &str = "ykqlrp7lumcik3ubzz3nfsahkbplfgqshavmgbxb4fauexqzat6idjad.onion:38089";

    fn parse_words(words: &[&str]) -> Result<Options, String> {
        parse(words.iter().map(|word| word.to_string()))
    }

    fn command(words: &[&str]) -> Command {
        parse_words(words).unwrap().command
    }

    #[test]
    fn nothing_or_help_shows_help() {
        assert_eq!(command(&[]), Command::Help);
        assert_eq!(command(&["--help"]), Command::Help);
        assert_eq!(command(&["send", "-h"]), Command::Help);
    }

    #[test]
    fn plain_commands() {
        assert_eq!(command(&["create"]), Command::Create);
        assert_eq!(command(&["seed"]), Command::Seed);
        assert_eq!(command(&["balance"]), Command::Balance);
        assert_eq!(command(&["history"]), Command::History);
        assert_eq!(command(&["address"]), Command::Address { new: false });
        assert_eq!(
            command(&["address", "--new"]),
            Command::Address { new: true }
        );
        assert_eq!(command(&["restore"]), Command::Restore { height: 0 });
        assert_eq!(
            command(&["restore", "--height", "1500000"]),
            Command::Restore { height: 1_500_000 }
        );
    }

    #[test]
    fn options_may_come_before_or_after_the_command() {
        let before = parse_words(&["--data-dir", "/tmp/w", "--wallet-lib", "lib.so", "sync"]);
        let after = parse_words(&["sync", "--wallet-lib", "lib.so", "--data-dir", "/tmp/w"]);
        assert_eq!(before, after);
        let options = before.unwrap();
        assert_eq!(options.data_dir, Some(PathBuf::from("/tmp/w")));
        assert_eq!(options.wallet_lib, Some(PathBuf::from("lib.so")));
        assert_eq!(options.command, Command::Sync { nodes: vec![] });
    }

    #[test]
    fn nodes_are_onion_addresses_in_the_order_given() {
        let Command::Sync { nodes } = command(&["sync", "--node", NODE_A, "--node", NODE_B]) else {
            panic!("not a sync");
        };
        let nodes: Vec<String> = nodes.iter().map(|node| node.to_string()).collect();
        assert_eq!(nodes, vec![NODE_A, NODE_B]);

        assert!(parse_words(&["sync", "--node", "node.example.com:38081"]).is_err());
        assert!(parse_words(&["sync", "--node", "127.0.0.1:38081"]).is_err());
        assert!(parse_words(&["sync", "--node"]).is_err());
    }

    #[test]
    fn send_takes_an_address_and_an_exact_amount_or_all() {
        assert_eq!(
            command(&["send", "7Addr", "1.5"]),
            Command::Send {
                to: "7Addr".into(),
                amount: SendAmount::Exact("1.5".parse().unwrap()),
                nodes: vec![],
            }
        );
        assert_eq!(
            command(&["send", "7Addr", "ALL"]),
            Command::Send {
                to: "7Addr".into(),
                amount: SendAmount::Everything,
                nodes: vec![],
            }
        );
        for wrong in [
            &["send"][..],
            &["send", "7Addr"],
            &["send", "7Addr", "lots"],
            &["send", "7Addr", "-1"],
            &["send", "7Addr", "1.5", "extra"],
        ] {
            assert!(parse_words(wrong).is_err(), "{wrong:?} must be refused");
        }
    }

    #[test]
    fn bridges_are_a_file_and_optionally_named_programs() {
        let program = if cfg!(windows) {
            "C:\\pt\\obfs4proxy.exe"
        } else {
            "/usr/bin/obfs4proxy"
        };
        let transport = format!("obfs4={program}");
        let options = parse_words(&[
            "sync",
            "--bridges",
            "bridges.txt",
            "--transport",
            &transport,
        ])
        .unwrap();
        assert_eq!(options.bridges, Some(PathBuf::from("bridges.txt")));
        assert_eq!(options.transports.len(), 1);
        assert_eq!(options.transports[0].protocols, vec!["obfs4"]);
        assert_eq!(options.transports[0].path, PathBuf::from(program));

        let plain = parse_words(&["sync"]).unwrap();
        assert_eq!(plain.bridges, None);
        assert!(plain.transports.is_empty());

        for wrong in [
            &["balance", "--bridges", "bridges.txt"][..],
            &["sync", "--transport", &transport],
            &["sync", "--bridges", "b.txt", "--transport", "obfs4"],
            &["sync", "--bridges"],
        ] {
            assert!(parse_words(wrong).is_err(), "{wrong:?} must be refused");
        }
    }

    #[test]
    fn what_a_command_does_not_take_is_refused() {
        for wrong in [
            &["launch"][..],
            &["create", "now"],
            &["create", "--node", NODE_A],
            &["balance", "--height", "5"],
            &["sync", "--new"],
            &["sync", "--fast"],
            &["restore", "--height", "soon"],
        ] {
            assert!(parse_words(wrong).is_err(), "{wrong:?} must be refused");
        }
    }
}
