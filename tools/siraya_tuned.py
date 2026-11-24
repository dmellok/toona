"""Builds crates/core/data/tuned/siraya_tech.json: Siraya Tech's published print settings
for Bambu printers, as numbers only.

Input is the JSON written by a fetch of the project files linked from
https://siraya.tech/pages/print-settings-download (one record per link: the page label,
the URL, and the filament settings read out of the project). Nothing of the files
themselves is kept: no G-code, no process settings, no models.

Usage: python3 tools/siraya_tuned.py <extracted.json> <retrieved YYYY-MM-DD>
"""
import json, re, sys

SOURCE = "https://siraya.tech/pages/print-settings-download"

# Page label -> catalogue product. Labels not listed are left out: products the
# catalogue does not have, high-flow variants, and foaming filaments whose settings
# depend on the hardness wanted.
PRODUCTS = {
    "ABS HT HF": "siraya_tech/ABS/fibreheart_abs_ht_hf",
    "ABS-CF Core": "siraya_tech/ABS/fibreheart_abs_cf_core",
    "ABS-GF": "siraya_tech/ABS/fibreheart_abs_gf",
    "ASA-GF": "siraya_tech/ASA/fibreheart_asa_gf",
    "PEBA 85A": "siraya_tech/PEBA/rebound_peba_85a",
    "PEBA 95A": "siraya_tech/PEBA/rebound_peba_95a",
    "PET-CF": "siraya_tech/PET/fibreheart_pet_cf",
    "PET-GF": "siraya_tech/PET/fibreheart_pet_gf",
    "PETG-CF Pro": "siraya_tech/PETG/fibreheart_petg_cf_pro",
    "PPA": "siraya_tech/PPA/fibreheart_ppa",
    "PPA-CF": "siraya_tech/PPA/fibreheart_ppa_cf",
    "PPA-CF Core": "siraya_tech/PPA/fibreheart_ppa_cf_core",
    "TPU 64D": "siraya_tech/TPU/flex_tpu_64d",
    "TPU 85A": "siraya_tech/TPU/flex_tpu_85a",
    "TPU-GF": "siraya_tech/TPU/fibreheart_tpu_gf",
    "Roamr TPU Air HR 80A": "siraya_tech/TPU/roamr_tpu_air_hr_80a",
    "Roamr TPU Air HR 85A": "siraya_tech/TPU/roamr_tpu_air_hr_85a",
}

# The settings taken. Plain numbers a profile already has; never G-code.
KEYS = [
    "nozzle_temperature", "nozzle_temperature_initial_layer",
    "nozzle_temperature_range_low", "nozzle_temperature_range_high",
    "hot_plate_temp", "hot_plate_temp_initial_layer",
    "textured_plate_temp", "textured_plate_temp_initial_layer",
    "cool_plate_temp", "cool_plate_temp_initial_layer",
    "eng_plate_temp", "eng_plate_temp_initial_layer",
    "supertack_plate_temp", "supertack_plate_temp_initial_layer",
    "filament_max_volumetric_speed", "filament_flow_ratio",
    "fan_max_speed", "fan_min_speed", "fan_cooling_layer_time",
    "slow_down_layer_time", "slow_down_min_speed", "close_fan_the_first_x_layers",
    "overhang_fan_speed", "additional_cooling_fan_speed",
    "filament_density", "filament_retraction_length", "temperature_vitrification",
]
NUMBER = re.compile(r"^-?\d+(\.\d+)?$")


def product_of(label):
    name = re.sub(r"\(.*?\)", "", label)
    name = re.sub(r"\b(X1C|Black|Rebound)\b", "", name)
    return PRODUCTS.get(" ".join(name.split()))


def printer_of(settings):
    name = re.sub(r"\(.*$", "", str(settings.get("printer_settings_id", ""))).strip()
    return name if re.fullmatch(r"Bambu Lab .+ 0\.\d nozzle", name) else None


def own_value(settings, key):
    """The first value of a setting: the one Siraya tuned. A project for a two-nozzle
    printer goes on to list the other nozzle variants and a second, stock filament,
    which are left at whatever the project was started from."""
    values = settings.get(key)
    if not isinstance(values, list) or not values:
        return None
    first = values[0]
    return first if isinstance(first, str) and NUMBER.match(first) else None


def main():
    records = json.load(open(sys.argv[1]))
    found, skipped, clashes = {}, {}, []
    for record in records:
        settings = record.get("settings")
        product = product_of(record["label"])
        printer = printer_of(settings) if settings else None
        if not settings or not product or not printer:
            skipped[record["label"]] = skipped.get(record["label"], 0) + 1
            continue
        taken = {key: own for key in KEYS if (own := own_value(settings, key))}
        # Pressure advance only counts where the profile turns it on.
        if own_value(settings, "enable_pressure_advance") == "1":
            if own := own_value(settings, "pressure_advance"):
                taken["pressure_advance"] = own
        entry = {"product": product, "printer": printer, "settings": taken}
        key = (product, printer)
        if key in found and found[key]["settings"] != taken:
            clashes.append(key)
        found.setdefault(key, entry)
    # Two different answers for the same product and printer: neither is trusted.
    for key in set(clashes):
        del found[key]
    entries = sorted(found.values(), key=lambda e: (e["product"], e["printer"]))
    out = {
        "brand": "Siraya Tech",
        "source": SOURCE,
        "retrieved": sys.argv[2],
        "note": "Values read from the print settings Siraya Tech publishes for Bambu printers. Numbers only.",
        "entries": entries,
    }
    # One entry per line, so a change to one printer's numbers is a one-line diff.
    head = json.dumps({k: v for k, v in out.items() if k != "entries"}, indent=1)[:-2]
    lines = ",\n".join("  " + json.dumps(entry, separators=(",", ":")) for entry in entries)
    with open("crates/core/data/tuned/siraya_tech.json", "w") as file:
        file.write(f'{head},\n "entries": [\n{lines}\n ]\n}}\n')
    print(f"{len(entries)} entries for {len({e['product'] for e in entries})} products")
    print("left out (two different answers):", sorted(set(clashes)))
    print("left out (no catalogue product, or not a plain Bambu printer):", dict(sorted(skipped.items())))


if __name__ == "__main__":
    main()
