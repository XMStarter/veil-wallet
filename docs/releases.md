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

Released on 27 November 2026: the first release, for Android only, for
the Monero main network. There is no release for Windows or for Linux;
those apps are built from source.

| | |
|---|---|
| The page | <https://github.com/XMStarter/veil-wallet/releases/tag/v0.1.0> |
| The file | `veil-0.1.0.apk`, 29,375,052 bytes, for 64-bit ARM and Android 7 or later |
| The commit | `981c6a68d998e6926f311ea2f916c272465ee25d`, tagged `v0.1.0` |
| The signed package, SHA-256 | `3a03aee92cc67644a430ad748fa7da4cb3bb9ac46e65d55593fe6363c0e2ab55` |
| The package without its signature, SHA-256 | `623396eccc7056ffe01498a0cca968b6775c0796b7965b849f3e7059e3f4b47b` |
| The certificate it is signed with, SHA-256 | `82ab766ac1dfaf805df1aa5af37d4c869a64260a92474dee5076cc25e0b08fcd` |
| What Android knows it by | `io.github.xmstarter.veil`, version 0.1.0, number 1 |

What it is: a wallet that creates and restores (16 words, or a classic
25-word phrase), receives on a new address for every payment, and sends,
with every connection through its own Tor client and bridges where Tor
is blocked. What it is not: audited. Nobody outside the project has
looked at it, and the people who know licences have not looked at how
the package hands on the work of others it contains
([concept.md](concept.md), "Decided for the release").

Known to be weak, and not put right: [audit.md](audit.md), "Known to be
weak".

How it was made, step by step as [development.md](development.md),
"Making a release", has it:

1. **The commit** is the merge of the pull request that brought the
   second build by hosted CI, on `main`. Hosted CI had passed on that
   pull request on Linux and on Windows.
2. **The owner's run** passed at the `offline`, `regtest` and `tor`
   levels as Linux and as Windows programs. It was made on commit
   `4da75aa`, which is the release's commit with the project's notes
   (`CLAUDE.md`) changed and no other file. At the `tor` level all five
   public nodes of the main network answered both times, each time at
   one height.
3. **The package was built twice**, by a cloud session of the project's
   and by a runner of GitHub's
   ([the run](https://github.com/XMStarter/veil-wallet/actions/runs/45977386949)),
   and the two unsigned packages are the same file: all 80 files in
   them, and the checksum above. Nobody else has built it.
4. **The signature** was made on the owner's machine with
   `release.sh sign`, which kept its result only after looking at it.
5. **The signed package was held against the unsigned one** in a cloud
   session, with `release.sh compare`: the same but for the signature,
   and signed by the key whose certificate is recorded.
6. **On a phone**, a Samsung Galaxy A17, the owner reported that
   everything works and that bridges do. What was asked for was that it
   is installed over the signed package that was there, of an earlier
   commit and the same key, and that the wallet opens, synchronises,
   sends a payment, and that Tor connects through obfs4 or Snowflake.
   Which of these were tried one by one was not said.
7. **Published** by the owner on the repository's releases page. The
   file there has the checksum of the signed package, and the tag
   stands on the commit.

One part of the package is newer than the rest of what was tried
before it: the transport program is built without two marks Go used to
write into it, a change made for this release so that two machines
build the same file. The file in the package is therefore not the one
the packages for trying carried. It is the same code, and the owner's
word about bridges on the phone is about this file.
