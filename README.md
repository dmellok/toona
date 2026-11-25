# Toona

Toona is a desktop app that adds third-party filaments to Bambu Studio. It reads the [Open Filament Database](https://github.com/OpenFilamentCollective/open-filament-database), lets you pick a filament and a printer, and writes a filament profile that Bambu Studio lists under Custom, the same kind it makes itself with Create Custom Filament.

It is an early preview. It runs on macOS on Apple silicon, and the builds are not signed with an Apple developer certificate or notarised.

## What it does

- Browses the catalogue by brand, material and name, or by scanning a spool's barcode.
- Builds each profile on one of Bambu Studio's own presets and changes only what the filament needs, so you can see every setting that differs and why.
- Backs up before it writes, and refuses to write while Bambu Studio is open.
- Lists what it added, lets you edit or remove it, and repairs profiles that were imported in a form Bambu Studio hides.
- Optionally, with a GitHub sign-in, shares a profile's changes with other people and shows which shared ones worked for them. G-code is never taken from, or sent to, anyone.

## What it sends

Toona talks to two places: GitHub, to download the catalogue, and `api.toona.wiki`, for shared profiles and update checks. On first run it asks two questions:

- **Check for updates** (on unless you turn it off): when Toona opens it asks whether a newer version exists. The request carries the Toona version and the kind of computer.
- **Share usage numbers** (off unless you turn it on): once a day, the Toona version, system, slicer, printer models and how many filaments were added. There is no id and no account; the first-run sheet shows exactly what would be sent.

## Layout

| Folder | What it is |
| --- | --- |
| `crates/core` | The library: reads the catalogue and the slicer's folders, builds and installs profiles |
| `crates/cli` | A command-line front end to the library |
| `app` | The desktop app: Tauri, with a React window in `app/src` and its backend in `app/src-tauri` |
| `tools` | Scripts that prepare data shipped with the library |
| `PROFILE_SPEC.md` | What a Bambu Studio filament profile is, worked out from real ones |

The server behind `api.toona.wiki` is not part of this repository.

## Build

You need Rust and Node.

```
cargo test --workspace
cd app
npm install
npx tauri dev      # run the app
npx tauri build    # make Toona.app and a .dmg
```

A release build also signs the update file, which needs the maintainers' signing key in `TAURI_SIGNING_PRIVATE_KEY`. Without it, add `--config '{"bundle":{"createUpdaterArtifacts":false}}'`.

## Licence

AGPL-3.0-or-later. See [LICENSE](LICENSE).
