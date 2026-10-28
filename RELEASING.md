# Releasing con/vert

How to cut a release. Everything here runs on Windows from the repo root unless noted.

## One-time setup

| What | Where | Notes |
|---|---|---|
| Rust (GNU), MinGW, nasm | `winget install Rustlang.Rustup BrechtSanders.WinLibs.POSIX.UCRT` | WinLibs includes nasm; put its `mingw64\bin` on PATH. |
| Node 20+ | already installed | `npm install` in `app/`. |
| FFmpeg (LGPL) | `app/src-tauri/binaries/ffmpeg-x86_64-pc-windows-gnu.exe` | Not in git. `bin/ffmpeg.exe` from BtbN's `ffmpeg-n9.0-latest-win64-lgpl-9.0.zip` (github.com/BtbN/FFmpeg-Builds, release 2026-10-27, SHA-256 `bac7c98811b44cbf3536f2fd8438851446f42b56e379125648212c9e98e3dc07`). Its licence is `licensing/ffmpeg-LICENSE.txt`. Core tests use it via `CONVERT_FFMPEG`. |
| `cargo-about` | `cargo install cargo-about --locked --features cli` | Builds the licence notices. |
| Updates | off | `plugins.updater.endpoints` is empty and `createUpdaterArtifacts` false: no updater key needed. To turn updates on, see File Beaver's original steps in git history (`89363cf`). |
| Code signing | `app/src-tauri/sign.ps1` | Optional but recommended; see below. |

## Cutting a release

1. **Version:** set the same version in `app/package.json`, `app/src-tauri/Cargo.toml`
   and `app/src-tauri/tauri.conf.json`.
2. **Tests:** `cargo test` in `core/`; run the e2e drags (`spike/` harnesses) if anything
   user-facing changed.
3. **Licence notices** (after any dependency change):
   ```
   cd licensing
   cargo about generate --config about.toml --manifest-path ../app/src-tauri/Cargo.toml about.hbs -o rust-notices.txt
   node build-notices.mjs
   ```
   A licence not on the allow-list (`about.toml`) fails here, on purpose.
4. **Build** (in `app/`, PowerShell):
   ```
   $env:SHIFTDROP_SIGN = "cert"   # or "azure"
   npx tauri build
   ```
   Without a certificate, skip the signing step (otherwise the bundler stops at
   "SignTool not found"):
   ```
   npx tauri build --config '{"bundle":{"windows":{"signCommand":null}}}'
   ```
   Output: `app/src-tauri/target/release/bundle/nsis/con-vert_<version>_x64-setup.exe`.
5. **Share** that one file. Unsigned, Windows SmartScreen shows "Windows protected your PC":
   More info > Run anyway.

## Code signing options (checked October 2026)

None of these removes the SmartScreen warning on day one: since 2024 every certificate
(OV and EV alike) builds reputation as people download the app. Signing still replaces
"Unknown publisher" with a name and is what that reputation attaches to.

| Option | Cost | Who can use it | `sign.ps1` mode |
|---|---|---|---|
| Azure Artifact Signing | ~$9.99/month | Organisations in US/CA/EU/UK; **individuals only in US/Canada** | `azure` |
| OV certificate (Sectigo, DigiCert, ...) | ~$220-440/year | Individuals or companies; key must be on a USB token or cloud HSM; max ~15-month validity | `cert` |
| SignPath Foundation | Free | Fully open-source projects (OSI licence) with automated public builds; publisher shown is "SignPath Foundation" | signs in CI, not here |

**`cert` mode:** install the certificate (token driver / cloud HSM client) so it appears in
the Windows certificate store, then set `SHIFTDROP_CERT_THUMBPRINT`. Needs `signtool.exe`
from the Windows SDK.

**`azure` mode:** install the Azure Artifact Signing dlib (`Microsoft.ArtifactSigning.Client`
/ "Trusted Signing Client Tools"), create a `metadata.json` with your endpoint, account and
certificate profile, sign in with `az login`, then set `SHIFTDROP_AZURE_DLIB` (path to the
dlib DLL) and `SHIFTDROP_AZURE_METADATA` (path to metadata.json).

## What the installer does

- Per-user install (`%LOCALAPPDATA%\con-vert`), no admin prompt; installs WebView2 if
  missing (silent bootstrapper).
- Ships `THIRD_PARTY_NOTICES.txt` next to the app.
- Uninstall removes the "launch at startup" entry (`installer-hooks.nsh`). Logs, settings
  and crash reports in `%LOCALAPPDATA%\app.convert.desktop` and `%APPDATA%\app.convert.desktop`
  are kept unless the user ticks "delete app data".
