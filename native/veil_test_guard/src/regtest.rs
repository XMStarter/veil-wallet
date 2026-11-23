//! A Monero daemon on a private chain, for tests.
//!
//! This is the only place in the workspace that starts a daemon, and it
//! refuses to on a CI runner. The daemon runs in regtest mode: offline, on a
//! chain of its own, with blocks produced on request. No real funds are
//! involved.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::refuse_in_hosted_ci;

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
        let rpc_port = free_port();
        let p2p_port = free_port();
        let data_dir =
            std::env::temp_dir().join(format!("veil-monerod-{}-{rpc_port}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        std::fs::create_dir_all(&data_dir).expect("daemon data directory");
        let child = Command::new(binary)
            .arg("--regtest")
            .arg("--offline")
            .arg("--fixed-difficulty=1")
            .arg("--non-interactive")
            .arg("--no-zmq")
            .arg("--disable-rpc-ban")
            .arg("--log-level=0")
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
        let daemon = RegtestDaemon {
            child,
            rpc_port,
            data_dir,
        };
        let started = Instant::now();
        while daemon.rpc("get_info", "{}").is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "monerod did not answer within a minute"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        Some(daemon)
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

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port()
}
