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
same byte for byte. Hosted CI's build of 27 November 2026 was the same
but for sixty bytes: two marks Go writes into a program, the first made
from the command the C compiler is called by, which is a path under the
home of whoever builds, and the second from the first. The program is
built without them now (`-buildid=`), and comes out the same with the
compiler under two different paths. Nothing on a phone reads either.

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

The owner has decided that lyrebird stays in the package for Android
and is handed on under its own terms (`docs/concept.md`, "Decided for
the release"). So its licence texts travel with the package, and the
package says where the source of exactly this build is: the commit of
this repository it was built from, whose `build.sh` pins lyrebird's
commit and builds it. `notices.sh` collects the texts into
`app/assets/licences/transport.txt`: lyrebird's two, Go's own, and those
of the Go packages the program says are in it (`go version -m`), read
from the source Go fetched for the build. The app for Android shows
them (Settings, About Veil). `../notices.sh --check` says whether the
file is still what the source says, on a machine where the program was
built. Nobody who knows licences has looked at this; nothing here is
legal advice.

## What a phone has shown

On one phone, the owner's, Tor (Arti) started the program from the app's
package and connected through obfs4 bridges and through a Snowflake
bridge. Nothing built for Android runs in a cloud session, so that phone
is all that is known.

## Not known yet

- WebTunnel, which nobody has had a bridge to try with.
- A network that blocks Tor: the phone's does not, so what was seen is
  that a bridged connection works, not that it gets through a block.
