//! Something that answers like a Monero node, just far enough for the
//! wallet library to start synchronising with it.
//!
//! It tells the wallet its version and a chain height, as a node does, and
//! has no blocks: a request for them gets a long answer of nothing. The
//! tests cut that answer short and watch what the wallet makes of a
//! connection lost in the middle of a synchronisation. A synchronisation
//! that succeeds needs a real node and is tested on a private chain.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// The size of the answer to a request for blocks.
pub const BLOCKS_REPLY_BYTES: usize = 256 * 1024;

pub struct FakeNode {
    port: u16,
    asked: Arc<Mutex<Vec<String>>>,
}

impl FakeNode {
    /// Starts a node that reports a chain of `height` blocks.
    pub fn start(height: u64) -> FakeNode {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback port");
        let port = listener.local_addr().expect("local address").port();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&asked);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let log = Arc::clone(&log);
                std::thread::spawn(move || serve(stream, height, &log));
            }
        });
        FakeNode { port, asked }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// What has been asked so far: the path of each request, followed by
    /// the method for a JSON-RPC call.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

/// Answers requests on one connection until it closes.
fn serve(mut stream: TcpStream, height: u64, log: &Mutex<Vec<String>>) {
    while let Some((path, body)) = read_request(&mut stream) {
        let (kind, reply) = if path == "/json_rpc" {
            let method = json_string(&body, "method").unwrap_or_default();
            log.lock().unwrap().push(format!("{path} {method}"));
            // One answer for every method: each reads the fields it knows.
            // Version 3.16 is what the pinned wallet library speaks.
            let reply = format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":\"0\",\"result\":{{\"status\":\"OK\",\
                 \"untrusted\":false,\"version\":{version},\"release\":true,\
                 \"current_height\":{height},\"height\":{height},\"target_height\":0,\
                 \"enabled\":true,\"earliest_height\":1}}}}",
                version = (3 << 16) | 16,
            );
            ("application/json", reply.into_bytes())
        } else {
            log.lock().unwrap().push(path);
            ("application/octet-stream", vec![0u8; BLOCKS_REPLY_BYTES])
        };
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\r\n",
            reply.len()
        );
        if stream.write_all(head.as_bytes()).is_err() || stream.write_all(&reply).is_err() {
            return;
        }
    }
}

/// Reads one HTTP request and returns its path and body.
fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut received = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        match stream.read(&mut chunk) {
            Ok(count) if count > 0 => received.extend_from_slice(&chunk[..count]),
            _ => return None,
        }
        // The wallet library first tries TLS on a new connection and falls
        // back to plain HTTP when that is refused. Refuse at once; left
        // unanswered, it waits twenty seconds.
        if !received[0].is_ascii_uppercase() {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&received[..head_end]).into_owned();
    let path = head.split_whitespace().nth(1)?.to_owned();
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while received.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(count) if count > 0 => received.extend_from_slice(&chunk[..count]),
            _ => return None,
        }
    }
    let body = String::from_utf8_lossy(&received[head_end..head_end + length]).into_owned();
    Some((path, body))
}

/// The value of a string field in a flat piece of JSON.
fn json_string(json: &str, field: &str) -> Option<String> {
    let after = json.split_once(&format!("\"{field}\""))?.1;
    let value = after.split_once('"')?.1;
    Some(value.split_once('"')?.0.to_owned())
}
