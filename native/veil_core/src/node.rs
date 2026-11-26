//! Which Monero nodes a wallet uses.

use std::fmt;
use std::str::FromStr;

use crate::CoreError;

/// A Monero node that is an onion service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnionNode {
    host: String,
    port: u16,
}

impl OnionNode {
    /// `host` is a bare `.onion` name, `port` the node's RPC port.
    pub fn new(host: &str, port: u16) -> Result<OnionNode, CoreError> {
        let host = host.trim().to_ascii_lowercase();
        if !veil_wallet::is_onion_host(&host) || port == 0 {
            return Err(CoreError::InvalidNode);
        }
        Ok(OnionNode { host, port })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Parses `host.onion:port`.
impl FromStr for OnionNode {
    type Err = CoreError;

    fn from_str(text: &str) -> Result<OnionNode, CoreError> {
        let (host, port) = text.trim().rsplit_once(':').ok_or(CoreError::InvalidNode)?;
        // Digits and nothing else: the standard library would take a
        // plus sign in front as well.
        if !port.bytes().all(|b| b.is_ascii_digit()) {
            return Err(CoreError::InvalidNode);
        }
        let port = port.parse().map_err(|_| CoreError::InvalidNode)?;
        OnionNode::new(host, port)
    }
}

impl fmt::Display for OnionNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// The node a wallet synchronises with and the node it sends through.
///
/// A node that serves a wallet's synchronisation learns roughly when the
/// wallet was created and when it is online. A node that receives a
/// transaction first learns that its sender is one hop away. Using two
/// nodes keeps one party from learning both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nodes {
    sync: OnionNode,
    broadcast: OnionNode,
}

impl Nodes {
    /// Synchronise with one node and send through another. The two must be
    /// different onion services.
    pub fn separate(sync: OnionNode, broadcast: OnionNode) -> Result<Nodes, CoreError> {
        if sync.host == broadcast.host {
            return Err(CoreError::SameNode);
        }
        Ok(Nodes { sync, broadcast })
    }

    /// Use one node for everything. This is for a user's own node, which
    /// already knows nothing it should not. Transactions still travel on a
    /// separate Tor circuit.
    pub fn single(node: OnionNode) -> Nodes {
        Nodes {
            sync: node.clone(),
            broadcast: node,
        }
    }

    pub fn sync(&self) -> &OnionNode {
        &self.sync
    }

    pub fn broadcast(&self) -> &OnionNode {
        &self.broadcast
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion";
    const B: &str = "duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion";

    #[test]
    fn a_node_is_an_onion_name_and_a_port() {
        let node: OnionNode = format!("  {}:18081 ", A.to_ascii_uppercase())
            .parse()
            .unwrap();
        assert_eq!(node.host(), A);
        assert_eq!(node.port(), 18081);
        assert_eq!(node.to_string(), format!("{A}:18081"));
    }

    #[test]
    fn anything_else_is_not_a_node() {
        for text in [
            "",
            A,
            "node.example.com:18081",
            "127.0.0.1:18081",
            "localhost:18081",
            &format!("http://{A}:18081"),
            &format!("{A}:0"),
            &format!("{A}:port"),
            &format!("{A}:70000"),
            &format!("user@{A}:18081"),
        ] {
            assert!(
                matches!(text.parse::<OnionNode>(), Err(CoreError::InvalidNode)),
                "{text:?} must be refused"
            );
        }
    }

    #[test]
    fn separate_nodes_must_be_different_services() {
        let a = OnionNode::new(A, 18081).unwrap();
        let a_other_port = OnionNode::new(A, 18089).unwrap();
        let b = OnionNode::new(B, 18081).unwrap();

        assert!(matches!(
            Nodes::separate(a.clone(), a_other_port),
            Err(CoreError::SameNode)
        ));
        let nodes = Nodes::separate(a.clone(), b.clone()).unwrap();
        assert_eq!(nodes.sync(), &a);
        assert_eq!(nodes.broadcast(), &b);

        let own = Nodes::single(a.clone());
        assert_eq!(own.sync(), own.broadcast());
    }

    /// A node as it is typed into Settings, or as anything else: read as
    /// an onion service and a port, or refused.
    #[test]
    fn made_up_nodes_are_onion_services_with_a_port_or_refused() {
        let plain = format!("{A}:18081");
        let untidy = format!("  {}:38089\n", B.to_ascii_uppercase());
        let under = format!("node.{A}:1");
        let samples = [
            plain.as_str(),
            untidy.as_str(),
            under.as_str(),
            "example.com:18081",
            "127.0.0.1:18081",
            "[::1]:18081",
            "http://node.onion:18081",
            "user@node.onion:18081",
            "node.onion:18081/path",
            "node.onion:+18081",
            "node.onion:0",
            "node.onion:65535",
            "node.onion:65536",
            "node.onion",
        ];
        veil_test_guard::fuzz::texts("node", &samples, |text| {
            let Ok(node) = text.parse::<OnionNode>() else {
                return;
            };
            // Nothing in the name but what a name is made of, so that it
            // can be put into an address as it stands.
            let host = node.host();
            assert!(host.ends_with(".onion") && host.len() > ".onion".len());
            assert!(host
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.'));
            assert!(host.split('.').all(|label| !label.is_empty()));
            assert!(host.parse::<std::net::IpAddr>().is_err());
            assert_ne!(node.port(), 0);
            // It is what was written: the name before the last colon, the
            // port after it in digits.
            let (name, port) = text.trim().rsplit_once(':').unwrap();
            assert_eq!(host, name.trim().to_ascii_lowercase());
            assert!(!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()));
            assert_eq!(port.trim_start_matches('0'), node.port().to_string());
            // And written out, it is read as the same node again.
            assert_eq!(node.to_string().parse::<OnionNode>().unwrap(), node);
        });
    }
}
