# Deploy

Moonshift's reusable installer. A small Tauri app that installs, updates,
and uninstalls a Windows app from a single self-contained `setup.exe`.

Deploy is a **dependency of the app it installs**: it contains no app data at
all. The app describes itself in an `app-pack.json` and points deploy at it
via `DEPLOY_APP_CONFIG` when building the installer. Every built installer is
single-app by construction and is named after that app at build time.

## The app-pack.json

Lives in the app repo (e.g. `packaging/app-pack.json`); all paths resolve
relative to the file itself:

```jsonc
{
  "appName": "App",
  "description": "...",
  "publisher": "Publisher",
  "identityName": "Publisher.App",        // sparse MSIX identity (optional but recommended)
  "identityPublisher": "CN=Publisher",    // must match the signing cert subject
  "identityCapabilities": ["runFullTrust", "unvirtualizedResources", "..."], // optional extras, e.g. graphicsCaptureProgrammatic
  "versionSource": "../src-tauri/tauri.conf.json",
  "defaultDir": "%LOCALAPPDATA%\\Programs\\App",
  "registryKey": "Software\\Publisher\\App",
  "launchExe": "app.exe",
  "startMenuName": "App",
  "autostartName": "App",
  "processes": ["app.exe", "helper.exe"],
  "iconPath": "../src-tauri/icons/128x128.png",
  "certPath": "dist/App.cer",             // produced by make-identity-package.ps1
  "msixPath": "dist/App.Identity.msix",   // produced by make-identity-package.ps1
  "payload": [
    { "path": "../src-tauri/target/release/app.exe", "name": "app.exe" },
    { "path": "../src-tauri/target/release/helper.exe", "name": "helper.exe" }
  ]
}
```

`payload` and `versionSource` are required; `iconPath`, `certPath`,
`msixPath`, and the identity fields are optional (a build without them skips
the corresponding install steps).

## CLI

Interactive install:

    App_setup.exe

Flags (NSIS-compatible, understood automatically by tauri-plugin-updater):

    /P              passive mode (small progress window, used for updates)
    /S              silent mode (headless, no UI at all)
    /UPDATE         update mode: install into the previously registered dir
    /R              restart the app after installing (with /ARGS arguments)
    /ARGS <args>    arguments to relaunch the app with
    /D=<dir>        override the install directory
    /uninstall      interactive uninstall

The installer also registers the app with Windows: an Add/Remove Programs
entry (Settings → Installed apps, `UninstallString` pointing at an
`uninstall.exe` copy inside the install directory) and a Start menu
"Uninstall <App>" shortcut. Uninstalling removes the app's files, shortcuts,
autostart entry, registry record, identity registration, and both entries.

## Building (from the app's side)

Set `DEPLOY_APP_CONFIG=<path to app-pack.json>` and run `npm run tauri build
-- --no-bundle` here; the consuming app's build script does this and renames
`src-tauri/target/release/deploy.exe` to `<App>_<version>_x64-setup.exe`.

## Scripts (generic, app-agnostic)

- `scripts/setup-signing.ps1 -Config <app-pack.json>` — one-time: creates the
  self-signed code-signing certificate and the minisign updater keypair
  (stored in `keys/` next to the app's config)
- `scripts/make-identity-package.ps1 -Config <app-pack.json>` — builds and
  signs the sparse identity MSIX
- `scripts/make-release-manifest.ps1 -Config ... -Artifact ... -Signature ...`
  — generates the updater's `latest.json`

See the app's own `packaging/build-installer.ps1` for the full pipeline.
