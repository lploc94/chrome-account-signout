# Chrome Account Signout

A small desktop app for removing one secondary Google account from Chrome on your device.

Account list → **Log out** next to an account → confirmation → result.
The app asks Chrome to close normally, removes the selected local account token,
and reopens the profile. The primary sync account is protected. There is no restore feature.

This changes local data only; it does not revoke Google sessions on other devices.
Chrome may change its data format. The app refuses to proceed if it cannot verify
the account, database schema, paths, or processes using the profile.

## Support

- macOS 14 or later: Apple Silicon and Intel.
- Windows 10/11 x64: experimental; requires WebView2.
- Google Chrome stable with the current user's default data location. A data root
  relocated through a link is supported. Arbitrary user-data-dir selection,
  Beta/Canary, Edge, and removal of the primary sync account are not supported.
- No Google password required. No token decryption or account data sent over the network.

Accounts are listed from cached profile metadata, so the list can appear while
Chrome is open and may include accounts that have already been removed. Chrome
is asked to close only after you confirm logout. The app then checks the account
identity and data again before removal. If Chrome still holds the database lock,
the app refuses to write and asks you to close Chrome manually.
If the operating system blocks access, you decide whether to grant permission.

All examples and test data are synthetic, such as `friend@example.test`.

## Development

Built with Tauri 2, Rust, and TypeScript/CSS. Account handling is shared across
platforms; operating system integration uses separate adapters. The previous Swift
app is not a dependency.

There is no telemetry or automatic updater.

Chrome is detected through registered applications and running processes on macOS,
and App Paths plus verified running processes on Windows. If multiple installations
make profile ownership ambiguous, the app refuses to proceed. macOS Crashpad
processes are matched to the exact Crashpad directory. Chrome instances with a
separate user-data-dir are not closed. If Chrome keeps running in the background
or refuses to close, save your work, close it manually, and try again.

## Build and test

Install Rust, Node.js, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
Windows requires MSVC Build Tools; macOS requires Xcode Command Line Tools.

```sh
npm ci
npm run check
npm test -- --run
npm run build
cargo test --workspace --locked
npm run tauri dev
```

Build the frontend before running Cargo commands that compile the app. Frontend
output goes to `ui-dist/`; `dist/` is reserved for release packages. Never use real
Chrome data in tests.
Confirmation is enforced in Rust: confirmation tokens are single-use, account
identity is rechecked before writing, and only one operation can run at a time,
even across multiple app instances.

## Releases

The macOS app is ad-hoc signed and is not notarized by Apple. The Windows installer
has no publisher signing certificate. Windows remains experimental until real
account removal has been tested.
The Windows installer installs for the current user and requires Microsoft Edge
WebView2 Runtime. It does not automatically grant data access or change operating
system security settings.

Release packages must pass privacy, SHA-256, and native checks before upload.
The manifest records the source commit and each package and executable hash.
Windows payload inspection requires `7zz` or `7z`; failure to extract blocks verification.
PDB files, source maps, browser data, and account screenshots are excluded from releases.

The reviewed image hash list contains the app icons and the default wizard artwork,
`Contrib/Graphics/Wizard/win.bmp`, from [Tauri's NSIS 3.13 distribution](https://github.com/tauri-apps/binary-releases/releases/tag/nsis-3.13).
The NSIS image was matched byte for byte against the official package and visually
inspected. Only that exact hash is accepted; unreviewed images remain blocked.

Build release packages with normalized source paths:

```sh
python3 scripts/build-release.py --target universal-apple-darwin --bundles app
python3 scripts/package-release.py --platform macos
python3 scripts/check-public-content.py --source --history --artifacts dist
python3 scripts/verify-release.py --dir dist
```

On Windows, use `python scripts/build-release.py --bundles nsis`.
The installed-app qualification helper leaves the Windows app open for observation.
Rerun it with `-GuiConfirmed -Cleanup` only after observing the account list or an
actionable error. This flag records manual confirmation; it does not independently
prove that the GUI works.

Dependency license notices are preserved in `THIRD-PARTY-NOTICES.txt` and bundled
with the app. The list includes build dependencies and dependencies for other
platforms; it does not define a license for this project's own source code.

The application interface, results, and error messages are in English.
