# Security

## Reporting a problem

If you find a security problem in con/vert, please report it privately through
[GitHub's private vulnerability reporting](https://github.com/{{GH_USER}}/con-vert/security/advisories/new)
rather than in a public issue. Include what you found, how to reproduce it and which
version you were using (Settings shows it).

Fixes ship as a new release on this repository.

## How con/vert protects you

- **Files stay on your PC.** Files are processed locally. Nothing is uploaded and there is
  no server.
- **No network connections.** con/vert doesn't check for updates or send anything. (The
  installer may download Microsoft's WebView2 runtime if Windows doesn't have it yet.)
- **No admin rights.** It installs for the current Windows user and runs without
  elevation.
- **Originals are read, never written.** Every result is a new file.
- **Crash reports stay local.** They're saved in
  `%LOCALAPPDATA%\app.convert.desktop\crashes` and never sent.

## Supported versions

Only the latest release gets fixes.
