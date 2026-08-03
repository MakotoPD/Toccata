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

### macOS

```bash
xcode-select --install
```

Xcode Command Line Tools are enough for a desktop-only build. WebKit comes
with the system.

### Linux

Debian / Ubuntu:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Arch:

```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
```

Fedora:

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel
sudo dnf group install "c-development"
```

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

## Checks

The same set CI runs:

```bash
pnpm lint && pnpm typecheck && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

## Not wired up yet

**Native libraries.** `libcdio`, `libcdio-paranoia` and FFmpeg are not linked
yet — `crates/cdio-sys` is an empty stub on purpose, so that a machine without
those headers can still build the whole workspace. Package names, bindgen
header paths, how the libraries get into the Tauri bundle, and the macOS
`@rpath` / `install_name_tool` fixups are documented here once the FFI layer
lands.

**Code signing.** Both are deferred, and both need credentials that cannot
live in this repository:

- macOS: an Apple Developer ID Application certificate, plus notarization
  through `notarytool` with an app-specific password or an App Store Connect
  API key. Without it, Gatekeeper blocks the `.dmg` on other machines.
- Windows: an Authenticode certificate (an EV or OV certificate from a CA);
  unsigned installers trigger SmartScreen warnings.
