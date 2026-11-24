import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CaretDown, Printer as PrinterIcon, SlidersHorizontal, Warning } from "@phosphor-icons/react";
import {
  api,
  asUiError,
  type Card,
  type CatalogueData,
  type Detail,
  type InStudio,
  type InstallRequest,
  type Status,
} from "./api";
import { Browse } from "./Browse";
import { DetailPanel } from "./DetailPanel";
import { InStudioView } from "./InStudioView";
import { Logo } from "./Logo";
import { Sheets, type Sheet } from "./Sheets";

/** How often to check whether the slicer is open while the window is in use. */
const STATUS_INTERVAL_MS = 5000;

function stored(key: string): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

function store(key: string, on: boolean) {
  try {
    localStorage.setItem(key, on ? "1" : "0");
  } catch {
    // Storage is a convenience; the app works without it.
  }
}

export default function App() {
  const [view, setView] = useState<"browse" | "studio">("browse");
  const [status, setStatus] = useState<Status | null>(null);
  const [catalogue, setCatalogue] = useState<CatalogueData | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [inStudio, setInStudio] = useState<InStudio | null>(null);
  const [advanced, setAdvanced] = useState(() => stored("toona.advanced"));

  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [detail, setDetail] = useState<Detail | null>(null);
  const [colour, setColour] = useState<Record<string, string>>({});
  const [baseOverride, setBaseOverride] = useState<Record<string, string>>({});
  const [model, setModel] = useState<string | null>(null);
  const [nozzles, setNozzles] = useState<string[] | null>(null);

  const [sheet, setSheet] = useState<Sheet | null>(null);
  const [busy, setBusy] = useState(false);
  /** The user ticked "Don't ask again" on the cloud-sync question. */
  const [syncOk, setSyncOk] = useState(() => stored("toona.syncOk"));

  const refreshStatus = useCallback(() => {
    api.status().then(setStatus).catch(() => undefined);
  }, []);

  const refreshInStudio = useCallback(() => {
    api.inStudio().then(setInStudio).catch(() => undefined);
  }, []);

  const loadCatalogue = useCallback((refresh: boolean) => {
    setLoadError(null);
    api
      .catalogue(refresh)
      .then(setCatalogue)
      .catch((error) => setLoadError(asUiError(error).message));
  }, []);

  useEffect(() => {
    refreshStatus();
    refreshInStudio();
    loadCatalogue(false);
    const timer = setInterval(refreshStatus, STATUS_INTERVAL_MS);
    window.addEventListener("focus", refreshStatus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", refreshStatus);
    };
  }, [refreshStatus, refreshInStudio, loadCatalogue]);

  // Printer choice: the model selected in the slicer, and its selected nozzle.
  const printers = status?.printers ?? [];
  const models = useMemo(() => [...new Set(printers.map((p) => p.model))], [printers]);
  const activeModel = model ?? printers.find((p) => p.selected)?.model ?? models[0] ?? null;
  const modelPrinters = printers.filter((p) => p.model === activeModel);
  const offered = modelPrinters.map((p) => p.nozzle);
  const chosen = (nozzles ?? []).filter((n) => offered.includes(n));
  const fallback = (modelPrinters.find((p) => p.selected) ?? modelPrinters[0])?.nozzle;
  const activeNozzles = chosen.length ? chosen : fallback ? [fallback] : [];
  const targets = modelPrinters.filter((p) => activeNozzles.includes(p.nozzle));
  const primaryPrinter = targets[0]?.name ?? null;

  // First filament shown: the first in the catalogue.
  useEffect(() => {
    if (!selectedKey && catalogue?.items.length) setSelectedKey(catalogue.items[0].key);
  }, [catalogue, selectedKey]);

  const chosenBase = selectedKey ? (baseOverride[selectedKey] ?? null) : null;

  // The panel keeps showing the previous filament until the next one has loaded, so
  // switching never blanks it. Only the most recent request may fill it.
  const detailRequest = useRef(0);
  const loadDetail = useCallback(() => {
    if (!selectedKey) return;
    const request = ++detailRequest.current;
    api
      .detail(selectedKey, primaryPrinter, chosenBase)
      .then((next) => request === detailRequest.current && setDetail(next))
      .catch(() => request === detailRequest.current && setDetail(null));
  }, [selectedKey, primaryPrinter, chosenBase]);

  useEffect(loadDetail, [loadDetail]);

  const afterChange = useCallback(() => {
    refreshInStudio();
    loadDetail();
    api.catalogue(false).then(setCatalogue).catch(() => undefined);
  }, [refreshInStudio, loadDetail]);

  /** Runs something that writes to the slicer's folder, turning its refusals into the right sheet. */
  const write = useCallback(
    async (run: (allowSync: boolean) => Promise<Sheet>, allowSync: boolean) => {
      setBusy(true);
      try {
        const done = await run(allowSync);
        setSheet(done);
        afterChange();
      } catch (raw) {
        const error = asUiError(raw);
        if (error.kind === "running") {
          setSheet({ kind: "running", retry: () => write(run, allowSync) });
          refreshStatus();
        } else if (error.kind === "cloud_sync") {
          setSheet({
            kind: "sync",
            confirm: (dontAsk) => {
              if (dontAsk) {
                setSyncOk(true);
                store("toona.syncOk", true);
              }
              return write(run, true);
            },
          });
        } else {
          setSheet({ kind: "error", message: error.message });
        }
      } finally {
        setBusy(false);
      }
    },
    [afterChange, refreshStatus],
  );

  const variant = detail
    ? (detail.variants.find((v) => v.name === colour[detail.key]) ?? detail.variants[0] ?? null)
    : null;

  const request = (allowSync: boolean): InstallRequest | null =>
    detail && targets.length
      ? {
          key: detail.key,
          variant: variant?.name ?? null,
          printers: targets.map((p) => p.name),
          base: baseOverride[detail.key] ?? null,
          allow_cloud_sync: allowSync,
          dry_run: false,
        }
      : null;

  const install = () => {
    if (!request(false)) return;
    const hex = variant?.hex ?? null;
    const colourName = variant?.name ?? detail!.name;
    void write(async (allowSync) => {
      const result = await api.install(request(allowSync)!);
      return { kind: "added", alias: result.alias, colourName, hex, backup: result.backup };
    }, syncOk);
  };

  const exportBundle = async () => {
    const wanted = request(true);
    if (!wanted) return;
    setBusy(true);
    try {
      const files = await api.exportBundle(wanted);
      setSheet({ kind: "exported", files });
    } catch (raw) {
      setSheet({ kind: "error", message: asUiError(raw).message });
    } finally {
      setBusy(false);
    }
  };

  const remove = (name: string, label: string) =>
    setSheet({
      kind: "confirm-remove",
      label,
      confirm: () =>
        write(async () => {
          const result = await api.remove(name);
          return { kind: "removed", label, stillInCloud: result.cloud_synced.length > 0 };
        }, true),
    });

  const fix = (names: string[]) =>
    void write(async (allowSync) => {
      const result = await api.fixImported(names, allowSync);
      return { kind: "fixed", names: result.profiles.map((p) => p.name) };
    }, syncOk);

  const selectFromBarcode = (key: string, variantName: string) => {
    setSelectedKey(key);
    setColour((current) => ({ ...current, [key]: variantName }));
  };

  const items: Card[] = catalogue?.items ?? [];
  const studioName = status?.slicer ?? "Bambu Studio";
  const addedCount = inStudio?.added.length ?? 0;

  return (
    <div className="shell">
      <header className="header">
        <div className="brand">
          <Logo size={30} />
          <span>Toona</span>
        </div>

        <nav className="tabs" aria-label="Views">
          <button type="button" className="tab" aria-pressed={view === "browse"} onClick={() => setView("browse")}>
            Browse
          </button>
          <button type="button" className="tab" aria-pressed={view === "studio"} onClick={() => setView("studio")}>
            <span>In Studio</span>
            {addedCount > 0 && <span className="count">{addedCount}</span>}
          </button>
        </nav>

        <div className="spacer" />

        {status && !status.found && (
          <div className="notice">
            <Warning size={18} aria-hidden />
            <span>{studioName} not found</span>
          </div>
        )}
        {status?.found && status.running && (
          <div className="notice">
            <Warning size={18} aria-hidden />
            <span>{studioName} is open</span>
          </div>
        )}

        {activeModel && (
          <div className="pill pill-select">
            <PrinterIcon size={18} aria-hidden />
            <span>{activeModel.replace(/^Bambu Lab /, "")}</span>
            <span className="mono">{activeNozzles.join(", ")} mm</span>
            {models.length > 1 && (
              <>
                <CaretDown size={14} aria-hidden />
                <select
                  aria-label="Printer"
                  value={activeModel}
                  onChange={(event) => {
                    setModel(event.target.value);
                    setNozzles(null);
                  }}
                >
                  {models.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </>
            )}
          </div>
        )}

        <button
          type="button"
          className="pill"
          aria-pressed={advanced}
          onClick={() => {
            store("toona.advanced", !advanced);
            setAdvanced(!advanced);
          }}
        >
          <SlidersHorizontal size={18} aria-hidden />
          <span>Advanced</span>
        </button>
      </header>

      {view === "browse" ? (
        <main className="view browse">
          <Browse
            items={items}
            loading={!catalogue && !loadError}
            loadError={loadError}
            retry={() => loadCatalogue(false)}
            selectedKey={selectedKey}
            onSelect={setSelectedKey}
            onBarcode={selectFromBarcode}
            advanced={advanced}
            version={catalogue?.version ?? null}
            warning={catalogue?.warning ?? null}
            refresh={() => loadCatalogue(true)}
          />
          {detail && (
            <DetailPanel
              detail={detail}
              variant={variant}
              onColour={(name) => setColour((current) => ({ ...current, [detail.key]: name }))}
              advanced={advanced}
              studioName={studioName}
              studioFound={status?.found ?? false}
              printers={modelPrinters}
              activeNozzles={activeNozzles}
              onNozzles={setNozzles}
              base={baseOverride[detail.key] ?? null}
              onBase={(preset) =>
                setBaseOverride((current) => {
                  const next = { ...current };
                  if (preset) next[detail.key] = preset;
                  else delete next[detail.key];
                  return next;
                })
              }
              busy={busy}
              onInstall={install}
              onRemove={() => variant && remove(variant.alias, variant.alias)}
              onExport={exportBundle}
            />
          )}
        </main>
      ) : (
        <main className="view">
          <InStudioView
            data={inStudio}
            studioName={studioName}
            busy={busy}
            onBrowse={() => setView("browse")}
            onRemove={(row) => remove(row.profile_name, row.alias)}
            onFix={fix}
          />
        </main>
      )}

      <Sheets sheet={sheet} studioName={studioName} close={() => setSheet(null)} />
    </div>
  );
}
