//! Reaching Tor through bridges, on the real network.
//!
//! Ignored by default and refused on a CI runner. These tests start a Tor
//! client and a pluggable-transport program. Run them with
//! `../check.sh bridges`; `../check.sh setup` installs the programs.
//!
//! The machine these run on reaches Tor directly, so they show that a
//! bridged connection works and that bridges are never gone around. They
//! cannot show that a bridge gets through a network that blocks Tor.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use veil_tor::{Bridges, Purpose, State, TorConfig, TorService, TransportProgram};

/// The obfs4 bridges built into Tor Browser, as served by the Tor Project
/// at bridges.torproject.org/moat/circumvention/builtin. They are public
/// and change now and then; `VEIL_TEST_OBFS4_BRIDGES` names a file of bridge
/// lines to use instead.
const BUILTIN_OBFS4: &[&str] = &[
    "obfs4 37.218.245.14:38224 D9A82D2F9C2F65A18407B1D2B764F130847F8B5D cert=bjRaMrr1BRiAW8IE9U5z27fQaYgOhX1UCmOpg2pFpoMvo6ZgQMzLsaTzzQNTlm7hNcb+Sg iat-mode=0",
    "obfs4 212.83.43.95:443 BFE712113A72899AD685764B211FACD30FF52C31 cert=ayq0XzCwhpdysn5o0EyDUbmSOx3X/oTEbzDMvczHOdBJKlvIdHHLJGkZARtT4dcBFArPPg iat-mode=1",
    "obfs4 51.222.13.177:80 5EDAC3B810E12B01F6FD8050D2FD3E277B289A08 cert=2uplIpLQ0q9+0qMFrK5pkaYRDOe460LL9WHBvatgkuRr/SL31wBOEupaMMJ6koRE6Ld0ew iat-mode=0",
    "obfs4 146.57.248.225:22 10A6CD36A537FCE513A322361547444B393989F0 cert=K1gDtDAIcUfeLqbstggjIw2rtgIKqdIhUlHp82XRqNSq/mtAjp1BIC9vHKJ2FAEpGssTPw iat-mode=0",
];

/// The Snowflake bridges built into Tor Browser, from the same place, with
/// `front=` added beside `fronts=`: the Snowflake client that Ubuntu 24.04
/// packages knows only the older spelling. `VEIL_TEST_SNOWFLAKE_BRIDGES`
/// names a file of bridge lines to use instead.
const BUILTIN_SNOWFLAKE: &[&str] = &[
    "snowflake 192.0.2.4:80 8838024498816A039FCBBAB14E6F40A0843051FA fingerprint=8838024498816A039FCBBAB14E6F40A0843051FA url=https://1098762253.rsc.cdn77.org/ fronts=app.datapacket.com,www.datapacket.com front=app.datapacket.com ice=stun:stun.epygi.com:3478,stun:stun.uls.co.za:3478,stun:stun.voipgate.com:3478,stun:stun.mixvoip.com:3478,stun:stun.telnyx.com:3478,stun:stun.hot-chilli.net:3478,stun:stun.fitauto.ru:3478,stun:stun.m-online.net:3478 utls-imitate=hellorandomizedalpn",
    "snowflake 192.0.2.3:80 2B280B23E1107BB62ABFC40DDCC8824814F80A72 fingerprint=2B280B23E1107BB62ABFC40DDCC8824814F80A72 url=https://1098762253.rsc.cdn77.org/ fronts=app.datapacket.com,www.datapacket.com front=app.datapacket.com ice=stun:stun.epygi.com:3478,stun:stun.uls.co.za:3478,stun:stun.voipgate.com:3478,stun:stun.mixvoip.com:3478,stun:stun.telnyx.com:3478,stun:stun.hot-chilli.net:3478,stun:stun.fitauto.ru:3478,stun:stun.m-online.net:3478 utls-imitate=hellorandomizedalpn",
];

/// A bridge that does not exist, on an address reserved for documentation.
const NO_SUCH_BRIDGE: &str = "obfs4 192.0.2.55:38114 316E643333645F6D79216558614D3931657A5F5F cert=YXJlIGZyZXF1ZW50bHkgZnVsbCBvZiBsaXR0bGUgbWVzc2FnZXMgeW91IGNhbiBmaW5kLg iat-mode=0";

/// Long-lived public onion services, tried in order.
const KNOWN_ONIONS: &[&str] = &[
    "duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion",
    "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion",
];

const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(300);

/// Finds an installed transport program by any of `names`.
fn installed(names: &[&str]) -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in names {
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(&file);
            if directory.is_absolute() && candidate.is_file() {
                return candidate;
            }
        }
    }
    panic!("none of {names:?} is installed; run ../check.sh setup");
}

/// The bridge lines for a test: from the file `variable` names, or built in.
fn lines(variable: &str, builtin: &[&str]) -> Vec<String> {
    match std::env::var_os(variable) {
        Some(file) => {
            let text = std::fs::read_to_string(&file).expect("the bridge file can be read");
            Bridges::lines_from_text(&text)
        }
        None => builtin.iter().map(|line| line.to_string()).collect(),
    }
}

fn start(name: &str, bridges: Bridges) -> (PathBuf, TorService) {
    // Every test checks this first; again here for a test added later.
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    let root = std::env::temp_dir().join(format!("veil-tor-bridges-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let service = TorService::start(TorConfig {
        state_dir: root.join("state"),
        cache_dir: root.join("cache"),
        bridges,
    })
    .expect("service starts");
    (root, service)
}

fn wait_until_ready(service: &TorService) {
    let started = Instant::now();
    let mut last_percent = u8::MAX;
    let mut last_message = None;
    loop {
        let status = service.status();
        if status.percent != last_percent || status.message != last_message {
            println!("bootstrap {}% {:?}", status.percent, status.message);
            last_percent = status.percent;
            last_message = status.message.clone();
        }
        match status.state {
            State::Ready => {
                println!("ready after {:.0?}", started.elapsed());
                return;
            }
            State::Failed => panic!("Tor failed to start: {:?}", status.message),
            State::Lost => panic!("Tor lost its network: {:?}", status.message),
            State::Bootstrapping => {}
        }
        assert!(
            started.elapsed() < BOOTSTRAP_TIMEOUT,
            "Tor did not bootstrap through the bridges in {BOOTSTRAP_TIMEOUT:?}: {:?}",
            status.message
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// SOCKS5 CONNECT through the listener; returns the stream and reply status.
fn socks5_connect(port: u16, host: &str, target_port: u16) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(180)))
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

/// Whether any known onion service answers an HTTP request through `port`.
fn an_onion_service_answers(port: u16) -> bool {
    for host in KNOWN_ONIONS {
        let (mut stream, status) = socks5_connect(port, host, 80);
        println!("connect {host}:80 -> SOCKS status {status:#04x}");
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

fn bootstrap_and_reach_an_onion_service(name: &str, bridges: Bridges) {
    let (root, service) = start(name, bridges);
    wait_until_ready(&service);
    assert!(
        an_onion_service_answers(service.socks_port(Purpose::Sync)),
        "no onion service answered through the bridges"
    );
    service.stop();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "needs the Tor network and an obfs4 program"]
fn obfs4_bridges_carry_tor() {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    let bridges = Bridges {
        lines: lines("VEIL_TEST_OBFS4_BRIDGES", BUILTIN_OBFS4),
        transports: vec![TransportProgram {
            protocols: vec!["obfs4".into()],
            path: installed(&["lyrebird", "obfs4proxy"]),
            arguments: Vec::new(),
        }],
    };
    bootstrap_and_reach_an_onion_service("obfs4", bridges);
}

#[test]
#[ignore = "needs the Tor network and a Snowflake program"]
fn snowflake_carries_tor() {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    let bridges = Bridges {
        lines: lines("VEIL_TEST_SNOWFLAKE_BRIDGES", BUILTIN_SNOWFLAKE),
        transports: vec![TransportProgram {
            protocols: vec!["snowflake".into()],
            path: installed(&["snowflake-client", "lyrebird"]),
            arguments: Vec::new(),
        }],
    };
    bootstrap_and_reach_an_onion_service("snowflake", bridges);
}

/// With a bridge set, Tor has no other way in. On this machine a direct
/// connection would work within seconds, so a Tor that becomes ready here
/// has gone around its bridge.
#[test]
#[ignore = "needs the Tor network and an obfs4 program"]
fn a_bridge_that_cannot_be_reached_is_not_gone_around() {
    veil_test_guard::refuse_in_hosted_ci("connecting to the Tor network");
    const LONG_ENOUGH_TO_CONNECT_DIRECTLY: Duration = Duration::from_secs(60);

    let bridges = Bridges {
        lines: vec![NO_SUCH_BRIDGE.to_owned()],
        transports: vec![TransportProgram {
            protocols: vec!["obfs4".into()],
            path: installed(&["lyrebird", "obfs4proxy"]),
            arguments: Vec::new(),
        }],
    };
    let (root, service) = start("unreachable", bridges);

    let started = Instant::now();
    let mut last = service.status();
    while started.elapsed() < LONG_ENOUGH_TO_CONNECT_DIRECTLY {
        let status = service.status();
        if status != last {
            println!("bootstrap {}% {:?}", status.percent, status.message);
            last = status.clone();
        }
        assert_ne!(
            status.state,
            State::Ready,
            "Tor connected although its only bridge does not exist"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    // And the listener carries nothing meanwhile.
    let (_stream, status) = socks5_connect(service.socks_port(Purpose::Sync), KNOWN_ONIONS[0], 80);
    assert_ne!(status, 0x00, "a connection was carried without Tor");

    service.stop();
    let _ = std::fs::remove_dir_all(root);
}
