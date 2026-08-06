# Toccata

A CD ripper that finishes the job in one pass.

Put an audio CD in the drive and Toccata reads it once, checks the result
against rips other people made of the same disc, finds the title, artist,
year, genre, cover and lyrics, and writes out every format you asked for.
No second pass, no separate tagger, no chasing cover art by hand.

Windows, Linux and macOS.

## What one pass gives you

- **A bit-perfect rip**, with bad sectors re-read one at a time before the
  ripper gives up on them, and a count of what it could not recover.
- **Verification against CTDB**, so you know whether your copy matches what
  other people got off the same pressing rather than merely hoping it does.
- **Every format at once.** Pick FLAC and MP3 and Ogg and they are all
  encoded while the disc is being read, not one after another.
- **Tags, cover art and lyrics** written into each of those files.
- **A CUE sheet and a rip log** next to the audio, and a `.lrc` file for any
  lyrics that carry timings, because that is where players look for them.

## Formats

| Format | What it is | What you can set |
| --- | --- | --- |
| FLAC | lossless, compressed — the default | compression level 0–12 |
| WAV | the samples with a header in front | nothing to set |
| AIFF | the same, in Apple's container | nothing to set |
| M4A (ALAC) | Apple Lossless | nothing to set |
| M4A (AAC) | lossy, in the MP4 container | bitrate |
| MP3 | lossy, via LAME | VBR quality 0–9 or fixed bitrate |
| AAC | lossy, as a raw stream | bitrate |
| Ogg Vorbis | lossy, via libvorbis | VBR quality 0–10 or fixed bitrate |
| APE | Monkey's Audio | compression level 0–5, `ape` build feature |

The lossless formats have nothing to configure on purpose: the samples are
written exactly as they came off the disc, and changing bit depth or sample
rate would only make the copy less faithful than the original.

M4A appears twice because it is a container, not a codec. `M4A (ALAC)` is
lossless and `M4A (AAC)` is not, and both end in `.m4a` — which is why
choosing several formats at once puts each one in its own folder.

APE is the one exception to "it just works": its only encoder lives in the
official Monkey's Audio SDK, which cannot be shipped with the source. Without
the `ape` feature the application builds and runs, just without that format.

## Install

Take the build for your system from
[Releases](https://github.com/MakotoPD/Toccata/releases) and run it. That is
the whole procedure — there is nothing else to install, no runtime to fetch,
no codec pack, nothing to put on PATH.

- **Windows** — run the `-setup.exe`. The FFmpeg libraries travel inside the
  installer, and the WebView2 runtime is fetched automatically on the rare
  machine that does not already have it. No Visual C++ redistributable.
- **Linux** — make the `.AppImage` executable and run it. It carries its own
  libraries.

If you want to work on Toccata rather than use it, see
[Building from source](#building-from-source) at the bottom.

## Using it

1. **Put a disc in.** Toccata notices, reads the table of contents and starts
   looking the disc up on its own. The first time a drive is used it also
   measures that drive's read offset against other people's rips, which takes
   one extra track read and never happens again for that drive.
2. **Check the release it found.** One Disc ID often matches several
   pressings, and Toccata will not guess between them — it shows you what each
   source returned, labelled with where it came from, and you pick. If nothing
   fits, search by artist and title, paste a MusicBrainz or Discogs link, or
   type everything in by hand.
3. **Choose your formats** in the Encoder tab, along with the quality settings
   for each of them.
4. **Rip.** Progress is per track. When it is done, the tracks are verified
   against CTDB and each one is marked as matching, not matching, or not
   present in the database.

Anything you correct by hand is remembered against that disc's ID, so putting
the same CD in again brings your version straight back.

## Where the files go

The output folder defaults to your system music folder and the layout is a
pattern you can change:

```
{albumartist}/{album}/{track} - {title}
```

Available placeholders: `albumartist`, `album`, `artist`, `title`, `track`,
`tracktotal`, `disc`, `disctotal`, `year`, `genre`, `label`, `catalog`.

Names are cleaned to what Windows accepts even on Linux and macOS, since a
music library tends to end up on a NAS or a shared drive sooner or later.

## Where the metadata comes from

Sources are tried in order, and the first one that answers wins. Every result
is labelled with its origin so you can see what you are trusting:

1. **Your own corrections**, if you have already fixed this disc once.
2. **MusicBrainz**, by Disc ID.
3. **CTDB**, which replicates MusicBrainz, Discogs and freedb and matches on a
   fuzzy table of contents, so it reaches discs an exact Disc ID misses.
4. **The barcode on the disc itself** — many CDs carry one in the subchannel —
   looked up on Discogs.
5. **Track identifiers (ISRC) from the disc**, looked up on MusicBrainz. This
   one rescues pressings nobody has catalogued whose recordings are known.

Cover art comes from the Cover Art Archive, Discogs, iTunes or Deezer, or from
a file you choose yourself. Lyrics come from LRCLIB, matched on track length.

Two optional tokens live in the settings, both blank by default:

- **Discogs** — raises the request limit and reveals fields anonymous callers
  do not see.
- **Last.fm** — a better source of genre than MusicBrainz, whose genre data is
  thin.

Everything works without either of them. Neither is sent anywhere except to
the service it belongs to, and they are kept in your own settings file.

## Verification and drive offset

Every drive reads a few samples early or late, and the amount is a property of
the drive model. If it is not corrected, a rip can look perfect and still match
nothing in any database. Toccata works the offset out for you by reading one
track and sliding it against known-good checksums until it lines up, then
remembers it for that drive by manufacturer and model — so the same drive stays
calibrated even if you move it to another computer or another operating system.
You can still set it by hand.

AccurateRip checksums are computed and shown locally. The AccurateRip database
itself is not queried; it belongs to Illustrate and requires their permission.
Online verification goes through CTDB.

## Platform notes

- **Linux** — reading a drive usually needs membership of the `cdrom` or
  `optical` group. If opening the device fails, the error says so and names the
  group.
- **macOS** — no download yet, only a build from source, because a `.app`
  built today expects Homebrew's FFmpeg to be present rather than carrying its
  own. Beyond that: the system mounts audio CDs by itself and holds on to the
  device, so unmount the disc with `diskutil unmountDisk` before ripping, and
  Apple Silicon machines have no built-in drive, so an external USB one is the
  only option.
- **Windows** — nothing special.

Development happens on Windows. The Linux and macOS drive layers are written
and build in CI on every change, but they have had far less use.

## Language

English and Polish, chosen from the settings or taken from your system.
Adding another language means dropping one JSON file into
`app/i18n/locales/` and changing no code.

## Known limits

- The last track of a disc is reported as "not in the database" rather than
  verified. CTDB trims a different amount off the end of the last track and the
  rule for it has not been worked out yet.
- GnuDB is deliberately absent: it only answers clients on a list it keeps, and
  its data reaches Toccata through CTDB anyway.
- CD-TEXT, which a few discs carry, is not read yet.

## Building from source

Only needed if you want to change Toccata. To use it, take a build from
[Releases](https://github.com/MakotoPD/Toccata/releases) instead — the
compiler and libraries below are for producing that build, not for running it.

| Tool | Version |
| --- | --- |
| Rust, stable, via [rustup](https://rustup.rs) | 1.94, minimum 1.85 for edition 2024 |
| [Node.js](https://nodejs.org) | 24 LTS |
| pnpm | pinned in `package.json`; `corepack enable` picks up the right one |

**Windows** — Microsoft C++ Build Tools with the *Desktop development with
C++* workload, and LLVM, which supplies the `libclang.dll` the FFmpeg bindings
are generated with:

```powershell
winget install --id LLVM.LLVM --exact
```

Windows has no package manager carrying FFmpeg development files, so fetch a
prebuilt one. It must carry `libmp3lame` and `libvorbis` and must not be
`nonfree`; the GPL builds at
[BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases) qualify,
and `ffmpeg-n8.1-latest-win64-gpl-shared-8.1` is what this was built against.
Unpack it outside the checkout and write a `.cargo/config.toml` at the
repository root, which Cargo reads no matter which shell the build starts in:

```toml
# Single quotes on purpose: a basic string reads every backslash as an escape.
[env]
FFMPEG_DIR = 'C:\path\to\ffmpeg-n8.1-latest-win64-gpl-shared-8.1'
LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
```

**macOS**:

```bash
xcode-select --install && brew install ffmpeg llvm pkg-config
```

**Linux**, Debian or Ubuntu:

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libavcodec-dev libavformat-dev libavutil-dev libswresample-dev libclang-dev pkg-config
```

FFmpeg is found through `pkg-config` there, so nothing needs setting by hand —
but a distribution shipping FFmpeg 6 or older will not do, since the bindings
are built against the 7 and 8 series.

Then:

```bash
pnpm install && pnpm tauri dev
```

`pnpm tauri build` produces the installers instead. To include APE, which
means you have read the Monkey's Audio SDK licence yourself and are satisfied
that distributing the result is allowed:

```bash
cargo build --features toccata-core/ape
```

The checks CI runs on every change:

```bash
pnpm lint && pnpm typecheck && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

The FFmpeg build used here is a GPL one, and `libcdio-paranoia` — also GPL — is
where the read path is headed, so a permissive license was never an option and
the choice was made up front rather than discovered later.

FFmpeg is linked, not vendored into this repository. An LGPL build stays LGPL
and a build with `--enable-gpl` is GPL; either is fine, since the application
is GPL regardless. What is not fine is `--enable-nonfree`, or `fdk-aac`, which
make the result non-redistributable. `libmp3lame` and `libvorbis` are required
— the first for MP3, the second because FFmpeg's own Vorbis encoder is audibly
worse than libvorbis.

HTTPS uses the platform TLS stack, so Linux builds link the system OpenSSL.
Windows and macOS use schannel and Security.framework and need nothing extra.

### Dependencies

| Dependency | License |
| --- | --- |
| tauri, tauri-build | Apache-2.0 OR MIT |
| tauri-plugin-dialog, @tauri-apps/plugin-dialog | Apache-2.0 OR MIT |
| tauri-plugin-opener, @tauri-apps/plugin-opener | Apache-2.0 OR MIT |
| @tauri-apps/api, @tauri-apps/cli | Apache-2.0 OR MIT |
| serde, serde_json | MIT OR Apache-2.0 |
| reqwest | MIT OR Apache-2.0 |
| roxmltree | MIT OR Apache-2.0 |
| rusqlite (with bundled SQLite) | MIT; SQLite is public domain |
| sha1 | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
| tokio | MIT |
| ffmpeg-next, ffmpeg-sys-next | WTFPL |
| lofty | MIT OR Apache-2.0 |
| FFmpeg itself (linked, not vendored) | GPL-3.0-or-later as built here |
| windows (Windows only) | Apache-2.0 OR MIT |
| libc (Linux and macOS only) | MIT OR Apache-2.0 |
| nuxt, vue | MIT |
| tailwindcss, @tailwindcss/vite | MIT |
| @nuxtjs/i18n, vue-i18n | MIT |
| @nuxt/eslint, eslint, eslint-config-prettier, prettier | MIT |
| vue-tsc, @types/node | MIT |
| typescript | Apache-2.0 |

Native libraries wired in later (`libcdio-paranoia`) are listed here once they
are.
