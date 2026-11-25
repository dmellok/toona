// Everything the window asks the backend for. Inside the app this goes through
// Tauri; in a plain browser (front-end development) it reads JSON that
// `cargo run -p toona-app --example dump` wrote into public/mock.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface Printer {
  name: string;
  model: string;
  nozzle: string;
  selected: boolean;
}

export interface Status {
  slicer: string;
  found: boolean;
  running: boolean;
  cloud_sync: boolean;
  printers: Printer[];
  problem: string | null;
}

export interface Card {
  key: string;
  brand: string;
  name: string;
  material: string;
  resin: boolean;
  colours: number;
  stripes: string[];
  discontinued: boolean;
  added: boolean;
}

export interface CatalogueData {
  version: string;
  warning: string | null;
  items: Card[];
}

export interface Variant {
  name: string;
  alias: string;
  hex: string | null;
  added: boolean;
}

export interface Base {
  preset: string;
  label: string;
  source: string;
}

/** One setting a profile changes over the preset it is built on. */
export interface Change {
  key: string;
  layer: "manufacturer" | "colour" | "community" | "you";
  from: string | null;
  to: string;
}

/** One thing the manufacturer lists, next to what the profile would use. */
export interface Spec {
  key: "nozzle" | "bed" | "flow" | "density";
  spec: string | null;
  value: string | null;
  /** Whether the profile's value is inside what the manufacturer lists; null when unknown. */
  ok: boolean | null;
  /** For flow, in mm³/s: the manufacturer's limit, the profile's value and the base preset's. */
  limit: number | null;
  at: number | null;
  base: number | null;
}

export interface Detail {
  key: string;
  brand: string;
  name: string;
  material: string;
  nozzle: [number, number] | null;
  bed: [number, number] | null;
  density: number | null;
  traits: string[];
  discontinued: boolean;
  variants: Variant[];
  /** The filament name the slicer shows when it is added without a colour. */
  alias: string;
  /** Already in the slicer without a colour. */
  added: boolean;
  base: Base | null;
  bases: string[];
  /** The printer presets the base in use exists for. */
  base_printers: string[];
  /** What adding it would change over the base preset, for the chosen colour. */
  changes: Change[];
  spec: Spec[];
  /** The name the catalogue gives the colour on screen, before any renaming. */
  default_alias: string;
  /** Set when the manufacturer published settings for this filament on this printer model. */
  tuned?: {
    brand: string;
    source: string;
    retrieved: string;
    /** How many settings it sets on the printer asked about. */
    settings: number;
    /** Printer presets of this model the values were published for. */
    published: string[];
    /** Published for the printer asked about, not borrowed from another nozzle. */
    own: boolean;
    borrowed_from: string | null;
  } | null;
  blocked: string | null;
}

export interface InstallRequest {
  key: string;
  variant: string | null;
  printers: string[];
  base: string | null;
  /** A filament name to use instead of the catalogue's. */
  alias: string | null;
  /** Use the manufacturer's settings for another nozzle where none were published for this one. */
  borrow_tuned: boolean;
  allow_cloud_sync: boolean;
  dry_run: boolean;
}

export interface Planned {
  name: string;
  filament_id: string;
  base: string;
  source: string;
}

export interface InstallResult {
  alias: string;
  dry_run: boolean;
  profiles: Planned[];
  files: string[];
  backup: string | null;
  will_cloud_sync: boolean;
}

export interface Added {
  profile_name: string;
  alias: string;
  catalogue_key: string;
  printer: string;
  hex: string | null;
  synced: boolean;
  present: boolean;
  parent: string | null;
  changes: Change[];
  /** It was made from, or already shared as, a version on the Toona server. */
  linked?: boolean;
}

/** A custom filament the user made in the slicer themselves. */
export interface Own {
  profile_name: string;
  alias: string;
  printer: string;
  hex: string | null;
  synced: boolean;
  catalogue_key: string | null;
  product: string | null;
  parent: string | null;
  changes: Change[];
  linked: boolean;
}

export interface Setting {
  key: string;
  group: string;
  base: string | null;
  value: string;
  /** Who changed it over the base preset; null when it is the preset's own value. */
  layer: Change["layer"] | null;
  editable: boolean;
  min: number;
  max: number;
  /** How much one press of the stepper changes it. */
  step: number;
  /** Shown after the value, such as "°". */
  unit: string;
}

/** One profile in the slicer, with where each value comes from. */
export interface Profile {
  profile_name: string;
  alias: string;
  printer: string;
  hex: string | null;
  built_on: string;
  brand: string | null;
  product: string | null;
  editable: boolean;
  version_id: number | null;
  /** The most the manufacturer says it can be pushed through a nozzle, in mm³/s. */
  max_flow?: number | null;
  settings: Setting[];
}

export interface Edit {
  key: string;
  value: string;
}

/** A profile the user imported, which the AMS does not offer. */
export interface Imported {
  name: string;
  /** Why Toona cannot make an AMS copy of it, if it cannot. */
  problem: string | null;
  /** The AMS copy already exists. */
  fixed: boolean;
}

export interface InStudio {
  added: Added[];
  own?: Own[];
  imported: Imported[];
}

export interface RemoveResult {
  removed: number;
  backup: string | null;
  cloud_synced: string[];
}

export interface BarcodeMatch {
  key: string;
  variant: string;
}

export interface User {
  id: number;
  handle: string;
  name: string | null;
  avatar: string | null;
}

export interface Account {
  /** The Toona server answers. When it doesn't, the community parts are left out. */
  available: boolean;
  user: User | null;
}

/** A sign-in in progress: the user types `user_code` in at GitHub. `done` means it needed no code. */
export interface SignIn {
  done: boolean;
  user: User | null;
  user_code: string;
  verification_uri: string;
  device_code: string;
  interval: number;
}

/** A profile version someone shared. */
export interface Shared {
  id: number;
  profile_id: number;
  parent_version: number | null;
  author: string;
  note: string;
  created: string;
  worked: number;
  problems: number;
  badges: string[];
  mine: "worked" | "problems" | null;
  own: boolean;
  built_on: string;
  changes: Change[];
  unusable: string | null;
  added: boolean;
}

/** `kind` says which dialog to show; `message` is written for people. */
export interface UiError {
  kind: "running" | "cloud_sync" | "no_slicer" | "offline" | "sign_in" | "other";
  message: string;
}

export function asUiError(error: unknown): UiError {
  if (error && typeof error === "object" && "kind" in error && "message" in error) {
    return error as UiError;
  }
  return { kind: "other", message: String(error) };
}

const inApp = "__TAURI_INTERNALS__" in window;

/** What the person chose about update checks and usage numbers. */
export interface Prefs {
  /** The first-run question was answered. */
  asked: boolean;
  updates: boolean;
  usage: boolean;
  version: string;
  /** Exactly what a usage hello sent now would contain. */
  report: Record<string, unknown>;
}

export interface Update {
  version: string;
  notes: string;
}

// In a plain browser: add ?firstrun to see the first-run question, ?update for a waiting update.
const mockQuery = new URLSearchParams(location.search);
const mockPrefs: Prefs = {
  asked: !mockQuery.has("firstrun"),
  updates: true,
  usage: false,
  version: "0.3.0",
  report: {
    version: "0.3.0",
    os: "macos",
    os_version: "15.1",
    arch: "aarch64",
    slicer: "Bambu Studio",
    printers: ["Bambu Lab X2D"],
    added: 4,
    first: true,
    new_week: true,
    new_month: true,
  },
};
const mockUpdate: Update = { version: "0.3.1", notes: "Faster start.\nFixes a nozzle mix-up on the A1 mini." };

async function mock<T>(file: string): Promise<T> {
  const response = await fetch(`/mock/${file}.json`);
  if (!response.ok) throw { kind: "other", message: `No mock data for ${file}` } satisfies UiError;
  const body = await response.json();
  if (body && typeof body === "object" && "err" in body) throw body.err as UiError;
  return (body && typeof body === "object" && "ok" in body ? body.ok : body) as T;
}

/** In the browser, `?studio=closed` pretends the slicer is not running so the install flow can be walked through. */
const mockClosed = new URLSearchParams(location.search).get("studio") === "closed";

async function mockWrite<T>(allowSync: boolean, file: string): Promise<T> {
  const status = await api.status();
  if (status.running) throw { kind: "running", message: `${status.slicer} is running.` } satisfies UiError;
  if (status.cloud_sync && !allowSync) throw { kind: "cloud_sync", message: "Not confirmed." } satisfies UiError;
  return mock<T>(file);
}

// Browser-only stand-ins for the account and community calls.
const mockUser: User = { id: 1, handle: "you", name: null, avatar: null };
let mockSignedIn = false;
const mockShared: Shared[] = [
  {
    id: 2, profile_id: 1, parent_version: 1, author: "printsmith", note: "Dropped the nozzle 5 °C, less stringing.",
    created: "2026-11-20T10:00:00Z", worked: 12, problems: 1, badges: ["tested"], mine: null, own: false,
    built_on: "Generic PLA", unusable: null, added: false,
    changes: [
      { key: "nozzle_temperature", layer: "community", from: "220", to: "215" },
      { key: "filament_flow_ratio", layer: "community", from: "0.98", to: "0.96" },
    ],
  },
  {
    id: 1, profile_id: 1, parent_version: null, author: "layerlines", note: "",
    created: "2026-11-12T10:00:00Z", worked: 3, problems: 0, badges: [], mine: null, own: false,
    built_on: "Generic PLA", unusable: null, added: false,
    changes: [{ key: "filament_flow_ratio", layer: "community", from: "0.98", to: "0.96" }],
  },
];

const mockOwn: Own = {
  profile_name: "eSUN PLA Plus @Bambu Lab X2D 0.4 nozzle", alias: "eSUN PLA Plus", printer: "Bambu Lab X2D 0.4 nozzle",
  hex: "#F26B21", synced: true, catalogue_key: null, product: null, parent: null, changes: [], linked: false,
};
const T = { group: "Temperature", editable: true, min: 150, max: 350, step: 5, unit: "°" };
const B = { ...T, min: 0, max: 130 };
const mockProfile: Profile = {
  profile_name: "", alias: "Polymaker Polylite PLA Black", printer: "Bambu Lab X2D 0.4 nozzle", hex: "#101012",
  built_on: "Generic PLA", brand: "Polymaker", product: "Polylite PLA", editable: true, version_id: 2, max_flow: 16,
  settings: [
    { key: "nozzle_temperature", base: "220", value: "215", layer: "community", ...T },
    { key: "nozzle_temperature_initial_layer", base: "220", value: "220", layer: null, ...T },
    { key: "nozzle_temperature_range_low", base: "190", value: "190", layer: null, ...T },
    { key: "nozzle_temperature_range_high", base: "240", value: "230", layer: "manufacturer", ...T },
    { key: "hot_plate_temp", base: "55", value: "55", layer: null, ...B },
    { key: "textured_plate_temp", base: "55", value: "60", layer: "you", ...B },
    { key: "filament_flow_ratio", group: "Flow", base: "0.98", value: "0.96", layer: "community", editable: true, min: 0.5, max: 1.5, step: 0.01, unit: "" },
    { key: "filament_max_volumetric_speed", group: "Flow", base: "12", value: "12", layer: null, editable: true, min: 0.5, max: 60, step: 1, unit: " mm³/s" },
    { key: "pressure_advance", group: "Flow", base: "0.02", value: "0.02", layer: null, editable: true, min: 0, max: 1, step: 0.002, unit: "" },
    { key: "fan_max_speed", group: "Cooling", base: "100", value: "100", layer: null, editable: true, min: 0, max: 100, step: 10, unit: "%" },
    { key: "filament_density", group: "Material", base: "1.24", value: "1.17", layer: "manufacturer", editable: true, min: 0.5, max: 20, step: 0.01, unit: " g/cm³" },
    { key: "default_filament_colour", group: "Other", base: "#FFFFFF", value: "#101012", layer: "colour", editable: false, min: 0, max: 0, step: 0, unit: "" },
  ],
};

/** Browser-only: what the backend would answer for a chosen base and colour, roughly. */
function mockDetail(detail: Detail, base: string | null, colour: string | null, name: string | null, borrowTuned: boolean): Detail {
  const picked = detail.variants.find((v) => v.name === colour);
  const defaultAlias = picked ? picked.alias : detail.alias;
  // ?tuned pretends the maker published settings for the 0.4 mm nozzle only.
  const published = detail.base_printers.filter((p) => p.includes("0.4"));
  const tuned = mockQuery.has("tuned")
    ? { brand: detail.brand, source: "https://example.com/print-settings", retrieved: "2026-11-25", settings: 26, published, own: true, borrowed_from: borrowTuned ? null : null }
    : detail.tuned;
  const hex = detail.variants.find((v) => v.name === colour)?.hex;
  const changes = detail.changes.filter((c) => c.layer !== "colour");
  if (hex) changes.unshift({ key: "default_filament_colour", layer: "colour", from: "", to: hex });
  // Pretend the slicer has silk presets for the 0.4 mm nozzle only, and a maker's flow limit.
  const silk = base?.includes("Silk") ?? false;
  return {
    ...detail,
    tuned,
    default_alias: defaultAlias,
    alias: name && !picked ? name : detail.alias,
    variants: detail.variants.map((v) => (name && v.name === colour ? { ...v, alias: name } : v)),
    changes,
    base_printers: silk ? detail.base_printers.filter((name) => name.includes("0.4")) : detail.base_printers,
    spec: detail.spec.map((row) =>
      row.key === "flow" && row.limit === null ? { ...row, spec: "16 mm³/s", limit: 16, ok: (row.at ?? 0) <= 16 } : row,
    ),
  };
}

export const api = {
  status: async (): Promise<Status> => {
    if (inApp) return invoke("status");
    const status = await mock<Status>("status");
    return mockClosed ? { ...status, running: false } : status;
  },
  catalogue: (refresh: boolean): Promise<CatalogueData> =>
    inApp ? invoke("catalogue", { refresh }) : mock("catalogue"),
  detail: (
    key: string,
    printer: string | null,
    base: string | null,
    colour: string | null,
    name: string | null,
    borrowTuned: boolean,
  ): Promise<Detail> =>
    inApp
      ? invoke("detail", { key, printer, base, colour, name, borrowTuned })
      : mock<Detail>(`detail-${key.split("/").join("~")}`)
          .catch(() =>
            // No dump for this filament: show the sample one under the key that was asked for.
            mock<Detail>("detail-polymaker~PLA~polylite_pla").then((detail) => ({ ...detail, key })),
          )
          .then((detail) => mockDetail(detail, base, colour, name, borrowTuned)),
  install: (request: InstallRequest): Promise<InstallResult> =>
    inApp ? invoke("install", { request }) : mockWrite(request.allow_cloud_sync, "install-dry"),
  exportBundle: (request: InstallRequest): Promise<string[]> =>
    inApp ? invoke("export_bundle", { request }) : Promise.resolve(["~/Downloads/Toona/example.bbsflmt"]),
  inStudio: (): Promise<InStudio> =>
    inApp ? invoke("in_studio") : mock<InStudio>("in_studio").then((data) => ({ ...data, own: data.own ?? [mockOwn] })),
  linkProduct: async (profileName: string, key: string | null): Promise<void> => {
    if (inApp) return invoke("link_product", { profileName, key });
    Object.assign(mockOwn, key
      ? { catalogue_key: key, product: "eSUN PLA+", parent: "Generic PLA", changes: mockShared[0].changes.map((c) => ({ ...c, layer: "you" })) }
      : { catalogue_key: null, product: null, parent: null, changes: [] });
  },
  profile: (profileName: string): Promise<Profile> =>
    inApp
      ? invoke("profile", { profileName })
      : Promise.resolve({ ...mockProfile, profile_name: profileName, alias: profileName.split(" @")[0], editable: profileName !== mockOwn.profile_name }),
  edit: (profileName: string, edits: Edit[], allowCloudSync: boolean): Promise<InstallResult> => {
    if (inApp) return invoke("edit", { profileName, edits, allowCloudSync, dryRun: false });
    return mockWrite<InstallResult>(allowCloudSync, "install-dry").then((result) => {
      for (const edit of edits) {
        const setting = mockProfile.settings.find((s) => s.key === edit.key)!;
        setting.value = edit.value;
        setting.layer = edit.value === setting.base ? null : "you";
      }
      return result;
    });
  },
  family: (versionId: number): Promise<Shared[]> =>
    inApp
      ? invoke("family", { versionId })
      : Promise.resolve([
          ...[...mockShared].reverse(),
          { ...mockShared[1], id: 3, parent_version: 1, author: "benchy_fan", note: "Hotter first layer for a textured plate.", worked: 1, problems: 2, badges: [] },
          { ...mockShared[1], id: 4, parent_version: 2, author: "you", own: true, note: "Slower fan for big parts.", worked: 0, problems: 0, badges: [] },
        ]),
  remove: (profileName: string): Promise<RemoveResult> =>
    inApp
      ? invoke("remove", { profileName })
      : mockWrite(true, "in_studio").then(() => ({ removed: 2, backup: null, cloud_synced: [profileName] })),
  fixImported: (names: string[], allowCloudSync: boolean): Promise<InstallResult> =>
    inApp
      ? invoke("fix_imported", { names, allowCloudSync, dryRun: false })
      : mockWrite(allowCloudSync, "install-dry"),
  barcode: (code: string): Promise<BarcodeMatch[]> =>
    inApp ? invoke("barcode", { code }) : Promise.resolve([]),
  prefs: (): Promise<Prefs> => (inApp ? invoke("prefs") : Promise.resolve({ ...mockPrefs })),
  setPrefs: (updates: boolean, usage: boolean): Promise<Prefs> =>
    inApp ? invoke("set_prefs", { updates, usage }) : Promise.resolve({ ...Object.assign(mockPrefs, { asked: true, updates, usage }) }),
  /** Sends the usage hello if the person opted in and none went today. */
  sendUsage: (): Promise<boolean> => (inApp ? invoke("send_usage") : Promise.resolve(mockPrefs.usage)),
  /** `automatic` is the check on opening, which the backend skips unless the person allowed it. */
  checkUpdate: (automatic: boolean): Promise<Update | null> =>
    inApp
      ? invoke("check_update", { automatic })
      : Promise.resolve(mockQuery.has("update") && (!automatic || mockPrefs.updates) ? mockUpdate : null),
  /** Resolves only on failure: after a successful install the app restarts. */
  installUpdate: (): Promise<void> =>
    inApp ? invoke("install_update") : new Promise((_, reject) => setTimeout(() => reject({ kind: "other", message: "Updates only install in the real app." }), 1500)),
  onUpdateProgress: (heard: (done: number, total: number | null) => void): (() => void) => {
    if (!inApp) return () => undefined;
    const stop = listen<[number, number | null]>("update-progress", (event) => heard(event.payload[0], event.payload[1]));
    return () => void stop.then((off) => off());
  },
  account: (): Promise<Account> =>
    inApp ? invoke("account") : Promise.resolve({ available: true, user: mockSignedIn ? mockUser : null }),
  signInStart: (): Promise<SignIn> =>
    inApp
      ? invoke("sign_in_start")
      : Promise.resolve({ done: false, user: null, user_code: "WDJB-MJHT", verification_uri: "https://github.com/login/device", device_code: "mock", interval: 2 }),
  signInPoll: async (deviceCode: string): Promise<User | null> => {
    if (inApp) return invoke("sign_in_poll", { deviceCode });
    mockSignedIn = true;
    return mockUser;
  },
  signOut: async (): Promise<void> => {
    if (inApp) return invoke("sign_out");
    mockSignedIn = false;
  },
  openSignInPage: async (): Promise<void> => {
    if (inApp) return invoke("open_sign_in_page");
    window.open("https://github.com/login/device", "_blank", "noopener");
  },
  community: (key: string, model: string, nozzle: string): Promise<Shared[]> =>
    inApp ? invoke("community", { key, model, nozzle }) : Promise.resolve(mockShared.map((v) => ({ ...v }))),
  installCommunity: (request: InstallRequest, versionId: number): Promise<InstallResult> =>
    inApp ? invoke("install_community", { request, versionId }) : mockWrite(request.allow_cloud_sync, "install-dry"),
  share: (profileName: string, note: string): Promise<Shared> =>
    inApp
      ? invoke("share", { profileName, note })
      : Promise.resolve({ ...mockShared[1], id: 3, author: "you", own: true, note, worked: 0 }),
  confirm: async (versionId: number, outcome: "worked" | "problems", model: string, nozzle: string): Promise<Shared> => {
    if (inApp) return invoke("confirm", { versionId, outcome, model, nozzle });
    const version = mockShared.find((v) => v.id === versionId)!;
    if (version.mine) version[version.mine] -= 1;
    version[outcome] += 1;
    version.mine = outcome;
    return { ...version };
  },
  reveal: async (path: string): Promise<void> => {
    if (!inApp) return;
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  },
};
