# Building & releasing AI Tumor Board

The distributable app is a *compiled snapshot* of this repo. Editing the app
always means: edit source → rebuild → redistribute. This doc is the repeatable
loop.

## Prerequisites (one time)

- **Rust** (stable) via [rustup](https://rustup.rs). macOS also needs Xcode
  Command Line Tools (`xcode-select --install`).
- **Node** 18+ and npm.
- macOS builds run on a Mac; Windows builds run on Windows. There is **no single
  binary** that runs on both — you produce one installer per OS. (The `.atb`/
  `.atbr` data files are cross-platform and open in either build.)
- First `cargo build` compiles SQLCipher + OpenSSL from source (a few minutes);
  subsequent builds are cached.

```bash
cd ai-tumor-board
npm install
rustup target add aarch64-apple-darwin x86_64-apple-darwin   # for universal macOS
```

## Develop (live)

```bash
npm run tauri dev
```

Edits to `src/` (React) hot-reload; edits to `src-tauri/` (Rust) recompile.

## Verify before shipping

```bash
npm run build                      # frontend typecheck + bundle
cd src-tauri && cargo test --lib   # backend tests (crypto, repo, import, export)
```

Both must be clean.

## Build installers

```bash
# macOS (universal — Intel + Apple Silicon)
npm run tauri build -- --target universal-apple-darwin

# macOS (host arch only, faster)
npm run tauri build

# Windows (run on Windows)
npm run tauri build
```

Artifacts land in `src-tauri/target/release/bundle/`:

- macOS: `macos/AI Tumor Board.app` and `dmg/AI Tumor Board_<version>_<arch>.dmg`
- Windows: `msi/*.msi` and `nsis/*-setup.exe`

## Signing & notarization (required for real distribution)

Unsigned builds trigger Gatekeeper (macOS) and SmartScreen (Windows) warnings and
may be blocked. Before distributing to physicians:

**macOS** — sign with a Developer ID cert and notarize. Set env vars, then build:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: <Name> (<TEAMID>)"
export APPLE_ID="<apple-id-email>"
export APPLE_PASSWORD="<app-specific-password>"
export APPLE_TEAM_ID="<TEAMID>"
npm run tauri build -- --target universal-apple-darwin
```

Tauri signs and (with the Apple credentials present) notarizes the bundle. See
Tauri's macOS distribution docs.

**Windows** — sign the `.exe`/`.msi` with an code-signing certificate (configure
`bundle.windows.certificateThumbprint` in `tauri.conf.json`, or sign the outputs
with `signtool`). EV certs get SmartScreen reputation fastest.

For institutional rollout, prefer managed software deployment (Jamf / Intune /
SCCM) over emailing installers.

## The compiled-in bits (change ⇒ rebuild)

- **Coordinator authority public key** (`crypto/role_credential.rs`,
  `AUTHORITY_PUBLIC_KEY_HEX`) — currently the all-zero placeholder (development
  mode, coordinator unrestricted). Provision the study's real Ed25519 public key
  before a production build to enforce the coordinator role gate.
- **KDF parameters / format version** (`crypto/mod.rs`, `crypto/key_derivation.rs`).
- **Code-signing identity** (build env).

Everything else — patients, assignments, responses, passwords — is runtime data
in `.atb`/`.atbr` files and needs no rebuild.

## Cutting a release

See `RELEASE_CHECKLIST.md`.
