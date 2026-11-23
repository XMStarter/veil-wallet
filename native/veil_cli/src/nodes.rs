//! Which stagenet nodes to try, and in what order.

use veil_core::OnionNode;

/// Public stagenet nodes that are onion services, from the public list at
/// monero.fail. They are here so that the tool works without setting
/// anything up. Nobody vetted them, three of the four appear to belong to
/// one operator, and the app's own node list is a separate decision.
const PUBLIC_STAGENET_NODES: &[&str] = &[
    "plowsof3t5hogddwabaeiyrno25efmzfxyro2vligremt7sxpsclfaid.onion:38089",
    "plowsofe6cleftfmk2raiw5h2x66atrik3nja4bfd3zrfa2hdlgworad.onion:38089",
    "plowsoffjexmxalw73tkjmf422gq6575fc7vicuu4javzn2ynnte6tyd.onion:38089",
    "ykqlrp7lumcik3ubzz3nfsahkbplfgqshavmgbxb4fauexqzat6idjad.onion:38089",
];

/// The nodes to try, in the order to try them: the ones the user named, as
/// named, or else the public ones in an order nobody can predict.
pub fn candidates(named: Vec<OnionNode>) -> Vec<OnionNode> {
    if !named.is_empty() {
        return named;
    }
    let mut nodes: Vec<OnionNode> = PUBLIC_STAGENET_NODES
        .iter()
        .map(|text| text.parse().expect("the built-in node list is valid"))
        .collect();
    shuffle(&mut nodes);
    nodes
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
        assert_eq!(candidates(named.clone()), named);
    }

    #[test]
    fn the_public_list_is_complete_whatever_the_order() {
        let mut got: Vec<String> = candidates(Vec::new())
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
