# Building Toccata

Steps below are what a clean machine needs, per platform. Everything was
verified against the toolchain versions listed at the end.

The repository is a single pnpm workspace (`app/` is the only JS package) plus
a Cargo workspace (`src-tauri/`, `crates/core`, `crates/cdio-sys`). All
commands are run from the repository root.

## Shared toolchain

| Tool | Version used |
| --- | --- |
| Rust (stable, via rustup) | 1.94.1, minimum 1.85 (edition 2024) |
| Node.js | 24 LTS |
| pnpm | 10.33.0 — pinned by `packageManager` in `package.json` |

Install Rust from <https://rustup.rs> and Node from <https://nodejs.org>.
Enable pnpm with `corepack enable`, which picks up the pinned version.

## Platform prerequisites

### Windows

- **Microsoft C++ Build Tools** with the *Desktop development with C++*
  workload — provides the MSVC linker Rust needs.
- **Microsoft Edge WebView2 Runtime** — preinstalled on Windows 10 1803 and
  later. Verify with `pnpm tauri info`.
- **LLVM** — `ffmpeg-sys-next` generates its bindings with bindgen, which needs
  `libclang.dll`.

  ```powershell
  winget install --id LLVM.LLVM --exact
  ```

- **FFmpeg 8.x, shared build with headers.** Windows has no package manager
  that ships FFmpeg development files, so fetch a prebuilt one. The build must
  carry `libmp3lame` and `libvorbis` and must not be `nonfree`; the GPL builds
  from <https://github.com/BtbN/FFmpeg-Builds/releases> satisfy both, and
  `ffmpeg-n8.1-latest-win64-gpl-shared-8.1.zip` is the one this was verified
  against. Unpack it anywhere outside the repository and point the build at it:

  ```powershell
  $env:FFMPEG_DIR = "C:\path\to\ffmpeg-n8.1-latest-win64-gpl-shared-8.1"
  $env:PATH = "$env:FFMPEG_DIR\bin;$env:PATH"
  ```

  `FFMPEG_DIR` is read while compiling; `PATH` is what lets the resulting
  binary and the test suite find the DLLs at run time. Set both permanently
  through *System Properties → Environment Variables* to avoid repeating this
  per shell.

### macOS

```bash
xcode-select --install
brew install ffmpeg llvm pkg-config
```

Xcode Command Line Tools are enough for a desktop-only build. WebKit comes
with the system. Homebrew's `ffmpeg` is built with `libmp3lame` and
`libvorbis` and without `nonfree`, which is what this project needs; `llvm`
supplies the `libclang` bindgen wants, as the one inside Xcode is not always
found.

### Linux

Debian / Ubuntu:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libavcodec-dev libavformat-dev libavutil-dev libswresample-dev \
  libclang-dev pkg-config
```

Arch:

```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool ffmpeg clang \
  pkgconf
```

Fedora:

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel \
  ffmpeg-free-devel clang-devel pkgconf-pkg-config
sudo dnf group install "c-development"
```

On Linux the FFmpeg libraries are found through `pkg-config`, so nothing has
to be set by hand. A distribution that ships FFmpeg 6 or older will not do:
`ffmpeg-next` 9 is built against the 7 and 8 series.

## Build

```bash
pnpm install
```

Development — starts the Nuxt dev server and the Tauri window against it:

```bash
pnpm tauri dev
```

Release bundle:

```bash
pnpm tauri build
```

Bundle targets come from `bundle.targets: "all"` in
`src-tauri/tauri.conf.json`, so each platform produces what it can: NSIS and
MSI on Windows, AppImage, deb and rpm on Linux, `.app` and `.dmg` on macOS.

## Optional: APE

Monkey's Audio is off by default. FFmpeg decodes APE but does not encode it,
and the only encoder is the one in the official SDK, whose licence has
historically carried terms that are not GPL compatible. It is therefore not
vendored here and not fetched by the build.

Turning it on means you have read the SDK's current licence yourself and are
satisfied that distributing the result is allowed:

```bash
cargo build --features toccata-core/ape
```

Without the feature everything builds and runs as normal, one format short.

## Checks

The same set CI runs:

```bash
pnpm lint && pnpm typecheck && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

## Not wired up yet

**Native libraries.** `libcdio` and `libcdio-paranoia` are not linked yet —
`crates/cdio-sys` is an empty stub on purpose, and the drive is read through
each system's own interface in the meantime. FFmpeg *is* linked, which is why
it appears under the prerequisites above.

**Shipping FFmpeg with the bundle.** The prerequisites cover building. A
release still has to carry the libraries it links against: the Windows DLLs
next to the executable, and the macOS `@rpath` / `install_name_tool` fixups
for the dylibs. Neither is wired into `tauri.conf.json` yet, so a bundle built
today runs only where FFmpeg is already installed.

**Code signing.** Both are deferred, and both need credentials that cannot
live in this repository:

- macOS: an Apple Developer ID Application certificate, plus notarization
  through `notarytool` with an app-specific password or an App Store Connect
  API key. Without it, Gatekeeper blocks the `.dmg` on other machines.
- Windows: an Authenticode certificate (an EV or OV certificate from a CA);
  unsigned installers trigger SmartScreen warnings.
