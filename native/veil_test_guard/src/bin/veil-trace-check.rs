//! Reads records of network calls made by `strace` and says whether
//! anything in them leaves the machine.
//!
//!     veil-trace-check [--ignore PROGRAM]... RECORD...
//!
//! Ends with 0 if nothing does, with 1 if something does, and with 2 if a
//! record cannot be read or holds no network call at all, which means the
//! tracing did not work and proves nothing. `../check.sh leaks` runs it.

use std::process::ExitCode;

use veil_test_guard::trace;

fn main() -> ExitCode {
    let mut ignored = Vec::new();
    let mut records = Vec::new();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--ignore" {
            match arguments.next() {
                Some(program) => ignored.push(program),
                None => {
                    eprintln!("error: --ignore needs the name of a program");
                    return ExitCode::from(2);
                }
            }
        } else {
            records.push(argument);
        }
    }
    if records.is_empty() {
        eprintln!("usage: veil-trace-check [--ignore PROGRAM]... RECORD...");
        return ExitCode::from(2);
    }
    let ignored: Vec<&str> = ignored.iter().map(String::as_str).collect();
    let mut leaks = 0;
    for record in &records {
        let text = match std::fs::read(record) {
            // A record may hold bytes that are no text, in what a program
            // sent; the lines that matter are read all the same.
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                eprintln!("error: {record} cannot be read: {error}");
                return ExitCode::from(2);
            }
        };
        let report = trace::read(&text, &ignored);
        if report.sockets == 0 && report.addresses == 0 {
            eprintln!("error: {record} holds no network call: the tracing did not work");
            return ExitCode::from(2);
        }
        println!(
            "{record}: {} sockets, {} addresses, {} leaving the machine",
            report.sockets,
            report.addresses,
            report.findings.len()
        );
        for finding in &report.findings {
            println!("  line {}: {}", finding.line, finding.what);
            println!("    {}", finding.text);
        }
        leaks += report.findings.len();
    }
    if leaks == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
