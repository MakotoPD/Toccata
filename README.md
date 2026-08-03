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
and identifies it through MusicBrainz and CUETools DB. Nothing is ripped yet.

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
| sha1 | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
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

Native libraries linked later in development (`libcdio-paranoia`, FFmpeg) are
listed here once they are wired in.
