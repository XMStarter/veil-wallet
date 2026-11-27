# Releases

What has been released, from which commit, and with which checksums. How
a release is made is in [development.md](development.md), "Making a
release"; how a download is checked is there under "Signed packages" and
in the [README](../README.md), "Download".

Every release for Android is signed with the key whose certificate has
the SHA-256 checksum in
[native/release-key.sha256](../native/release-key.sha256).

A release's page on GitHub carries the same numbers as its entry here.
The entry is written after the release, since a release cannot name its
own checksums: they follow from the commit it is built from.

## 0.1.0

Not released yet. The first release, for Android only, for the Monero
main network. There is no release for Windows or for Linux; those apps
are built from source.

What it is: a wallet that creates and restores (16 words, or a classic
25-word phrase), receives on a new address for every payment, and sends,
with every connection through its own Tor client and bridges where Tor
is blocked. What it is not: audited. Nobody outside the project has
looked at it, and the people who know licences have not looked at how
the package hands on the work of others it contains
([concept.md](concept.md), "Decided for the release").

Known to be weak, and not put right: [audit.md](audit.md), "Known to be
weak".
