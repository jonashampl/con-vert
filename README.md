<p align="center">
  <img src="app/src-tauri/icons/128x128.png" width="96" height="96" alt="con/vert icon">
</p>

<h1 align="center">con/vert</h1>

<p align="center">
  <b>Drag a file, tap Shift, done.</b><br>
  Convert audio, read a loop's tempo and key, and convert or edit images and PDFs,<br>
  right on your Windows desktop. Free and offline.
</p>

<p align="center">
  <a href="https://github.com/jonashampl/con-vert/releases/latest"><b>Download for Windows</b></a> ·
  by <b>Jonas J. Hampl</b> · Instagram <a href="https://instagram.com/1onazz">@1onazz</a>
</p>

## What it does

Drag a file and a ring of actions opens around your cursor. Every result is saved as a
new file next to the original, so the original is never changed.

**Audio and video**

- **Convert** to MP3, WAV, FLAC, M4A, OGG, OPUS or AAC (video files give their sound).
- **BPM + Key** saves a copy named with the tempo and key, for example
  `Loop 146 BPM Dphy.wav`. Tuned for the scales producers use most: major, minor and
  Phrygian.
- **Compress** makes a smaller file in the same format (WAV becomes FLAC, lossless).
- **Cover** saves a track's album art as a JPG.
- **Zip** packs several files into one archive.

**Images and PDFs** (from File Beaver)

Convert between JPG, PNG, WEBP, HEIC, TIFF, AVIF, BMP, PDF and DOCX, and compress, crop,
edit, annotate, redact, add a background or strip metadata.

## Install

1. Download `con-vert_x.y.z_x64-setup.exe` from the
   [latest release](https://github.com/jonashampl/con-vert/releases/latest).
2. Open it. It installs in a few seconds, without admin rights.
3. A glass icon appears on your desktop and con/vert starts with Windows.

> [!NOTE]
> **Seeing "Windows protected your PC"?** Click **More info**, then **Run anyway**.
> Windows shows this for apps that aren't code-signed. All the code is in this
> repository for anyone to check.

## How to use it

- **Shift:** start dragging a file in File Explorer or on the desktop and tap
  <kbd>Shift</kbd>. The ring opens under the cursor; drop on an action.
- **The glass icon:** drag a file from anywhere (Explorer, a browser, FL Studio…) onto the
  icon. Let go on the icon itself and the ring waits for a click.
- **Second page:** tap <kbd>Shift</kbd> again or click the middle of the ring to switch
  between Convert and the tools.
- **Cancel:** drop in the middle, drag away from the ring or press <kbd>Esc</kbd>.
- **Settings** are in the tray icon by the clock. If the icon ever gets stuck,
  **Reset icon** there puts it back.

## Is it safe?

- **Offline.** Everything runs on your PC: no account, no upload, no tracking, no
  automatic updates.
- **Originals are never touched.** Results are always new files.
- **No admin rights.** It installs for your Windows account only.
- **Crash reports stay local** in `%LOCALAPPDATA%\app.convert.desktop\crashes`.
- **Clean uninstall** from **Settings › Apps › Installed apps**.

See [SECURITY.md](SECURITY.md) to report a problem.

## Build from source

You'll need Rust (GNU toolchain) with MinGW and nasm, Node.js 20 or later, and an
`ffmpeg.exe` in `app/src-tauri/binaries/` (not committed). [RELEASING.md](RELEASING.md) has
the exact steps.

```sh
cd core && cargo test
cd app && npm install && npx tauri build --debug --no-bundle
```

## Credits

con/vert is made by **Jonas J. Hampl** ([@1onazz](https://instagram.com/1onazz)).

It is built on [File Beaver](https://github.com/DEVIOUSDEV101/file-beaver) by
**DEVIOUSDEV101**, who created it and shared it as open source. Thank you! Audio is handled
by [FFmpeg](https://ffmpeg.org) (LGPL), key detection uses Spotify's
[Basic Pitch](https://github.com/spotify/basic-pitch) model (Apache-2.0), and icons are by
[Lucide](https://lucide.dev). Every component's licence is in
[THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt).

## Licence

con/vert is **free**: use it for anything, including paid work, and copy, change and share
it, but **don't sell it** or a changed version of it. That's the MIT License with the
Commons Clause; File Beaver's own code stays under its original MIT License. Full terms in
[LICENSE](LICENSE).

© 2026 Jonas J. Hampl
