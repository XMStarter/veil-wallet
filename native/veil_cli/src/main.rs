//! `veil`: a command-line wallet for trying Veil's core on the Monero
//! stagenet.
//!
//! It is a thin shell over `veil_core`: every command that needs the
//! network starts the embedded Tor client and reaches a stagenet node that
//! is an onion service. The commands that need no network open the wallet
//! without any node set, so they cannot reach one.

mod args;
mod commands;
mod prompt;
mod transports;

use std::process::ExitCode;

fn main() -> ExitCode {
    let options = match args::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("veil: {message}");
            eprintln!("Run `veil --help` to see what it can do.");
            return ExitCode::from(2);
        }
    };
    match commands::run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(commands::Fail(message)) => {
            eprintln!("veil: {message}");
            ExitCode::FAILURE
        }
    }
}
