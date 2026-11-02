# Changelog

All notable changes to con/vert. Releases before con/vert are File Beaver's, the project
con/vert is built on.

## 1.0.0 (2 November 2026)

First public release of con/vert by Jonas J. Hampl, built on File Beaver 0.1.1.

- Audio and video: convert to MP3, WAV, FLAC, M4A, OGG, OPUS and AAC; BPM + Key naming
  (major, minor and Phrygian); Compress; Cover art; Zip.
- Glass desktop icon, Reset icon in Settings and the tray.
- Shift opens the ring only for drags that start in File Explorer or on the desktop, so it
  never gets in the way in other apps (FL Studio and the like); their files can go onto
  the icon.
- Every file opens the ring on its conversions; Shift or the middle switches to the tools.
- Licence: free, not for resale (MIT with the Commons Clause).

# File Beaver

## 0.1.1 (26 October 2026)

- Fixed: a "File Beaver closed unexpectedly" message appeared after restarting Windows,
  after an update and after reinstalling, even though nothing had crashed. It now only
  appears after a real crash.

## 0.1.0 (26 October 2026)

First public release.

- The beaver: a small widget on your desktop. Drag a file onto him to open the ring of
  actions; let go on him to choose with a click. He stays behind your windows and comes
  forward when you drag a file.
- Shift: start dragging a file anywhere and tap Shift to open the same ring under the
  cursor.
- Convert between JPG, PNG, WEBP, HEIC, TIFF, AVIF, BMP and PDF, and save PDFs as DOCX.
- Compress, Crop, Add background, Annotate, Redact, Edit and remove metadata.
- Signed automatic updates, per-user install without admin rights, local-only crash
  reports.
