//! Asking the user for things.
//!
//! On a terminal, secrets are read without being shown. When input is
//! piped, as in tests and scripts, each answer is one line.

use std::io::{self, BufRead, IsTerminal, Write};

use zeroize::Zeroizing;

/// Asks for something that must not appear on the screen.
pub fn secret(question: &str) -> io::Result<Zeroizing<String>> {
    if io::stdin().is_terminal() {
        rpassword::prompt_password(question).map(Zeroizing::new)
    } else {
        next_line().map(Zeroizing::new)
    }
}

/// Asks for something that may be shown while it is typed.
pub fn line(question: &str) -> io::Result<String> {
    eprint!("{question}");
    io::stderr().flush()?;
    next_line()
}

fn next_line() -> io::Result<String> {
    let mut text = String::new();
    if io::stdin().lock().read_line(&mut text)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the input ended before an answer was given",
        ));
    }
    let answer = text.trim_end_matches(['\r', '\n']).to_owned();
    // The line may have held a secret.
    Zeroizing::new(text);
    Ok(answer)
}
