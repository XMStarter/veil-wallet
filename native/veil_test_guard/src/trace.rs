//! Reading a record of a program's network calls for anything that
//! leaves the machine.
//!
//! Veil's rule is that nothing reaches the network except through Tor.
//! In the tests Tor is a stand-in on loopback, so there the rule can be
//! checked from outside the program: every address any of its network
//! calls names has to be this machine's own. `strace` writes such a
//! record (`strace -f -e trace=network,...`), and [`read`] goes through
//! it. `../check.sh leaks` puts the two together.
//!
//! What counts as leaving the machine:
//!
//! - an address that is not loopback, in any call that carries one;
//! - port 53 on any address, loopback included: that is a name being
//!   looked up, and the resolver on loopback passes the question on;
//! - the sockets of the system's services that look names up (the
//!   resolver's, and the one that finds names on the local network), for
//!   the same reason; and a question for a host put to the service that
//!   caches such answers. That service also answers for the machine's own
//!   lists of users and groups, which Tor's code consults about who owns
//!   its files: those questions stay on the machine and are no finding;
//! - a listener or a socket bound to every interface instead of loopback;
//! - a kind of socket nobody expected, and an address the record does
//!   not show: what cannot be read is not waved through.
//!
//! A program the tests start but that is not Veil, the Monero daemon of
//! the tests on a private chain, is left out by name ([`read`]'s
//! `ignored`): what it does on the network is its own affair, and the
//! wallet's connection to it is in the wallet's part of the record.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};

/// One thing in the record that leaves the machine, or may.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The line of the record, counted from one.
    pub line: usize,
    /// What is wrong with it.
    pub what: String,
    /// The line itself.
    pub text: String,
}

/// What a record held.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Addresses read, in calls of programs that were not left out.
    pub addresses: usize,
    /// Sockets made, by the same programs.
    pub sockets: usize,
    /// Everything that leaves the machine, or may.
    pub findings: Vec<Finding>,
}

/// Kinds of socket that stay on the machine by their nature, or whose
/// addresses are looked at one by one.
const EXPECTED_FAMILIES: &[&str] = &["AF_INET", "AF_INET6", "AF_UNIX", "AF_LOCAL", "AF_NETLINK"];

/// Goes through a record made by `strace -f` of the network calls of a
/// program and of `execve`, `clone`, `clone3`, `fork` and `vfork`.
/// Programs whose file name is in `ignored`, and what they start, are
/// left out.
pub fn read(trace: &str, ignored: &[&str]) -> Report {
    let mut report = Report::default();
    let mut processes = Processes::default();
    let mut cache = NameCache::default();
    for (index, text) in trace.lines().enumerate() {
        let (pid, call) = split_pid(text);
        processes.note(pid, call, ignored);
        if pid.is_some_and(|pid| processes.left_out.contains(&pid)) {
            continue;
        }
        let mut found = |what: String| {
            report.findings.push(Finding {
                line: index + 1,
                what,
                text: text.to_owned(),
            })
        };
        let name = call_name(call);
        if let Some(what) = cache.note(pid.unwrap_or(0), name, call) {
            found(what);
        }
        if name == Some("socket") && !call.starts_with("<...") {
            report.sockets += 1;
            let family = call
                .trim_start_matches("socket(")
                .split([',', ' ', ')'])
                .next()
                .unwrap_or_default();
            if !EXPECTED_FAMILIES.contains(&family) {
                found(format!(
                    "a socket of the kind {family}, which nobody expected"
                ));
            }
        }
        let addresses = addresses_in(call);
        if addresses.is_empty() {
            // `connect` and `bind` always carry an address. One the record
            // shows only as a number cannot be vouched for.
            let carries_one = matches!(name, Some("connect" | "bind"));
            if carries_one && !call.starts_with("<...") && !call.contains("NULL") {
                found("an address the record does not show".into());
            }
            continue;
        }
        for address in addresses {
            report.addresses += 1;
            if let Some(what) = judge(&address, name) {
                found(what);
            }
        }
    }
    report
}

/// The process a line belongs to, and the rest of the line.
fn split_pid(line: &str) -> (Option<u32>, &str) {
    let trimmed = line.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    match trimmed[..digits].parse() {
        Ok(pid) if trimmed[digits..].starts_with(' ') => {
            (Some(pid), trimmed[digits..].trim_start())
        }
        // A record of one process alone has no numbers in front.
        _ => (None, trimmed),
    }
}

/// The name of the call a line is about, also for the second half of a
/// call that another process's line cut in two.
fn call_name(call: &str) -> Option<&str> {
    if let Some(rest) = call.strip_prefix("<... ") {
        return rest.split(' ').next();
    }
    let name = call.split('(').next()?;
    let is_name = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    is_name.then_some(name)
}

/// Which processes are left out: one that has started a program named
/// in `ignored`, and every process such a one makes.
#[derive(Default)]
struct Processes {
    left_out: HashSet<u32>,
    /// Processes whose start of such a program is in the record without
    /// its outcome yet: another process's line came in between.
    starting: HashSet<u32>,
}

impl Processes {
    fn note(&mut self, pid: Option<u32>, call: &str, ignored: &[&str]) {
        let Some(pid) = pid else { return };
        let name = call_name(call);
        if name == Some("execve") {
            // Only a start that worked changes what the process is; after
            // one that failed the old program goes on.
            let worked = call.trim_end().ends_with("= 0");
            if call.starts_with("<...") {
                if self.starting.remove(&pid) && worked {
                    self.left_out.insert(pid);
                }
                return;
            }
            let program = call
                .split('"')
                .nth(1)
                .and_then(|path| path.rsplit('/').next())
                .unwrap_or_default();
            if ignored.contains(&program) {
                if worked {
                    self.left_out.insert(pid);
                } else if call.contains("<unfinished") {
                    self.starting.insert(pid);
                }
            }
            return;
        }
        let makes_one = matches!(name, Some("clone" | "clone3" | "fork" | "vfork"));
        if makes_one && self.left_out.contains(&pid) {
            let child = call
                .rsplit(" = ")
                .next()
                .and_then(|result| result.split_whitespace().next())
                .and_then(|number| number.parse::<u32>().ok());
            if let Some(child) = child {
                self.left_out.insert(child);
            }
        }
    }
}

/// Connections to the system's cache of name lookups (`nscd`). It answers
/// for the machine's own lists of users and groups as well as for names
/// of hosts, so the socket alone says nothing: the question does. The
/// first thing sent after a connection that worked is the question, and
/// its second number is its kind. Where the service does not run, the
/// connection fails, nothing is asked of it, and whatever the program
/// does instead is in the record by itself.
#[derive(Default)]
struct NameCache {
    /// Connections whose outcome another process's line has cut off.
    connecting: HashMap<u32, u32>,
    /// Connections that worked and have not been asked anything yet, by
    /// process and by the number of the socket.
    open: HashSet<(u32, u32)>,
}

/// The kinds of question that are about hosts, as glibc numbers them
/// (`nscd-client.h`): a host by name, by name for IPv6, by address, by
/// address for IPv6, the hosts table itself, and an address lookup.
const HOST_QUESTIONS: &[u32] = &[4, 5, 6, 7, 13, 14];

impl NameCache {
    /// Takes note of a line; returns what is wrong with it, if anything.
    fn note(&mut self, pid: u32, name: Option<&str>, call: &str) -> Option<String> {
        let worked = call.trim_end().ends_with("= 0");
        match name {
            Some("connect") if call.starts_with("<...") => {
                let socket = self.connecting.remove(&pid)?;
                if worked {
                    self.open.insert((pid, socket));
                }
                None
            }
            Some("connect") if call.contains("nscd") => {
                let socket = first_number(call)?;
                if worked {
                    self.open.insert((pid, socket));
                } else if call.contains("<unfinished") {
                    self.connecting.insert(pid, socket);
                }
                None
            }
            Some("sendto" | "sendmsg" | "send") if !call.starts_with("<...") => {
                let socket = first_number(call)?;
                if !self.open.remove(&(pid, socket)) {
                    return None;
                }
                let question = between(call, "\"", "\"").map(bytes_of).unwrap_or_default();
                let kind = question
                    .get(4..8)
                    .map(|kind| u32::from_le_bytes([kind[0], kind[1], kind[2], kind[3]]));
                match kind {
                    Some(kind) if HOST_QUESTIONS.contains(&kind) => {
                        Some("the system's cache of names was asked for a host".into())
                    }
                    Some(_) => None,
                    None => Some(
                        "the system's cache of names was asked something the record does not show"
                            .into(),
                    ),
                }
            }
            _ => None,
        }
    }
}

/// The first argument of a call, when it is a number: the socket.
fn first_number(call: &str) -> Option<u32> {
    let arguments = &call[call.find('(')? + 1..];
    let digits = arguments.chars().take_while(char::is_ascii_digit).count();
    arguments[..digits].parse().ok()
}

/// The bytes a text of the record stands for: `strace` writes what is not
/// a letter as a backslash and a letter, or a backslash and up to three
/// digits in base eight.
fn bytes_of(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut rest = text.bytes().peekable();
    while let Some(byte) = rest.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        match rest.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b'v') => bytes.push(0x0b),
            Some(b'f') => bytes.push(0x0c),
            Some(b'a') => bytes.push(0x07),
            Some(b'b') => bytes.push(0x08),
            Some(b'e') => bytes.push(0x1b),
            Some(digit @ b'0'..=b'7') => {
                let mut value = u32::from(digit - b'0');
                for _ in 0..2 {
                    match rest.peek() {
                        Some(next @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(next - b'0');
                            rest.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(value as u8);
            }
            Some(other) => bytes.push(other),
            None => {}
        }
    }
    bytes
}

/// An address as the record writes it: the text between the braces that
/// begin with `sa_family=`.
fn addresses_in(call: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = call;
    while let Some(start) = rest.find("{sa_family=") {
        let from = &rest[start..];
        let end = closing_brace(from).unwrap_or(from.len());
        found.push(from[..end].to_owned());
        rest = &from[end..];
    }
    found
}

/// Where the address that starts at the beginning of `text` ends: the
/// first closing brace that is not inside quotes.
fn closing_brace(text: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '}' if !quoted => return Some(at + 1),
            _ => {}
        }
    }
    None
}

/// What is wrong with an address, if anything.
fn judge(address: &str, call: Option<&str>) -> Option<String> {
    let family = address
        .trim_start_matches("{sa_family=")
        .split([',', '}'])
        .next()
        .unwrap_or_default();
    match family {
        "AF_NETLINK" | "AF_UNSPEC" => None,
        "AF_UNIX" | "AF_LOCAL" => {
            let path = address.split("sun_path=").nth(1).unwrap_or_default();
            // These two answer for names of hosts and nothing else. The
            // cache of such answers is looked at by what it is asked;
            // see `NameCache`.
            let resolver = path.contains("resolve") || path.contains("avahi");
            resolver.then(|| "the system's resolver was asked: a name was looked up".into())
        }
        "AF_INET" => {
            let Some(ip) =
                between(address, "inet_addr(\"", "\"").and_then(|ip| ip.parse::<Ipv4Addr>().ok())
            else {
                return Some("an address that cannot be read".into());
            };
            judge_ip(
                ip.is_loopback(),
                ip.is_unspecified(),
                port_of(address),
                call,
            )
        }
        "AF_INET6" => {
            let Some(ip) = between(address, "inet_pton(AF_INET6, \"", "\"")
                .and_then(|ip| ip.parse::<Ipv6Addr>().ok())
            else {
                return Some("an address that cannot be read".into());
            };
            let loopback =
                ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback());
            judge_ip(loopback, ip.is_unspecified(), port_of(address), call)
        }
        other => Some(format!(
            "an address of the kind {other}, which nobody expected"
        )),
    }
}

fn judge_ip(
    loopback: bool,
    unspecified: bool,
    port: Option<u16>,
    call: Option<&str>,
) -> Option<String> {
    if port == Some(53) {
        return Some("port 53: a name was looked up".into());
    }
    if loopback {
        return None;
    }
    if unspecified && call == Some("bind") {
        return Some("bound to every interface, not to loopback alone".into());
    }
    Some("an address outside this machine".into())
}

fn port_of(address: &str) -> Option<u16> {
    between(address, "port=htons(", ")").and_then(|port| port.parse().ok())
}

/// The text between the first `before` and the next `after`.
fn between<'a>(text: &'a str, before: &str, after: &str) -> Option<&'a str> {
    let rest = &text[text.find(before)? + before.len()..];
    Some(&rest[..rest.find(after)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whats(trace: &str) -> Vec<String> {
        read(trace, &[])
            .findings
            .into_iter()
            .map(|finding| finding.what)
            .collect()
    }

    #[test]
    fn calls_that_stay_on_the_machine_are_no_findings() {
        // As strace writes them, taken from a run of the core's tests.
        let trace = r#"940   socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC|SOCK_NONBLOCK, IPPROTO_IP) = 21
940   bind(21, {sa_family=AF_INET, sin_port=htons(0), sin_addr=inet_addr("127.0.0.1")}, 16 <unfinished ...>
938   socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC|SOCK_NONBLOCK, IPPROTO_IP <unfinished ...>
940   <... bind resumed>)               = 0
938   <... socket resumed>)             = 24
941   connect(25, {sa_family=AF_INET, sin_port=htons(40123), sin_addr=inet_addr("127.0.0.1")}, 16) = -1 EINPROGRESS (Operation now in progress)
941   accept4(21, {sa_family=AF_INET, sin_port=htons(51000), sin_addr=inet_addr("127.0.0.1")}, [128 => 16], SOCK_CLOEXEC) = 26
941   connect(27, {sa_family=AF_INET6, sin6_port=htons(8080), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "::1", &sin6_addr), sin6_scope_id=0}, 28) = 0
941   connect(28, {sa_family=AF_INET6, sin6_port=htons(8080), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "::ffff:127.0.0.1", &sin6_addr), sin6_scope_id=0}, 28) = 0
941   connect(29, {sa_family=AF_UNIX, sun_path="/tmp/some}thing/socket"}, 110) = 0
941   bind(30, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 0
941   sendto(25, "data", 4, MSG_NOSIGNAL, NULL, 0) = 4
941   connect(31, {sa_family=AF_UNSPEC, sa_data="\0\0\0\0\0\0\0\0\0\0\0\0\0\0"}, 16) = 0
941   +++ exited with 0 +++
"#;
        let report = read(trace, &[]);
        assert_eq!(report.findings, Vec::new());
        assert_eq!(report.sockets, 2);
        assert_eq!(report.addresses, 8);
    }

    #[test]
    fn an_address_outside_the_machine_is_found_in_any_call() {
        let outside = [
            r#"7 connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("93.184.216.34")}, 16) = 0"#,
            r#"7 sendto(3, "x", 1, 0, {sa_family=AF_INET, sin_port=htons(9001), sin_addr=inet_addr("10.0.0.5")}, 16) = 1"#,
            r#"7 sendmsg(3, {msg_name={sa_family=AF_INET6, sin6_port=htons(443), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "2001:db8::1", &sin6_addr), sin6_scope_id=0}, msg_namelen=28, msg_iov=[{iov_base="x", iov_len=1}], msg_iovlen=1, msg_controllen=0, msg_flags=0}, 0) = 1"#,
            r#"7 <... accept4 resumed>{sa_family=AF_INET, sin_port=htons(40000), sin_addr=inet_addr("192.168.1.20")}, [128 => 16], SOCK_CLOEXEC) = 9"#,
            r#"7 connect(3, {sa_family=AF_INET, sin_port=htons(80), sin_addr=inet_addr("0.0.0.0")}, 16) = 0"#,
        ];
        for line in outside {
            assert_eq!(
                whats(line),
                vec!["an address outside this machine"],
                "{line}"
            );
        }
        // The finding says where it is and shows the line.
        let trace = format!(
            "7 socket(AF_INET, SOCK_STREAM, IPPROTO_IP) = 3\n{}\n",
            outside[0]
        );
        let report = read(&trace, &[]);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].line, 2);
        assert_eq!(report.findings[0].text, outside[0]);
    }

    #[test]
    fn a_name_looked_up_is_found_even_on_loopback() {
        // The resolver Ubuntu runs listens on loopback and asks the
        // network itself.
        let stub = r#"7 connect(3, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("127.0.0.53")}, 16) = 0"#;
        assert_eq!(whats(stub), vec!["port 53: a name was looked up"]);
        let far = r#"7 sendto(3, "q", 1, 0, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("8.8.8.8")}, 16) = 1"#;
        assert_eq!(whats(far), vec!["port 53: a name was looked up"]);
        let resolved = r#"7 connect(3, {sa_family=AF_UNIX, sun_path="/run/systemd/resolve/io.systemd.Resolve"}, 42) = 0"#;
        assert_eq!(whats(resolved).len(), 1);
    }

    #[test]
    fn the_cache_of_names_is_judged_by_what_it_is_asked() {
        let connect =
            r#"7 connect(3, {sa_family=AF_UNIX, sun_path="/var/run/nscd/socket"}, 110) = 0"#;
        // Who a user is: Tor's code asks that about the owner of its
        // files. The machine's own list answers; nothing leaves.
        let user = format!(
            "{connect}\n{}\n",
            r#"7 sendto(3, "\2\0\0\0\1\0\0\0\5\0\0\0001000\0", 17, MSG_NOSIGNAL, NULL, 0) = 17"#
        );
        assert_eq!(whats(&user), Vec::<String>::new());
        let groups = format!(
            "{connect}\n{}\n",
            r#"7 sendto(3, "\2\0\0\0\v\0\0\0\7\0\0\0passwd\0", 19, MSG_NOSIGNAL, NULL, 0) = 19"#
        );
        assert_eq!(whats(&groups), Vec::<String>::new());
        // The address of a host: that is a name looked up.
        for question in [
            r#"7 sendto(3, "\2\0\0\0\16\0\0\0\f\0\0\0example.org\0", 24, MSG_NOSIGNAL, NULL, 0) = 24"#,
            r#"7 sendto(3, "\2\0\0\0\4\0\0\0\f\0\0\0example.org\0", 24, MSG_NOSIGNAL, NULL, 0) = 24"#,
            r#"7 sendto(3, "\2\0\0\0\r\0\0\0\6\0\0\0hosts\0", 18, MSG_NOSIGNAL, NULL, 0) = 18"#,
        ] {
            assert_eq!(
                whats(&format!("{connect}\n{question}\n")),
                vec!["the system's cache of names was asked for a host"],
                "{question}"
            );
        }
        // A question the record does not show is not waved through.
        let hidden = format!("{connect}\n7 sendto(3, 0x7ffd00, 24, MSG_NOSIGNAL, NULL, 0) = 24\n");
        assert_eq!(
            whats(&hidden),
            vec!["the system's cache of names was asked something the record does not show"]
        );
        // Where the service does not run, nothing is asked of it, and
        // what is sent on another socket with the same number later is
        // not taken for a question.
        let absent = r#"7 connect(3, {sa_family=AF_UNIX, sun_path="/var/run/nscd/socket"}, 110) = -1 ENOENT (No such file or directory)
7 sendto(3, "\2\0\0\0\16\0\0\0", 8, MSG_NOSIGNAL, NULL, 0) = 8
"#;
        assert_eq!(whats(absent), Vec::<String>::new());
        // The outcome of the connection cut off by another process's line.
        let cut = r#"7 connect(3, {sa_family=AF_UNIX, sun_path="/var/run/nscd/socket"}, 110 <unfinished ...>
8 socket(AF_INET, SOCK_STREAM, IPPROTO_IP) = 4
7 <... connect resumed>)            = 0
7 sendto(3, "\2\0\0\0\16\0\0\0\f\0\0\0example.org\0", 24, MSG_NOSIGNAL, NULL, 0) = 24
"#;
        assert_eq!(whats(cut).len(), 1);
        let cut_and_absent = cut.replace(
            ")            = 0",
            ") = -1 ENOENT (No such file or directory)",
        );
        assert_eq!(whats(&cut_and_absent), Vec::<String>::new());
        // Only the first thing sent is the question.
        let twice = format!(
            "{connect}\n{}\n{}\n",
            r#"7 sendto(3, "\2\0\0\0\1\0\0\0\5\0\0\0001000\0", 17, MSG_NOSIGNAL, NULL, 0) = 17"#,
            r#"7 sendto(3, "\2\0\0\0\16\0\0\0", 8, MSG_NOSIGNAL, NULL, 0) = 8"#
        );
        assert_eq!(whats(&twice), Vec::<String>::new());
    }

    #[test]
    fn the_bytes_of_a_text_of_the_record_are_read_back() {
        assert_eq!(bytes_of(r"\2\0\0\0\v\0\0\0"), vec![2, 0, 0, 0, 11, 0, 0, 0]);
        assert_eq!(bytes_of(r"\16\0\0\0001"), vec![14, 0, 0, 0, b'1']);
        assert_eq!(
            bytes_of(r#"a\n\377\\\"z"#),
            vec![b'a', b'\n', 255, b'\\', b'"', b'z']
        );
    }

    #[test]
    fn a_listener_on_every_interface_is_found() {
        let any = r#"7 bind(3, {sa_family=AF_INET, sin_port=htons(18081), sin_addr=inet_addr("0.0.0.0")}, 16) = 0"#;
        assert_eq!(
            whats(any),
            vec!["bound to every interface, not to loopback alone"]
        );
        let any6 = r#"7 bind(3, {sa_family=AF_INET6, sin6_port=htons(18081), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "::", &sin6_addr), sin6_scope_id=0}, 28) = 0"#;
        assert_eq!(
            whats(any6),
            vec!["bound to every interface, not to loopback alone"]
        );
    }

    #[test]
    fn what_cannot_be_read_is_not_waved_through() {
        let pointer = "7 connect(3, 0x7ffd1234, 16) = 0";
        assert_eq!(whats(pointer), vec!["an address the record does not show"]);
        let odd = "7 socket(AF_PACKET, SOCK_RAW, htons(ETH_P_ALL)) = 3";
        assert_eq!(
            whats(odd),
            vec!["a socket of the kind AF_PACKET, which nobody expected"]
        );
        let family = r#"7 connect(3, {sa_family=AF_BLUETOOTH, sa_data="..."}, 16) = 0"#;
        assert_eq!(
            whats(family),
            vec!["an address of the kind AF_BLUETOOTH, which nobody expected"]
        );
        let garbled = r#"7 connect(3, {sa_family=AF_INET, sin_port=htons(80), sin_addr=inet_addr("not an address")}, 16) = 0"#;
        assert_eq!(whats(garbled), vec!["an address that cannot be read"]);
        // The second half of a call cut in two carries no address of its
        // own; the first half was judged.
        assert_eq!(whats("7 <... connect resumed>) = 0"), Vec::<String>::new());
    }

    #[test]
    fn a_program_that_is_not_veil_is_left_out_with_all_it_starts() {
        let trace = r#"100 socket(AF_INET, SOCK_STREAM, IPPROTO_IP) = 3
100 clone(child_stack=NULL, flags=CLONE_CHILD_CLEARTID|CLONE_CHILD_SETTID|SIGCHLD, child_tidptr=0x7f) = 200
200 connect(3, {sa_family=AF_INET, sin_port=htons(40000), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
200 execve("/home/someone/veil-wallet-lib/out/monerod", ["monerod", "--regtest"], 0x7ffd /* 30 vars */) = 0
200 bind(5, {sa_family=AF_INET, sin_port=htons(28080), sin_addr=inet_addr("0.0.0.0")}, 16) = 0
200 clone3({flags=CLONE_VM|CLONE_FS|CLONE_FILES|CLONE_SIGHAND|CLONE_THREAD, child_tid=0x7f, exit_signal=0, stack=0x7f, stack_size=0x7fff80}, 88 <unfinished ...>
200 <... clone3 resumed> => {parent_tid=[201]}, 88) = 201
201 connect(6, {sa_family=AF_INET, sin_port=htons(18080), sin_addr=inet_addr("203.0.113.9")}, 16) = 0
100 connect(3, {sa_family=AF_INET, sin_port=htons(28081), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
"#;
        // Left out, the daemon and its threads are not looked at; the
        // test that started it is, before and after.
        let report = read(trace, &["monerod"]);
        assert_eq!(report.findings, Vec::new());
        assert_eq!(report.addresses, 2);
        // Not left out, both of its calls are findings.
        assert_eq!(read(trace, &[]).findings.len(), 2);
        // The start of the daemon cut in two by another process's line.
        let cut = r#"400 execve("/opt/monerod", ["monerod"], 0x7ffd /* 3 vars */ <unfinished ...>
401 socket(AF_INET, SOCK_STREAM, IPPROTO_IP) = 3
400 <... execve resumed>)               = 0
400 bind(5, {sa_family=AF_INET, sin_port=htons(28080), sin_addr=inet_addr("0.0.0.0")}, 16) = 0
"#;
        assert_eq!(read(cut, &["monerod"]).findings, Vec::new());
        let cut_and_failed =
            cut.replace(")               = 0", ") = -1 ENOEXEC (Exec format error)");
        assert_eq!(read(&cut_and_failed, &["monerod"]).findings.len(), 1);
        // A program of the same name that did not start changes nothing.
        let failed = r#"300 execve("/usr/bin/monerod", ["monerod"], 0x7ffd /* 3 vars */) = -1 ENOENT (No such file or directory)
300 connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("198.51.100.7")}, 16) = 0
"#;
        assert_eq!(read(failed, &["monerod"]).findings.len(), 1);
    }

    #[test]
    fn a_record_of_one_process_has_no_numbers_in_front() {
        let alone = r#"connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("198.51.100.7")}, 16) = 0"#;
        assert_eq!(whats(alone), vec!["an address outside this machine"]);
    }
}
