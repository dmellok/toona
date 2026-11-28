<p align="center">
  <img src="app/src-tauri/icons/128x128@2x.png" width="112" height="112" alt="Toona's icon">
</p>

<h1 align="center">Toona</h1>

<p align="center">
  <strong>Add third-party filaments to Bambu Studio and PrusaSlicer.</strong><br>
  Pick a filament, pick your printer, and it shows up in the slicer.
</p>

<p align="center">
  <a href="https://github.com/dmellok/toona/releases"><img src="https://img.shields.io/github/v/release/dmellok/toona?include_prereleases&label=download&color=15171A" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/macOS-Apple%20silicon-15171A" alt="macOS on Apple silicon">
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-AGPL--3.0-15171A" alt="Licence: AGPL 3.0"></a>
</p>

![Toona's Browse view: a search for Polymaker, cards for each filament, and Polylite PLA in Azure Blue ready to add to Bambu Studio](.github/readme/browse.png)

Toona is a desktop app that reads the [Open Filament Database](https://github.com/OpenFilamentCollective/open-filament-database) and writes a filament profile that Bambu Studio lists under Custom, the same kind it makes itself with Create Custom Filament. For PrusaSlicer it writes a filament preset that shows under User presets.

It is an early preview. It runs on macOS on Apple silicon, and the builds are not signed with an Apple developer certificate or notarised. [Download the latest build](https://github.com/dmellok/toona/releases).

## What it does

- Browses the catalogue by brand, material and name, or by scanning a spool's barcode.
- Builds each profile on one of the slicer's own presets and changes only what the filament needs, so you can see every setting that differs and why.
- Backs up before it writes, and refuses to write while the slicer is open.
- Lists what it added, lets you edit or remove it, and repairs profiles that were imported in a form Bambu Studio hides.
- Optionally, with a GitHub sign-in, shares a profile's changes with other people and shows which shared ones worked for them. G-code is never taken from, or sent to, anyone.

## Every setting, and where it came from

Each profile is built on one of Bambu Studio's own presets. Turn on Advanced and a profile's page shows which settings differ from that preset and which layer set each one: the manufacturer's figures, the colour, a profile someone shared, or your own edits.

![A profile's Layers view: columns for density, nozzle temperature, flow and other settings, with rows showing which layer set each value](.github/readme/layers.png)

## What's in Bambu Studio

The In Studio view lists what Toona added, the custom filaments you made yourself, and imported profiles that Bambu Studio hides from the AMS list, which Toona can repair.

![The In Studio view: a filament Toona added, custom filaments made in Bambu Studio, and imported profiles with a Fix button](.github/readme/in-studio.png)

## PrusaSlicer

With both slicers installed, a menu in the header switches between them. PrusaSlicer ties a filament to printers by a condition instead of keeping a file per nozzle, so Toona builds on the preset PrusaSlicer shows for your printer and keeps that preset's condition: one filament can serve several nozzles, and where a printer needs its own it is named PrusaSlicer's way, such as `eSUN PLA+ Orange @MK4S HF0.4`. PrusaSlicer support is new and has been tried on PrusaSlicer 2.9 with Prusa's own printers.

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
