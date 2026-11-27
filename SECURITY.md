# Security

Veil holds money. If you find a way to take it, to learn whose it is, or to
make the wallet lie to its user, please tell us before you tell anybody
else.

## How to report

Use GitHub's private report for this repository: on the
[Security tab](https://github.com/XMStarter/veil-wallet/security), "Report
a vulnerability". Only you and the maintainer can read it.

Do not open a public issue for something that can cost a user money or
their privacy. A public issue is the right place for everything else.

It helps to say:

- which release or commit, on which system;
- what an attacker has to be able to do first: be on the network, run a
  program on the device, hold the device, run a node the wallet talks to;
- what happens, and how to see it happen.

Never send a recovery phrase, a wallet file or a password, yours or
anybody's. No report needs one.

## What to expect

Veil is maintained by one person. A report is read and answered, a fix is
made where one is possible, and the reporter is named in the release that
carries it unless they would rather not be. There is no bounty.

## What counts

Anything in Veil's own code, in the patches it applies to the Monero wallet
library (`native/wallet_lib/patches`), and in the way a release is built
and signed. [docs/audit.md](docs/audit.md) says what Veil promises, whom it
defends against and whom it does not, and lists what is already known to be
weak; a report about something on that list is still welcome when it shows
the weakness to be worse than written.

A fault in Monero itself, in Arti or in the Tor network belongs to those
projects, and we would like to know of it as well when it reaches Veil's
users.

## What a release is known by

A release for Android is signed with one key. The checksum of its
certificate is in [native/release-key.sha256](native/release-key.sha256),
and `native/release.sh who <file>` says who signed a package.
[docs/development.md](docs/development.md), "Signed packages", says how a
download is compared with a build of one's own. Nobody outside the project
has audited Veil.
