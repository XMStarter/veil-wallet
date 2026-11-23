# veil_cli

`veil`: a command-line wallet for trying Veil's core on the Monero stagenet.
Stagenet is a public test network whose coins have no value. The tool does
not work on any other network, and nothing here is meant for real funds.

It is a thin shell over [`veil_core`](../veil_core). Every command that
needs the network starts the embedded Tor client and talks to a stagenet
node that is an onion service. The other commands open the wallet with no
node set, so they cannot reach one, and they do not start Tor.

## Commands

```bash
veil create                    # a new wallet; shows the recovery phrase once
veil restore [--height N]      # from a 16- or 25-word phrase
veil address [--new]           # the address to receive on
veil seed                      # the recovery phrase
veil balance                   # as of the last sync
veil history                   # as of the last sync
veil sync                      # synchronise through Tor
veil send <address> <amount>   # synchronise, show the fee, ask, then send
```

`veil send <address> all` sends the whole balance. `veil --help` lists the
options.

The wallet and Tor's files live in one directory: `.veil/stagenet` in the
home directory, or `--data-dir`. The wallet library is taken from
`--wallet-lib`, then `VEIL_WALLET_LIB`, then next to the program, then
`~/veil-wallet-lib/out`, where `../check.sh setup` builds it.

From a checkout:

```bash
cargo run -p veil_cli -- create
```

## Nodes

Without `--node`, the tool tries the public stagenet onion nodes listed in
[`src/nodes.rs`](src/nodes.rs) in random order, synchronises with the first
one that answers, and sends through the next. If that node stops answering
in the middle of a synchronisation, the next one continues it. That list comes from a public
directory; nobody vetted it, and most of it belongs to one operator. It is
there so that the tool works out of the box, and it is not the list the app
will ship.

`--node host.onion:port` names a node; give it twice to send through a
different node than the one synchronised with. Only named nodes are used
then.

If the wallet's data file cannot be read, the tool says so, opens the wallet
from its keys, and keeps the unreadable file next to it.

If the connection is lost while a transaction is being sent, the tool says
that it may have gone out. Run `veil sync` and look at `veil history`
before sending again.

## Bridges

On a network that blocks Tor:

```bash
veil sync --bridges bridges.txt
```

`bridges.txt` holds bridge lines, one per line, as given out at
<https://bridges.torproject.org> or by Tor Browser. The tool looks for an
installed program for the transports they use (`lyrebird` or `obfs4proxy`
for obfs4, `snowflake-client` for Snowflake); `--transport obfs4=/full/path`
names one instead. The setting is not remembered: give it with every
`sync` and `send`.

## Trying a send

A send needs coins. Stagenet faucets are linked from
<https://docs.getmonero.org/infrastructure/networks/>. Give one the address
from `veil address`, wait for about ten blocks (twenty minutes), run
`veil sync`, and then send some of it, for example to a second address of
the same wallet from `veil address --new`.

## Tests

| What | Needs | Run with |
|---|---|---|
| The command line; create, restore, addresses, the password, no overwriting; no Tor without a network command | The wallet library for all but the command line | `../check.sh offline` |
| A new wallet synchronises on stagenet through Tor, and the result is saved | The wallet library and the Tor network | `../check.sh stagenet` |
| A new wallet synchronises through obfs4 bridges | The same, and an obfs4 program | `../check.sh bridges` |

The last two run on a machine you control and never in hosted CI.

## Not done yet

- A send is tried by hand; there is no funded test wallet. It has been
  tried once: coins from a faucet arrived, part of them went to a second
  address of the same wallet through a different node than the one
  synchronised with, and the transaction was in the next block.
- Remembering bridge settings between runs.
