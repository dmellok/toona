import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CaretDown, GithubLogo, Printer as PrinterIcon, Warning } from "@phosphor-icons/react";
import {
  api,
  asUiError,
  type Account,
  type Added,
  type Card,
  type CatalogueData,
  type Detail,
  type Edit,
  type Own,
  type Profile,
  type InStudio,
  type InstallRequest,
  type Shared,
  type Status,
} from "./api";
import { Browse } from "./Browse";
import { DetailPanel } from "./DetailPanel";
import { InStudioView } from "./InStudioView";
import { Logo } from "./Logo";
import { ProfileView } from "./ProfileView";
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

/** A small remembered value, such as the bases chosen before. */
function recall<T>(key: string, fallback: T): T {
  try {
    const text = localStorage.getItem(key);
    return text ? (JSON.parse(text) as T) : fallback;
  } catch {
    return fallback;
  }
}

function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // As above: a convenience only.
  }
}

/** How many bases the picker offers as "used before". */
const RECENT_BASES = 5;

export default function App() {
  const [view, setView] = useState<"browse" | "studio">("browse");
  const [status, setStatus] = useState<Status | null>(null);
  const [catalogue, setCatalogue] = useState<CatalogueData | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [inStudio, setInStudio] = useState<InStudio | null>(null);
  const [advanced, setAdvanced] = useState(() => stored("toona.advanced"));
  /** In a narrow window the selected filament covers the list; this says whether it is showing. */
  const [detailOpen, setDetailOpen] = useState(false);

  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [detail, setDetail] = useState<Detail | null>(null);
  const [colour, setColour] = useState<Record<string, string>>({});
  /** The base chosen for a filament, kept between sessions. */
  const [baseOverride, setBaseOverride] = useState<Record<string, string>>(() => recall("toona.bases", {}));
  const [recentBases, setRecentBases] = useState<string[]>(() => recall("toona.recentBases", []));
  const [model, setModel] = useState<string | null>(null);
  /** The nozzles ticked for each printer model, kept between sessions. */
  const [nozzles, setNozzles] = useState<Record<string, string[]>>(() => recall("toona.nozzles", {}));

  const [account, setAccount] = useState<Account | null>(null);
  /** Shared profiles for one filament; shown only while that filament is the one on screen. */
  const [shared, setShared] = useState<{ key: string; versions: Shared[] }>({ key: "", versions: [] });

  /** The profile whose own page is open over the In Studio list. */
  const [profile, setProfile] = useState<Profile | null>(null);

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
    api.account().then(setAccount).catch(() => undefined);
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
  const chosen = ((activeModel && nozzles[activeModel]) || []).filter((n) => offered.includes(n));
  const fallback = (modelPrinters.find((p) => p.selected) ?? modelPrinters[0])?.nozzle;
  const activeNozzles = chosen.length ? chosen : fallback ? [fallback] : [];
  const primaryPrinter = modelPrinters.find((p) => activeNozzles.includes(p.nozzle))?.name ?? null;

  // First filament shown: the first in the catalogue.
  useEffect(() => {
    if (!selectedKey && catalogue?.items.length) setSelectedKey(catalogue.items[0].key);
  }, [catalogue, selectedKey]);

  const chosenBase = selectedKey ? (baseOverride[selectedKey] ?? null) : null;
  const chosenColour = selectedKey ? (colour[selectedKey] ?? null) : null;

  // The panel keeps showing the previous filament until the next one has loaded, so
  // switching never blanks it. Only the most recent request may fill it.
  const detailRequest = useRef(0);
  const loadDetail = useCallback(() => {
    if (!selectedKey) return;
    const request = ++detailRequest.current;
    api
      .detail(selectedKey, primaryPrinter, chosenBase, chosenColour)
      .then((next) => request === detailRequest.current && setDetail(next))
      .catch(() => request === detailRequest.current && setDetail(null));
  }, [selectedKey, primaryPrinter, chosenBase, chosenColour]);

  useEffect(loadDetail, [loadDetail]);

  const community = account?.available ?? false;
  const user = account?.user ?? null;
  const firstNozzle = activeNozzles[0] ?? null;

  const sharedRequest = useRef(0);
  const loadShared = useCallback(() => {
    if (!community || !selectedKey || !activeModel || !firstNozzle) return;
    const request = ++sharedRequest.current;
    api
      .community(selectedKey, activeModel, firstNozzle)
      .then((versions) => request === sharedRequest.current && setShared({ key: selectedKey, versions }))
      .catch(() => request === sharedRequest.current && setShared({ key: selectedKey, versions: [] }));
    // `user` is here so each person's own answers are loaded after signing in or out.
  }, [community, selectedKey, activeModel, firstNozzle, user?.id]);

  useEffect(loadShared, [loadShared]);

  const afterChange = useCallback(() => {
    refreshInStudio();
    loadDetail();
    loadShared();
    api.catalogue(false).then(setCatalogue).catch(() => undefined);
  }, [refreshInStudio, loadDetail, loadShared]);

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

  // No colour is chosen until the user picks one; a filament can be added without.
  const variant = detail ? (detail.variants.find((v) => v.name === colour[detail.key]) ?? null) : null;

  // A base the user chose may not exist for every nozzle; those nozzles are left out.
  const baseFor = detail && detail.key === selectedKey ? detail.base_printers : [];
  const targets = modelPrinters.filter(
    (p) => activeNozzles.includes(p.nozzle) && (baseFor.length === 0 || baseFor.includes(p.name)),
  );

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

  /** Counts up on every sign-in attempt, so a cancelled one stops being polled. */
  const signInAttempt = useRef(0);

  /** Signs in with GitHub, then runs `then` if given. */
  const signIn = async (then?: () => void) => {
    const attempt = ++signInAttempt.current;
    const finish = (signedIn: Account["user"]) => {
      setAccount({ available: true, user: signedIn });
      setSheet(null);
      then?.();
    };
    try {
      const start = await api.signInStart();
      if (start.done) return finish(start.user);
      setSheet({
        kind: "sign-in",
        code: start.user_code,
        cancel: () => {
          signInAttempt.current += 1;
          setSheet(null);
        },
      });
      let wait = Math.max(start.interval, 2) * 1000;
      while (attempt === signInAttempt.current) {
        await new Promise((resolve) => setTimeout(resolve, wait));
        if (attempt !== signInAttempt.current) return;
        const signedIn = await api.signInPoll(start.device_code);
        if (signedIn) return finish(signedIn);
        // GitHub asks for slower polling if it is asked too often; stay well clear of that.
        wait = Math.min(wait + 500, 10_000);
      }
    } catch (raw) {
      if (attempt === signInAttempt.current) setSheet({ kind: "error", message: asUiError(raw).message });
    }
  };

  const signOut = () => {
    void api.signOut().finally(() => {
      setAccount({ available: true, user: null });
      setSheet(null);
    });
  };

  const openProfile = (profileName: string) => {
    api
      .profile(profileName)
      .then(setProfile)
      .catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }));
  };

  const saveEdits = (edits: Edit[], alsoShare: boolean) => {
    if (!profile) return;
    const { profile_name: name, alias } = profile;
    void write(async (allowSync) => {
      const result = await api.edit(name, edits, allowSync);
      const saved = await api.profile(name);
      setProfile(saved);
      if (alsoShare) {
        const changes = saved.settings
          .filter((s) => s.layer !== null)
          .map((s) => ({ key: s.key, layer: s.layer!, from: s.base, to: s.value }));
        const row: Added = { profile_name: name, alias, catalogue_key: "", printer: saved.printer, hex: saved.hex, synced: false, present: true, parent: saved.built_on, changes, linked: saved.version_id !== null };
        // After this turn, so the share question replaces the "Saved" sheet.
        setTimeout(() => share(row), 0);
      }
      return { kind: "saved", label: alias, changes: edits.length, backup: result.backup };
    }, syncOk);
  };

  const linkProduct = (row: Own) =>
    setSheet({
      kind: "link",
      label: row.alias,
      items: (catalogue?.items ?? []).filter((item) => !item.resin),
      current: row.catalogue_key,
      pick: (key) => {
        setSheet(null);
        api
          .linkProduct(row.profile_name, key)
          .then(refreshInStudio)
          .catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }));
      },
    });

  const showHistory = (version: Shared) => {
    api
      .family(version.id)
      .then((versions) => setSheet({ kind: "tree", versions, current: version.id }))
      .catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }));
  };

  // `signedIn` is passed when sharing continues straight after signing in, because
  // this function still holds the signed-out state it was created with.
  const share = (row: Added | Own, signedIn = user !== null) => {
    if (!signedIn) return void signIn(() => share(row, true));
    setSheet({
      kind: "share",
      label: row.alias,
      changes: row.changes.filter((change) => change.layer !== "colour").length,
      isEdit: row.linked ?? false,
      confirm: async (note) => {
        setBusy(true);
        try {
          await api.share(row.profile_name, note);
          setSheet({ kind: "shared", label: row.alias });
          refreshInStudio();
          loadShared();
        } catch (raw) {
          const error = asUiError(raw);
          if (error.kind === "sign_in") {
            setAccount({ available: true, user: null });
            void signIn(() => share(row, true));
          } else setSheet({ kind: "error", message: error.message });
        } finally {
          setBusy(false);
        }
      },
    });
  };

  const addShared = (version: Shared) => {
    const wanted = request(false);
    if (!wanted) return;
    const hex = variant?.hex ?? null;
    const colourName = variant?.name ?? detail!.name;
    void write(async (allowSync) => {
      const result = await api.installCommunity({ ...wanted, allow_cloud_sync: allowSync }, version.id);
      return { kind: "added", alias: result.alias, colourName, hex, backup: result.backup };
    }, syncOk);
  };

  const confirmShared = async (version: Shared, outcome: "worked" | "problems") => {
    if (!activeModel || !firstNozzle) return;
    try {
      const updated = await api.confirm(version.id, outcome, activeModel, firstNozzle);
      setShared((current) => ({ ...current, versions: current.versions.map((v) => (v.id === updated.id ? updated : v)) }));
    } catch (raw) {
      setSheet({ kind: "error", message: asUiError(raw).message });
    }
  };

  const selectFromBarcode = (key: string, variantName: string) => {
    setSelectedKey(key);
    setDetailOpen(true);
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
          <button type="button" className="tab" aria-pressed={view === "browse"} onClick={() => { setView("browse"); setProfile(null); }}>
            Browse
          </button>
          <button type="button" className="tab" aria-pressed={view === "studio"} onClick={() => { setView("studio"); setProfile(null); }}>
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
            <span className="mono">{(targets.length ? targets.map((p) => p.nozzle) : activeNozzles).join(", ")} mm</span>
            {models.length > 1 && (
              <>
                <CaretDown size={14} aria-hidden />
                <select
                  aria-label="Printer"
                  value={activeModel}
                  onChange={(event) => {
                    setModel(event.target.value);
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

        {community && (
          <button
            type="button"
            className="pill"
            onClick={() => (user ? setSheet({ kind: "account", user, signOut }) : void signIn())}
          >
            <GithubLogo size={18} aria-hidden />
            <span>{user ? `@${user.handle}` : "Sign in"}</span>
          </button>
        )}

        <button
          type="button"
          className="pill switch"
          role="switch"
          aria-checked={advanced}
          onClick={() => {
            store("toona.advanced", !advanced);
            setAdvanced(!advanced);
          }}
        >
          <span>Advanced</span>
          <span className="switch-track" aria-hidden>
            <span className="switch-thumb" />
          </span>
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
            onSelect={(key) => {
              setSelectedKey(key);
              setDetailOpen(true);
            }}
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
              onColour={(name) =>
                setColour((current) => {
                  const next = { ...current };
                  if (name) next[detail.key] = name;
                  else delete next[detail.key];
                  return next;
                })
              }
              advanced={advanced}
              studioName={studioName}
              studioFound={status?.found ?? false}
              printers={modelPrinters}
              activeNozzles={activeNozzles}
              onNozzles={(next) => {
                if (!activeModel) return;
                const all = { ...nozzles, [activeModel]: next };
                setNozzles(all);
                remember("toona.nozzles", all);
              }}
              recentBases={recentBases}
              base={baseOverride[detail.key] ?? null}
              onBase={(preset) => {
                const next = { ...baseOverride };
                if (preset) next[detail.key] = preset;
                else delete next[detail.key];
                setBaseOverride(next);
                remember("toona.bases", next);
                if (preset) {
                  const name = preset.split(" @")[0];
                  const recent = [name, ...recentBases.filter((other) => other !== name)].slice(0, RECENT_BASES);
                  setRecentBases(recent);
                  remember("toona.recentBases", recent);
                }
              }}
              busy={busy}
              onInstall={install}
              onRemove={() => remove(variant?.alias ?? detail.alias, variant?.alias ?? detail.alias)}
              onExport={exportBundle}
              shared={shared.key === detail.key ? shared.versions : []}
              signedIn={user !== null}
              onAddShared={addShared}
              onConfirm={confirmShared}
              onHistory={showHistory}
              open={detailOpen}
              onClose={() => setDetailOpen(false)}
            />
          )}
        </main>
      ) : profile ? (
        <main className="view">
          <ProfileView
            profile={profile}
            studioName={studioName}
            busy={busy}
            community={community}
            onBack={() => setProfile(null)}
            onSave={saveEdits}
          />
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
            onShare={community ? share : null}
            onOpen={openProfile}
            onLink={linkProduct}
          />
        </main>
      )}

      <Sheets sheet={sheet} studioName={studioName} close={() => setSheet(null)} />
    </div>
  );
}
