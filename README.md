# Toccata

Desktop CD Audio ripper for Windows, Linux and macOS.

One pass over the disc gives a bit-perfect rip with verification, complete
metadata, cover art, genre, correct disc numbers for multi-disc releases and
song lyrics. Output formats are selectable and can be combined — the disc is
read once and encoded to every selected format in parallel.

Planned output formats: FLAC, WAV, AIFF, ALAC, APE, MP3, AAC, Ogg Vorbis and
M4A (both AAC and ALAC variants).

## Status

Early development. The application reads the table of contents of an audio CD
on all three platforms, computes the MusicBrainz and FreeDB disc identifiers,
and identifies it through MusicBrainz, CUETools DB and Discogs, with cover art,
search by name or barcode, and hand-entered metadata remembered per disc. Audio comes off
the disc as WAV, with drive offset correction. Encoding and verification are
not written yet.

```bash
pnpm install && pnpm tauri dev
```

Prerequisites and release builds are described in [docs/BUILD.md](docs/BUILD.md).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

The application links `libcdio-paranoia`, which is GPL, so the project as a
whole is GPL. This rules out a permissive license.

### Dependencies

| Dependency | License |
| --- | --- |
| tauri, tauri-build | Apache-2.0 OR MIT |
| serde, serde_json | MIT OR Apache-2.0 |
| reqwest | MIT OR Apache-2.0 |
| roxmltree | MIT OR Apache-2.0 |
| tauri-plugin-opener, @tauri-apps/plugin-opener | Apache-2.0 OR MIT |
| sha1 | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
| ffmpeg-next, ffmpeg-sys-next | WTFPL |
| lofty | MIT OR Apache-2.0 |
| FFmpeg itself (linked, not vendored) | GPL-3.0-or-later as built here |
| windows (Windows only) | Apache-2.0 OR MIT |
| libc (Linux and macOS only) | MIT OR Apache-2.0 |
| tokio | MIT |
| nuxt, vue | MIT |
| tailwindcss, @tailwindcss/vite | MIT |
| @nuxtjs/i18n, vue-i18n | MIT |
| @tauri-apps/api, @tauri-apps/cli | Apache-2.0 OR MIT |
| @nuxt/eslint, eslint, eslint-config-prettier, prettier | MIT |
| vue-tsc | MIT |
| typescript | Apache-2.0 |

HTTPS goes through the platform TLS stack, so Linux builds link the system
OpenSSL. Windows and macOS use schannel and Security.framework and need
nothing extra.

FFmpeg is linked, not bundled into this repository. Its license depends on how
it was configured: an LGPL build stays LGPL, and a build with `--enable-gpl` is
GPL. Either is fine here, since the application is GPL regardless. What is not
fine is `--enable-nonfree`, which makes the result non-redistributable; a build
carrying it, or `fdk-aac`, must not be used. `libmp3lame` and `libvorbis` are
required, the first for MP3 and the second because FFmpeg's own Vorbis encoder
is audibly worse.

Native libraries linked later in development (`libcdio-paranoia`, FFmpeg) are
listed here once they are wired in.
