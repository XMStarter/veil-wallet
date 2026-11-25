# The transport program for Android

Tor reaches its network through bridges where it is blocked, and a bridge
speaks a transport: obfs4, Snowflake or WebTunnel. Tor does not speak
them itself. It starts a program that does and talks to it over the
machine's own loopback. On a desktop Veil uses the programs installed
there (`veil_core::transports` looks for them). A phone has none and lets
an app start only what came in the app's own package, so the app for
Android carries one.

It is **lyrebird**, the Tor Project's program for obfs4, Snowflake,
WebTunnel and meek, the one Tor Browser carries. `build.sh` builds it for
Android on 64-bit ARM from source:

```bash
./build.sh            # into ~/veil-transports
```

`../check.sh --android setup` runs it, and `--android app` runs it if it
has not been run, then puts the result into the package.

## What is pinned

| What | Pinned by |
|---|---|
| lyrebird's source | the commit of its release, by hash, fetched from the Tor Project |
| The Go packages it needs | the checksums in lyrebird's own `go.sum`; a package that differs stops the build |
| Go, the compiler | its release, by the checksum its publisher gives; unpacked inside the work folder |
| The C compiler | the Android NDK that `check.sh` pins |

Built twice in a cloud session, in different folders, it came out the
same byte for byte. Nobody has compared it with a build on another
machine.

## Why it is built the way it is

- **With the NDK's C compiler.** On Android a Go program built without
  one cannot look up names on the network: Go's own way reads a file
  Android does not have. obfs4 bridges are given as numbers; Snowflake
  and WebTunnel name servers.
- **`-checklinkname=0`.** One of Snowflake's packages (`anet`) lists the
  phone's network interfaces in a way that works on Android 11 and
  later, where Go's own no longer does, and reaches into Go's `net`
  package by name for it. Go's linker refuses that unless told to allow
  it. Without the flag the build ends with `invalid reference to
  net.zoneCache`.
- **Named `liblyrebird.so`.** It is a program, not a library. Android
  unpacks only files named like libraries from a package, into the one
  folder whose files an app may start. The app's build is told to have
  them unpacked (`useLegacyPackaging`) and to leave this file as it is.

## Licence

lyrebird is free software under the GNU General Public License, version
3 or later: most of it is under a BSD licence, and part of its obfs4,
with which the whole is combined, is under the GPL (`LICENSE` and
`LICENSE-GPL3.txt` in its source; `build.sh` copies both next to the
program). Veil is under the MIT licence. The two are separate programs:
Tor starts lyrebird as a process of its own and talks to it over a
socket, and nothing of lyrebird is compiled into Veil's code.

What follows from that for whoever hands the app on with lyrebird inside
is theirs to settle before a release: at the least the licence texts
have to travel with the package, and the recipient has to be told where
the source of exactly this build is, which is the commit pinned in
`build.sh` together with that script. It is listed among the open
decisions in `docs/concept.md`. Nothing here is legal advice.

## Not known yet

- Whether Tor (Arti) can start the program on a phone at all, and
  whether a bridge then carries a connection. Nothing built for Android
  runs in a cloud session.
- Snowflake on a phone, which needs more of the network than obfs4.
