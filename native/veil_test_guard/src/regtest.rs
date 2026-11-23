//! A Monero daemon on a private chain, for tests.
//!
//! This is the only place in the workspace that starts a daemon, and it
//! refuses to on a CI runner. The daemon runs in regtest mode: offline, on a
//! chain of its own, with blocks produced on request. No real funds are
//! involved.

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::refuse_in_hosted_ci;

/// How long a daemon may take to answer its first request.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// How often starting the daemon is tried. A daemon that does not come up
/// is started again on other ports; every failure is reported, with the
/// end of the daemon's log, also when a later attempt works.
const START_ATTEMPTS: usize = 3;

/// A regtest daemon that is killed, and its chain deleted, when dropped.
pub struct RegtestDaemon {
    child: Child,
    rpc_port: u16,
    data_dir: PathBuf,
}

impl RegtestDaemon {
    /// Starts the daemon named by `VEIL_MONEROD` and waits until it answers.
    ///
    /// Without that variable the calling test is skipped: this returns
    /// `None`, unless `VEIL_REQUIRE_WALLET_LIB` is set, which turns the skip
    /// into a failure.
    pub fn start() -> Option<RegtestDaemon> {
        refuse_in_hosted_ci("starting a Monero daemon");
        let Some(binary) = std::env::var_os("VEIL_MONEROD") else {
            assert!(
                std::env::var_os("VEIL_REQUIRE_WALLET_LIB").is_none(),
                "VEIL_MONEROD is not set but the daemon is required"
            );
            eprintln!("skipped: set VEIL_MONEROD to a monerod binary to run this test");
            return None;
        };
        // One daemon is started at a time. Tests run side by side, and a
        // port found free for one daemon must not be handed to the next
        // before the first has taken it.
        static STARTING: Mutex<()> = Mutex::new(());
        let _one_at_a_time = STARTING.lock().unwrap_or_else(|e| e.into_inner());

        let mut failures = Vec::new();
        for _ in 0..START_ATTEMPTS {
            match Self::start_once(&binary) {
                Ok(daemon) => {
                    // Written past the test harness, which shows a passing
                    // test's output to nobody: a start that needed a second
                    // attempt has to be seen in the log.
                    for failure in &failures {
                        let _ = writeln!(
                            std::io::stderr(),
                            "note: an attempt to start monerod failed before one worked: {failure}"
                        );
                    }
                    return Some(daemon);
                }
                Err(failure) => failures.push(failure),
            }
        }
        panic!(
            "monerod did not start in {START_ATTEMPTS} attempts:\n{}",
            failures.join("\n")
        );
    }

    /// Starts the daemon on ports of its own and waits until it answers.
    /// On failure, says what is known: how it ended and the end of its log.
    fn start_once(binary: &OsStr) -> Result<RegtestDaemon, String> {
        let (rpc_port, p2p_port) = two_free_ports();
        Self::start_on(binary, rpc_port, p2p_port)
    }

    /// Starts the daemon on the given ports and waits until it answers.
    fn start_on(binary: &OsStr, rpc_port: u16, p2p_port: u16) -> Result<RegtestDaemon, String> {
        // Checked here as well as in `start`: nothing is ever run with a
        // daemon's command line on a CI runner, whatever the program is.
        refuse_in_hosted_ci("starting a Monero daemon");
        let data_dir =
            std::env::temp_dir().join(format!("veil-monerod-{}-{rpc_port}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        std::fs::create_dir_all(&data_dir).expect("daemon data directory");
        let log_file = data_dir.join("monerod.log");
        let child = Command::new(binary)
            .arg("--regtest")
            .arg("--offline")
            .arg("--fixed-difficulty=1")
            .arg("--non-interactive")
            .arg("--no-zmq")
            .arg("--disable-rpc-ban")
            .arg("--log-level=0")
            .arg(format!("--log-file={}", log_file.display()))
            .arg(format!("--data-dir={}", data_dir.display()))
            .arg("--rpc-bind-ip=127.0.0.1")
            .arg(format!("--rpc-bind-port={rpc_port}"))
            .arg("--p2p-bind-ip=127.0.0.1")
            .arg(format!("--p2p-bind-port={p2p_port}"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("monerod starts");
        let mut daemon = RegtestDaemon {
            child,
            rpc_port,
            data_dir,
        };
        let started = Instant::now();
        loop {
            if daemon.rpc("get_info", "{}").is_some() {
                return Ok(daemon);
            }
            let ended = match daemon.child.try_wait() {
                Ok(Some(status)) => Some(format!("monerod ended by itself ({status})")),
                _ if started.elapsed() > START_TIMEOUT => Some(format!(
                    "monerod was running and did not answer within {} seconds",
                    START_TIMEOUT.as_secs()
                )),
                _ => None,
            };
            if let Some(how) = ended {
                // Dropping `daemon` kills it and removes its files.
                return Err(format!(
                    "{how}; RPC port {rpc_port}, P2P port {p2p_port}; its log ends:\n{}",
                    end_of(&log_file)
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// The daemon's RPC port on `127.0.0.1`.
    pub fn rpc_port(&self) -> u16 {
        self.rpc_port
    }

    /// Calls a JSON-RPC method and returns the response body.
    pub fn rpc(&self, method: &str, params: &str) -> Option<String> {
        let body = format!(r#"{{"jsonrpc":"2.0","id":"0","method":"{method}","params":{params}}}"#);
        let mut stream = TcpStream::connect(("127.0.0.1", self.rpc_port)).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(300)))
            .ok()?;
        write!(
            stream,
            "POST /json_rpc HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .ok()?;
        // The daemon keeps the connection open, so read exactly the body
        // that Content-Length announces instead of waiting for a close.
        let mut response = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(end) = find(&response, b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&response[..end]).to_ascii_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse().ok())?;
                let body_start = end + 4;
                while response.len() < body_start + length {
                    let read = stream.read(&mut chunk).ok()?;
                    if read == 0 {
                        return None;
                    }
                    response.extend_from_slice(&chunk[..read]);
                }
                let body = &response[body_start..body_start + length];
                return Some(String::from_utf8_lossy(body).into_owned());
            }
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                return None;
            }
            response.extend_from_slice(&chunk[..read]);
        }
    }

    /// Asks the daemon to add `count` blocks, crediting `address`.
    pub fn add_blocks(&self, count: u64, address: &str) {
        let params = format!(r#"{{"amount_of_blocks":{count},"wallet_address":"{address}"}}"#);
        let response = self
            .rpc("generateblocks", &params)
            .expect("the daemon answers generateblocks");
        assert!(
            response.contains("\"status\": \"OK\""),
            "generateblocks failed: {response}"
        );
    }
}

impl Drop for RegtestDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Two ports on `127.0.0.1` that are free now and differ from each other.
fn two_free_ports() -> (u16, u16) {
    let listen = || TcpListener::bind(("127.0.0.1", 0)).expect("a free port");
    let port = |listener: &TcpListener| listener.local_addr().expect("a local address").port();
    // Both are held open until both are known, so they cannot be the same.
    let (first, second) = (listen(), listen());
    (port(&first), port(&second))
}

/// The last lines of a log file, or why they cannot be shown.
fn end_of(log_file: &Path) -> String {
    const LINES: usize = 15;
    match std::fs::read(log_file) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let from = lines.len().saturating_sub(LINES);
            if lines.is_empty() {
                "(the log is empty)".to_owned()
            } else {
                lines[from..].join("\n")
            }
        }
        Err(error) => format!("(no log: {error})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No daemon is started here: the program is this test program, which
    /// does not know the daemon's options and ends at once.
    #[test]
    fn a_program_that_ends_at_once_is_reported_with_how_it_ended() {
        // Not on a CI runner: the program gets a daemon's command line, and
        // hosted CI runs nothing that looks like a daemon.
        if crate::ci_marker(|name| std::env::var_os(name)).is_some() {
            return;
        }
        let this_program = std::env::current_exe().unwrap();
        let failure = match RegtestDaemon::start_once(this_program.as_os_str()) {
            Err(failure) => failure,
            Ok(_) => panic!("a test program answered like a daemon"),
        };
        assert!(failure.contains("ended by itself"), "{failure}");
        assert!(failure.contains("RPC port"), "{failure}");
    }

    /// How ports were picked when a daemon once failed to come up: one at
    /// a time, each given back before the daemon took it.
    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .and_then(|listener| listener.local_addr())
            .expect("a free port")
            .port()
    }

    /// One round: two daemons started at the same moment, the way two tests
    /// of one test program did. Returns, for each, how long it took to
    /// answer or what went wrong.
    fn start_a_pair(binary: &OsStr) -> Vec<Result<Duration, String>> {
        std::thread::scope(|scope| {
            let starts: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        let began = Instant::now();
                        // The daemon is stopped as soon as it has answered.
                        RegtestDaemon::start_on(binary, free_port(), free_port())
                            .map(|_daemon| began.elapsed())
                    })
                })
                .collect();
            starts
                .into_iter()
                .map(|start| start.join().expect("a start does not panic"))
                .collect()
        })
    }

    /// Takes and gives back loopback ports all the time, as the other
    /// tests of a test program do while a daemon is starting, until told
    /// to stop.
    fn keep_ports_busy(stop: &std::sync::atomic::AtomicBool) {
        while !stop.load(std::sync::atomic::Ordering::SeqCst) {
            let taken: Vec<TcpListener> = (0..8)
                .filter_map(|_| TcpListener::bind(("127.0.0.1", 0)).ok())
                .collect();
            std::thread::sleep(Duration::from_millis(100));
            drop(taken);
        }
    }

    /// Not a check of Veil: a way to find out why a test daemon once did
    /// not come up within a minute, when the log of that run could not
    /// say. It starts daemons in pairs at the same moment, without what
    /// was added afterwards (one start at a time, ports picked together, a
    /// second attempt), first with nothing else going on and then while
    /// loopback ports are taken and given back around it. Every start that
    /// fails is shown with how the daemon ended and the end of its log, and
    /// fails the test: a failure here is the finding.
    ///
    /// `VEIL_DAEMON_ROUNDS` sets the number of pairs per part (20).
    #[test]
    #[ignore = "starts Monero daemons; run it with ./check.sh daemons"]
    fn daemons_started_side_by_side_come_up() {
        refuse_in_hosted_ci("starting a Monero daemon");
        let binary = std::env::var_os("VEIL_MONEROD").expect("VEIL_MONEROD names the daemon");
        let rounds: usize = std::env::var("VEIL_DAEMON_ROUNDS")
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(20);

        let mut failures = 0;
        for (part, busy) in [("nothing else going on", false), ("ports busy", true)] {
            let stop = std::sync::atomic::AtomicBool::new(false);
            let mut times = Vec::new();
            std::thread::scope(|scope| {
                if busy {
                    scope.spawn(|| keep_ports_busy(&stop));
                }
                for round in 1..=rounds {
                    for outcome in start_a_pair(&binary) {
                        match outcome {
                            Ok(time) => times.push(time),
                            Err(failure) => {
                                failures += 1;
                                println!("{part}, pair {round}: FAILED: {failure}");
                            }
                        }
                    }
                }
                stop.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            times.sort();
            let seconds = |time: Option<&Duration>| time.map_or(0.0, Duration::as_secs_f64);
            println!(
                "{part}: {} of {} daemons came up; time to answer: fastest {:.1}s, \
                 middle {:.1}s, slowest {:.1}s (the limit is {}s)",
                times.len(),
                rounds * 2,
                seconds(times.first()),
                seconds(times.get(times.len() / 2)),
                seconds(times.last()),
                START_TIMEOUT.as_secs(),
            );
        }
        assert_eq!(failures, 0, "{failures} daemons did not come up; see above");
    }

    #[test]
    fn two_ports_are_two() {
        let (first, second) = two_free_ports();
        assert_ne!(first, second);
    }

    #[test]
    fn the_end_of_a_log_is_its_last_lines() {
        let file = std::env::temp_dir().join(format!("veil-log-{}", std::process::id()));
        let lines: Vec<String> = (1..=40).map(|n| format!("line {n}")).collect();
        std::fs::write(&file, lines.join("\n")).unwrap();
        let end = end_of(&file);
        let _ = std::fs::remove_file(&file);
        assert!(
            end.starts_with("line 26") && end.ends_with("line 40"),
            "{end}"
        );
        assert!(end_of(&file).starts_with("(no log"));
    }
}
