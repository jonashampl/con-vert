# Changelog

All notable changes to con/vert. Releases before con/vert are File Beaver's, the project
con/vert is built on.

## 1.0.4 (3 November 2026)

- Key detection is gone. Measured on 1757 files it was right only about 60% of the time,
  and a wrong key in a file name does more harm than none. Converted files are named with
  their tempo only ("Song 124 BPM.mp3"); the "BPM + Key" tool is now "BPM". Keys you wrote
  in your own file names are left as they are.
- The installer is smaller: the note model used for keys is no longer bundled.

## 1.0.3 (3 November 2026)

- Key detection retuned on 1757 files with known keys: 837 songs from ten genres (GTZAN),
  604 EDM tracks (GiantSteps), 283 kit loops and 33 producer beats. Keys right 56.0% ->
  59.8% overall: songs 58.5% -> 63.1%, EDM 52.3% -> 58.3%. Phrygian is named far more
  carefully: false "phy" calls 124 -> 49. The 808-overtone change from 1.0.2 is undone
  (it cost songs and EDM more than it gained beats).

## 1.0.2 (2 November 2026)

- Key detection: a distorted 808's overtones no longer read as a major third, and minor
  is favoured more. On a producer's own 33 beats 58% -> 67% keys right, on 262 kit loops
  53% -> 55%; minor beats named major dropped from 16% to 6%.
- BPM + Key always listens, even when the name already states a tempo and key. If they're
  wrong, the copy gets them corrected where they stood ("ANIME 140 gsMIN" -> "ANIME 140 BPM
  Fm"); if they're right, the card says so.
- Key names written with capitals are understood: "CSMIN", "DBmin".
- The progress card can be dragged aside; its glass follows.
- The glass icon stays at desktop level: it no longer jumps above windows during drags in
  other apps, and drags in FL Studio no longer make the ring flicker on it.

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
