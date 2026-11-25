//! Keeps tests that leave the machine out of hosted CI.
//!
//! Some of Veil's tests start a Tor client or a Monero daemon. They are
//! switched off by default (`#[ignore]`, or the `regtest` feature), and the
//! workflows never switch them on. This crate is the second line of
//! defence: such a test calls [`refuse_in_hosted_ci`] first, and fails
//! before anything is started if it finds itself on a CI runner anyway.
//!
//! The crate also holds [`RegtestDaemon`], the one launcher of a test
//! daemon in the workspace, so that no test can start one around the check,
//! and [`trace`], which reads a record of the tests' network calls for
//! anything that leaves the machine (`veil-trace-check`, `check.sh leaks`).
//! And [`fuzz`], which makes up input for whatever reads text or bytes
//! from outside (`check.sh fuzz`).
//!
//! The reason is in `docs/development.md`, "Hosted CI and the hosting
//! account".

use std::ffi::OsString;

pub mod fuzz;
mod regtest;
pub mod trace;

pub use regtest::RegtestDaemon;

/// Environment variables that CI services set on their runners.
/// `GITHUB_ACTIONS` is set on every GitHub Actions runner, self-hosted ones
/// included; `CI` is set by GitHub and by most other services.
const MARKERS: &[&str] = &["GITHUB_ACTIONS", "CI"];

/// Fails the calling test if it runs on a CI runner.
///
/// `what` names the thing the test was about to do, for the message:
/// `refuse_in_hosted_ci("starting a Tor client")`.
///
/// There is no way to switch this off. A test that needs the Tor network or
/// a daemon runs on a machine the project controls.
pub fn refuse_in_hosted_ci(what: &str) {
    if let Some(marker) = ci_marker(|name| std::env::var_os(name)) {
        panic!(
            "refused: {what} is not allowed in hosted CI ({marker} is set). \
             Run this test on a machine the project controls; \
             see docs/development.md."
        );
    }
}

/// Returns the first CI marker that is set to something other than an empty
/// value, `false`, or `0`.
fn ci_marker(lookup: impl Fn(&str) -> Option<OsString>) -> Option<&'static str> {
    MARKERS.iter().copied().find(|name| {
        lookup(name).is_some_and(|value| {
            let value = value.to_string_lossy();
            let value = value.trim();
            !(value.is_empty() || value.eq_ignore_ascii_case("false") || value == "0")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(vars: &'static [(&'static str, &'static str)]) -> Option<&'static str> {
        ci_marker(|name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        })
    }

    #[test]
    fn a_developer_machine_is_not_ci() {
        assert_eq!(with(&[]), None);
        assert_eq!(with(&[("HOME", "/home/dev")]), None);
    }

    #[test]
    fn github_actions_is_ci() {
        assert_eq!(with(&[("GITHUB_ACTIONS", "true")]), Some("GITHUB_ACTIONS"));
        assert_eq!(
            with(&[("CI", "true"), ("GITHUB_ACTIONS", "true")]),
            Some("GITHUB_ACTIONS")
        );
    }

    #[test]
    fn other_ci_services_are_ci() {
        assert_eq!(with(&[("CI", "true")]), Some("CI"));
        assert_eq!(with(&[("CI", "1")]), Some("CI"));
    }

    #[test]
    fn an_explicitly_false_marker_does_not_count() {
        assert_eq!(with(&[("CI", "")]), None);
        assert_eq!(with(&[("CI", "false")]), None);
        assert_eq!(with(&[("CI", "False")]), None);
        assert_eq!(with(&[("CI", "0")]), None);
        assert_eq!(with(&[("CI", " ")]), None);
    }
}
