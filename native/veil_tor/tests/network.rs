//! Tests that need the real Tor network.
//!
//! They are ignored by default so that `cargo test` works offline. Run them
//! with `cargo test --test network -- --ignored --nocapture`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use veil_tor::{Bridges, Purpose, State, TorConfig, TorService};

/// Long-lived public onion services, tried in order. One reachable service
/// is enough; any single one may be down. Override with `VEIL_TEST_ONION`
/// (`host.onion:port`).
const KNOWN_ONIONS: &[&str] = &[
    "duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion:80",
    "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion:80",
];

const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(300);

fn start(name: &str) -> (PathBuf, TorService) {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    let root = std::env::temp_dir().join(format!("veil-tor-net-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let service = TorService::start(TorConfig {
        state_dir: root.join("state"),
        cache_dir: root.join("cache"),
        bridges: Bridges::none(),
    })
    .expect("service starts");
    (root, service)
}

fn wait_until_ready(service: &TorService) {
    let started = Instant::now();
    let mut last_percent = u8::MAX;
    loop {
        let status = service.status();
        if status.percent != last_percent {
            println!("bootstrap {}% {:?}", status.percent, status.message);
            last_percent = status.percent;
        }
        match status.state {
            State::Ready => return,
            State::Failed => panic!("Tor failed to start: {:?}", status.message),
            State::Lost => panic!("Tor lost its network: {:?}", status.message),
            State::Bootstrapping => {}
        }
        assert!(
            started.elapsed() < BOOTSTRAP_TIMEOUT,
            "Tor did not bootstrap in {BOOTSTRAP_TIMEOUT:?}: {:?}",
            status.message
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// SOCKS5 CONNECT through the listener; returns the stream and reply status.
fn socks5_connect(port: u16, host: &str, target_port: u16) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .unwrap();
    stream.write_all(&[5, 1, 0]).unwrap();
    let mut method = [0u8; 2];
    stream.read_exact(&mut method).unwrap();
    assert_eq!(method, [5, 0]);

    let mut request = vec![5, 1, 0, 3, host.len() as u8];
    request.extend_from_slice(host.as_bytes());
    request.extend_from_slice(&target_port.to_be_bytes());
    stream.write_all(&request).unwrap();

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).unwrap();
    if head[1] == 0 {
        let rest = if head[3] == 4 { 18 } else { 6 };
        let mut skip = vec![0u8; rest];
        stream.read_exact(&mut skip).unwrap();
    }
    (stream, head[1])
}

fn targets() -> Vec<(String, u16)> {
    let from_env = std::env::var("VEIL_TEST_ONION").ok();
    let list: Vec<&str> = match &from_env {
        Some(value) => vec![value.as_str()],
        None => KNOWN_ONIONS.to_vec(),
    };
    list.into_iter()
        .map(|entry| {
            let (host, port) = entry.rsplit_once(':').expect("host:port");
            (host.to_owned(), port.parse().expect("numeric port"))
        })
        .collect()
}

/// Sends an HTTP request through `port` and reports whether any listed
/// onion service answered with HTTP.
fn any_onion_answers(port: u16) -> bool {
    for (host, target_port) in targets() {
        let (mut stream, status) = socks5_connect(port, &host, target_port);
        println!("connect {host}:{target_port} -> SOCKS status {status:#04x}");
        if status != 0 {
            continue;
        }
        let request = format!("HEAD / HTTP/1.0\r\nHost: {host}\r\n\r\n");
        if stream.write_all(request.as_bytes()).is_err() {
            continue;
        }
        let mut reply = [0u8; 5];
        if stream.read_exact(&mut reply).is_ok() && &reply == b"HTTP/" {
            return true;
        }
    }
    false
}

#[test]
#[ignore = "needs the Tor network"]
fn bootstraps_and_reaches_an_onion_service() {
    let (root, service) = start("onion");
    wait_until_ready(&service);

    assert!(
        any_onion_answers(service.socks_port(Purpose::Sync)),
        "no onion service answered through the sync listener"
    );
    assert!(
        any_onion_answers(service.socks_port(Purpose::Broadcast)),
        "no onion service answered through the broadcast listener"
    );

    // Still onion-only once Tor is up: nothing may leave through an exit.
    let (_stream, status) = socks5_connect(service.socks_port(Purpose::Sync), "example.com", 80);
    assert_eq!(status, 0x02, "clearnet target must be refused");

    service.stop();
    let _ = std::fs::remove_dir_all(root);
}
