// Everything the window asks the backend for. Inside the app this goes through
// Tauri; in a plain browser (front-end development) it reads JSON that
// `cargo run -p toona-app --example dump` wrote into public/mock.

import { invoke } from "@tauri-apps/api/core";

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
  layer: "manufacturer" | "colour" | "you";
  from: string | null;
  to: string;
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
  base: Base | null;
  bases: string[];
  /** What adding it would change over the base preset, for the first colour. */
  changes: Change[];
  blocked: string | null;
}

export interface InstallRequest {
  key: string;
  variant: string | null;
  printers: string[];
  base: string | null;
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
}

export interface InStudio {
  added: Added[];
  imported: string[];
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

/** `kind` says which dialog to show; `message` is written for people. */
export interface UiError {
  kind: "running" | "cloud_sync" | "no_slicer" | "offline" | "other";
  message: string;
}

export function asUiError(error: unknown): UiError {
  if (error && typeof error === "object" && "kind" in error && "message" in error) {
    return error as UiError;
  }
  return { kind: "other", message: String(error) };
}

const inApp = "__TAURI_INTERNALS__" in window;

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

export const api = {
  status: async (): Promise<Status> => {
    if (inApp) return invoke("status");
    const status = await mock<Status>("status");
    return mockClosed ? { ...status, running: false } : status;
  },
  catalogue: (refresh: boolean): Promise<CatalogueData> =>
    inApp ? invoke("catalogue", { refresh }) : mock("catalogue"),
  detail: (key: string, printer: string | null, base: string | null): Promise<Detail> =>
    inApp
      ? invoke("detail", { key, printer, base })
      : mock<Detail>(`detail-${key.split("/").join("~")}`).catch(() =>
          mock<Detail>("detail-polymaker~PLA~polylite_pla"),
        ),
  install: (request: InstallRequest): Promise<InstallResult> =>
    inApp ? invoke("install", { request }) : mockWrite(request.allow_cloud_sync, "install-dry"),
  exportBundle: (request: InstallRequest): Promise<string[]> =>
    inApp ? invoke("export_bundle", { request }) : Promise.resolve(["~/Downloads/Toona/example.bbsflmt"]),
  inStudio: (): Promise<InStudio> => (inApp ? invoke("in_studio") : mock("in_studio")),
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
  reveal: async (path: string): Promise<void> => {
    if (!inApp) return;
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  },
};
