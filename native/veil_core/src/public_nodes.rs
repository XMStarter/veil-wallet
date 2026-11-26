//! Which nodes to try, and in what order.
//!
//! The app and the command-line tool work without anything being set up:
//! where the user names no node of their own, they go to public ones that
//! are onion services. There is a list for each network a wallet can be
//! on.

use veil_wallet::Network;

use crate::OnionNode;

/// Public stagenet nodes that are onion services, from the public list at
/// monero.fail. They are here so that the command-line tool and builds of
/// the app for trying work on stagenet, whose coins are worth nothing.
/// Nobody vetted them, and three of the four appear to belong to one
/// operator.
const PUBLIC_STAGENET_NODES: &[&str] = &[
    "plowsof3t5hogddwabaeiyrno25efmzfxyro2vligremt7sxpsclfaid.onion:38089",
    "plowsofe6cleftfmk2raiw5h2x66atrik3nja4bfd3zrfa2hdlgworad.onion:38089",
    "plowsoffjexmxalw73tkjmf422gq6575fc7vicuu4javzn2ynnte6tyd.onion:38089",
    "ykqlrp7lumcik3ubzz3nfsahkbplfgqshavmgbxb4fauexqzat6idjad.onion:38089",
];

/// Public nodes of the main network that are onion services, each run by
/// somebody else, by the names they give themselves: plowsof, Rucknium,
/// HashVault, SupportXMR, and one whose operator is not known to us.
/// They come from the public list at monero.fail. A wallet for real money
/// synchronises with one of them and hands its transactions to another.
///
/// What a node can and cannot do is in `docs/concept.md`: it cannot take
/// funds, it can hold back a transaction, give a wallet a wrong picture
/// of the chain, and learn when a wallet behind Tor is in use. Nobody of
/// this project has looked at how these are run. The owner chose these
/// five after a run in which each of them had answered and agreed with
/// the others; a sixth, which was not there in that run, was taken off.
/// `../check.sh tor` asks every one of them whether it is there. A user's
/// own node replaces them all.
const PUBLIC_MAINNET_NODES: &[&str] = &[
    "plowsof3t5hogddwabaeiyrno25efmzfxyro2vligremt7sxpsclfaid.onion:18089",
    "rucknium757bokwv3ss35ftgc3gzb7hgbvvglbg3hisp7tsj2fkd2nyd.onion:18081",
    "hashvaultsvg2rinvxz7kos77hdfm6zrd5yco3tx2yh2linsmusfwyad.onion:18081",
    "supportxmrcnzyyl4gtmtvql446f4pdqfcrykwaltwsg3trj3ekn3fad.onion:18089",
    "xmrnodei64luhuwm4qfi7jga2k3tdpy7l2ovbledihfjgt2xvvw3paad.onion:18089",
];

/// The public nodes of a network, as they are written here, or none: the
/// third network Monero has, testnet, is one Veil has no use for.
pub fn public_nodes(network: Network) -> Vec<OnionNode> {
    let list: &[&str] = match network {
        Network::Mainnet => PUBLIC_MAINNET_NODES,
        Network::Stagenet => PUBLIC_STAGENET_NODES,
        Network::Testnet => &[],
    };
    list.iter()
        .map(|text| text.parse().expect("the built-in node list is valid"))
        .collect()
}

/// The nodes to try on `network`, in the order to try them: the ones the
/// user named, as named, or else the public ones in an order nobody can
/// predict.
pub fn candidates(network: Network, named: Vec<OnionNode>) -> Vec<OnionNode> {
    if !named.is_empty() {
        return named;
    }
    let mut nodes = public_nodes(network);
    shuffle(&mut nodes);
    nodes
}

/// [`candidates`] on stagenet, for the command-line tool, which knows no
/// other network.
pub fn stagenet_candidates(named: Vec<OnionNode>) -> Vec<OnionNode> {
    candidates(Network::Stagenet, named)
}

/// Fisher-Yates with the operating system's randomness. If that fails, the
/// order stays as it is, which only costs unpredictability.
fn shuffle<T>(items: &mut [T]) {
    for last in (1..items.len()).rev() {
        let mut bytes = [0u8; 8];
        if getrandom::fill(&mut bytes).is_err() {
            return;
        }
        let pick = (u64::from_le_bytes(bytes) % (last as u64 + 1)) as usize;
        items.swap(last, pick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_nodes_are_used_as_given() {
        let named: Vec<OnionNode> = vec![
            PUBLIC_STAGENET_NODES[3].parse().unwrap(),
            PUBLIC_STAGENET_NODES[0].parse().unwrap(),
        ];
        assert_eq!(stagenet_candidates(named.clone()), named);
    }

    #[test]
    fn the_public_list_is_complete_whatever_the_order() {
        let mut got: Vec<String> = stagenet_candidates(Vec::new())
            .iter()
            .map(|node| node.to_string())
            .collect();
        got.sort();
        let mut expected: Vec<String> = PUBLIC_STAGENET_NODES
            .iter()
            .map(|s| s.to_string())
            .collect();
        expected.sort();
        assert_eq!(got, expected);
    }

    #[test]
    fn each_network_has_nodes_of_its_own_and_asks_them_all() {
        for (network, list) in [
            (Network::Mainnet, PUBLIC_MAINNET_NODES),
            (Network::Stagenet, PUBLIC_STAGENET_NODES),
        ] {
            let mut got: Vec<String> = candidates(network, Vec::new())
                .iter()
                .map(|node| node.to_string())
                .collect();
            got.sort();
            let mut expected: Vec<String> = list.iter().map(|s| s.to_string()).collect();
            expected.sort();
            assert_eq!(got, expected);
            // Enough of them that a transaction can go to another node
            // than the one synchronised with.
            assert!(got.len() >= 2);
            got.dedup();
            assert_eq!(got.len(), list.len(), "a node is listed twice");
        }
        // A node named by the user is used on any network, and alone.
        let named: Vec<OnionNode> = vec![PUBLIC_STAGENET_NODES[0].parse().unwrap()];
        assert_eq!(candidates(Network::Mainnet, named.clone()), named);
        assert!(candidates(Network::Testnet, Vec::new()).is_empty());
    }

    /// A node of one network answers a wallet of the other with a refusal
    /// at best. The two lists share no address, and each is on the ports
    /// its network's nodes listen on, which is how a line that was pasted
    /// into the wrong list would show.
    #[test]
    fn no_node_is_on_both_networks() {
        for node in PUBLIC_MAINNET_NODES {
            assert!(!PUBLIC_STAGENET_NODES.contains(node));
            assert!(
                node.ends_with(":18081") || node.ends_with(":18089"),
                "{node} is not on a port of the main network"
            );
        }
        for node in PUBLIC_STAGENET_NODES {
            assert!(
                node.ends_with(":38081") || node.ends_with(":38089"),
                "{node} is not on a port of stagenet"
            );
        }
    }

    #[test]
    fn shuffling_keeps_every_item() {
        let mut items: Vec<u32> = (0..50).collect();
        shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<u32>>());
        // Fifty items staying in place by chance is not going to happen.
        assert_ne!(items, sorted);
    }
}
