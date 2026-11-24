# Bambu Studio user filament profile: minimum valid spec

Derived from this machine on 2026-11-23, not from documentation.

- Bambu Studio app `02.09.01.70`, system profile bundle `02.09.00.83` (`system/BBL.json`)
- Config root (macOS): `~/Library/Application Support/BambuStudio`
- Evidence: three profiles made with **Create Custom Filament** (`user/10000001/filament/base/*.json`),
  five profiles brought in with **Import Configs** (`user/10000001/filament/*.json`), the system
  presets they derive from, and the Bambu Studio source (`master`) for the loader, the AMS dialog
  and the cloud sync. Each rule below says which of these it rests on.

Markers: **[files]** seen in the real files, **[source]** read in Studio's source,
**[verified]** reproduced by a script against the real files.

## 1. Why imported profiles are not usable and created ones are

| Field | Create Custom Filament | Import Configs |
|---|---|---|
| Location | `filament/base/<name>.json` | `filament/<name>.json` |
| `name` | `eSUN PLA Plus @Bambu Lab X2D 0.4 nozzle` | `X2D  eSUN PLA+ Filament` (no `@`) |
| `inherits` | `""` (root preset) | `Bambu PLA Basic @BBL X2D 0.4 nozzle` |
| `filament_id` | `P6c4d05a` (own id) | absent; loader copies the parent's (`GFA00`) |
| `compatible_printers` | `["Bambu Lab X2D 0.4 nozzle"]` | absent; inherited from the parent |
| `filament_vendor` | `["eSUN"]` | absent; inherited (`Bambu Lab`) |
| `filament_type` | `["PLA"]` | absent; inherited |
| `filament_settings_id` | `[<name>]` | `[<name>]` |
| `from` | `User` | `User` |
| `version` | `2.9.0.83` | `2.5.0.18` (whatever the exporting machine had) |
| `instantiation`, `setting_id`, `type` | absent | absent |
| Other keys | full config, 158 keys | only the 3 to 6 keys that differ from the parent |
| `.info` `base_id` | empty | parent's `setting_id` (`GFSA00_08`) |
| `.info` `setting_id` | `PFUS…` (assigned by Bambu Cloud) | `PFUS…` (assigned by Bambu Cloud) |
| `.info` `sync_info`, `user_id` | empty | empty |

What that causes **[source]**:

1. **AMS / printer list.** `AMSMaterialsSetting::get_filaments_info` skips every preset that is not
   its own root (`get_preset_base(preset) != &preset`), every preset with an empty `filament_id`,
   every preset whose `compatible_printers` does not name a preset of the connected printer and
   nozzle, and every preset with an empty alias. An imported profile inherits, so it is never
   offered for an AMS slot, and even if it were it would report the parent's id (`GFA00`,
   "Bambu PLA Basic") to the printer. A created profile is a root with its own `P…` id.
2. **Alias.** Only a user root preset gets an alias, and the alias is the name up to `@`. No `@`
   in the name means no alias.
3. **Printer compatibility.** An imported profile is compatible only with what its parent lists
   (here one nozzle preset). If the parent preset does not exist on the importing machine the
   loader drops the profile entirely ("can not find parent").

User filament presets are always enabled; the `filaments` list in `BambuStudio.conf` only holds
system presets (297 entries, none of the eight user profiles) **[files]**.

Not established from the files: whether the five imported profiles show in the main filament
dropdown on this machine. By the loader's rules they should, for the X2D 0.4 nozzle only.

## 2. Files

For one filament on one printer preset, two files in `user/<preset_folder>/filament/base/`:

```
<alias> @<printer preset name>.json
<alias> @<printer preset name>.info
```

- `<preset_folder>` is `app.preset_folder` in `BambuStudio.conf` (the Bambu account id, or
  `default` when signed out) **[files]**.
- `base/` is where Studio puts root ("detached") presets; the loader reads `filament/` and then
  `filament/base/` **[source]**.
- One file per printer preset. A filament for several nozzles is several files sharing one alias
  and one `filament_id` **[source]**.
- UTF-8, LF, 4-space indent, keys sorted, trailing newline **[files]**.

## 3. The `.json`

### Required keys

| Key | Value | Basis |
|---|---|---|
| `name` | `<alias> @<printer preset name>`; equals the file name without `.json` | files, source |
| `from` | `"User"` | files |
| `inherits` | `""` | files, source |
| `version` | Dotted numeric; major must not exceed the app's. Use the system bundle version without zero padding: `02.09.00.83` gives `2.9.0.83`. A file with no parsable version is skipped. | files, source |
| `filament_id` | `"P"` + first 7 hex of `md5("<alias>@<user_id>")`, or `md5("<alias>")` when signed out. Same alias on another printer preset reuses the id. | verified on all three profiles |
| `filament_settings_id` | `[<name>]` | files |
| `compatible_printers` | `[<printer preset name>]`, a system machine preset name such as `Bambu Lab X2D 0.4 nozzle` | files, source |
| `filament_vendor` | `[<vendor>]` | files |
| `filament_type` | `[<type>]`, a type Studio knows (`PLA`, `PETG`, `ABS`, `TPU`, …) | files |

Must be absent: `setting_id`, `instantiation`, `type`, `include`, `base_id`. These belong to
system presets or to the `.info` **[files]**.

### Settings

Every other key is a Studio filament option. Values are strings or arrays of strings, never
JSON numbers or booleans **[files]**.

A root preset has no parent, so it has to carry the settings itself. Studio writes the base
preset's whole resolved config (158 keys). Any key left out takes Studio's built-in default,
not the base preset's value **[source]**, so the minimum that prints like the base preset is the
base preset's resolved chain. Resolution, checked against all three real profiles **[verified]**:

1. Resolve the parent (`inherits`) first, recursively.
2. Apply each preset named in `include` over that (all its settings).
3. Apply the preset's own settings.
4. Per-extruder-variant options whose array is shorter than `filament_extruder_variant` are
   padded by repeating the last value (on the X2D: 6 variants).
5. Drop `filament_contact_safe`, `filament_emission_safe`, `filament_ingredients_safe`,
   `filament_long_retractions_when_ec`, `filament_retraction_distances_when_ec`: present in
   system presets, never written by Studio 2.9.

Doing this for `Bambu PLA Basic @BBL X2D 0.4 nozzle` and `Bambu PETG Basic @BBL X2D 0.4 nozzle`
reproduces the real profiles key for key. The only differences are the values edited by hand
(vendor, plate and nozzle temperatures, volumetric speed) and one number format
(`"12.0"` where Studio wrote `"12"`).

The 22 keys Studio writes beyond the resolved chain are its defaults (`pressure_advance`,
`filament_notes`, `compatible_prints`, `default_filament_colour`, …) and may be omitted.

`filament_flush_temp_fast`, `filament_retract_length_toolchange` and
`filament_retract_restart_extra_toolchange` are in the per-variant list in Studio `master`, but
Studio 2.9 writes each with one value **[files]**. Leave them at one value.

## 4. The `.info`

Five lines, `key = value`, in this order, LF, written even when the value is empty **[files]**:

```
sync_info = 
user_id = 
setting_id = 
base_id = 
updated_time = 1795459307
```

| Key | For a new profile | Notes |
|---|---|---|
| `sync_info` | empty | Studio's own states: `create`, `update`, `delete`, `hold`, `save` |
| `user_id` | empty | Empty in all eight real files, including synced ones |
| `setting_id` | empty | Assigned by Bambu Cloud (`PFUS` + 14 hex) at first sync |
| `base_id` | empty | Must be empty for a root preset. Imported ones hold the parent's `setting_id` |
| `updated_time` | Unix seconds | If missing or 0 Studio stamps the current time |

The file is optional for loading: without it all five values start empty **[source]**.

## 5. Cloud sync

`app.sync_user_preset` is `true` on this machine and the account is signed in.

- A user preset with an empty `setting_id` is uploaded to Bambu Cloud at the next sync, and Studio
  then writes the returned `PFUS…` id into the `.info` **[source]**. Studio syncs at start-up.
  A profile written into the folder is therefore uploaded the next time Studio starts, the same
  as one made with Create Custom Filament. There is no file-level switch that reliably prevents
  it: `sync_info = hold` is honoured once but never saved back.
- Once a profile has a `setting_id`, removing only the local files does not remove it: the next
  sync downloads it again. It has to be deleted inside Studio, which also deletes the cloud copy.
- Whether the printer or the Handy app need the cloud copy to recognise a `P…` id was not
  established here.

## 6. Loader behaviour to respect

- A `.json` that fails to parse is **deleted** by Studio along with its `.info` **[source]**.
  Write to a temporary name and rename, and never write while Studio is running.
- A user preset whose name matches an already loaded preset is ignored.
- The X2D's static capability file says `support_user_preset: false`, but the flag is set at
  run time from the printer, and this printer's AMS already holds three `P…` ids
  (`ams_filament_ids` in `BambuStudio.conf`) **[files]**.

## 7. Printers on this machine

- `user/10000001/machine/` is empty: no user machine presets.
- `BambuStudio.conf` `models`: `Bambu Lab X2D`, nozzles `0.2;0.4;0.6;0.8`.
- Selected preset: `Bambu Lab X2D 0.4 nozzle`. Installed nozzles (`nozzle_slots`): `0.4 Standard` ×2.
- System machine presets: `Bambu Lab X2D 0.2 / 0.4 / 0.6 / 0.8 nozzle` (`GM046`, `GM045`, `GM047`, `GM048`).

## 8. Import bundle (fallback)

Studio's own export is `<alias>.bbsflmt`, a zip **[source]**:

```
bundle_structure.json
<vendor>/<alias> @<printer preset name>.json
```

```json
{
    "bundle_id": "<user_id or offline>_<alias>_<timestamp>",
    "bundle_type": "filament config bundle",
    "filament_name": "<alias>",
    "filament_vendor": [
        { "vendor": "<vendor>", "filament_path": ["<vendor>/<alias> @<printer>.json"] }
    ],
    "version": ""
}
```

The entry name `bundle_structure.json` is from memory of Studio's headers and was not re-read
here. Importing such a bundle has not been tested on this machine.

## 9. OrcaSlicer

Not installed here, nothing verified. Same profile family, so the same rules are expected to
apply with `OrcaSlicer/` as the config root.
