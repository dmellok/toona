import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowCircleUp, CaretDown, GearSix, GithubLogo, Printer as PrinterIcon, Warning } from "@phosphor-icons/react";
import {
  api,
  asUiError,
  type Account,
  type Added,
  type Backup,
  type Card,
  type CatalogueData,
  type Detail,
  type Edit,
  type Own,
  type Prefs,
  type Update,
  type Profile,
  type InStudio,
  type InstallRequest,
  type Shared,
  type Standard,
  type Status,
} from "./api";
import { Browse } from "./Browse";
import { DetailPanel } from "./DetailPanel";
import { filamentCount, InStudioView, isProduct, NO_FILTERS, place, sharedKey, shortModel, type ProductLink, type Removal, type StudioFilters } from "./InStudioView";
import { guesses } from "./match";
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
  /** Names the person gave profiles, by filament and colour, kept between sessions. */
  const [names, setNames] = useState<Record<string, string>>(() => recall("toona.names", {}));
  /** Use a manufacturer's settings for one nozzle on the others it published nothing for. */
  const [borrowTuned, setBorrowTuned] = useState(() => stored("toona.borrowTuned"));
  const [model, setModel] = useState<string | null>(null);
  /** The nozzles ticked for each printer model, kept between sessions. */
  const [nozzles, setNozzles] = useState<Record<string, string[]>>(() => recall("toona.nozzles", {}));

  const [account, setAccount] = useState<Account | null>(null);
  /** Shared profiles for one filament; shown only while that filament is the one on screen. */
  const [shared, setShared] = useState<{ key: string; versions: Shared[] }>({ key: "", versions: [] });
  /** How Toona's own profile for the selected filament did on the chosen printer and nozzle. */
  const [standard, setStandard] = useState<{ key: string; score: Standard | null }>({ key: "", score: null });

  /** What the In Studio list is narrowed to. All but the search is kept between sessions. */
  const [studioFilters, setStudioFilters] = useState<StudioFilters>(() => ({
    ...NO_FILTERS,
    ...recall<Partial<StudioFilters>>("toona.studio", {}),
    query: "",
  }));
  /** How many profiles other people shared for the products in the slicer, by product, printer and nozzle. */
  const [sharedCounts, setSharedCounts] = useState<Record<string, number>>({});
  const sharedAsked = useRef(new Set<string>());

  /** The profile whose own page is open over the In Studio list. */
  const [profile, setProfile] = useState<Profile | null>(null);

  const [sheet, setSheet] = useState<Sheet | null>(null);
  const [busy, setBusy] = useState(false);
  /** The user ticked "Don't ask again" on the cloud-sync question. */
  const [syncOk, setSyncOk] = useState(() => stored("toona.syncOk"));

  const [prefs, setPrefs] = useState<Prefs | null>(null);
  /** A newer version of Toona that is waiting to be installed. */
  const [update, setUpdate] = useState<Update | null>(null);

  /** What Toona does by itself when it opens, each only if the person allowed it. */
  const background = useCallback(() => {
    api.sendUsage().catch(() => undefined);
    api.checkUpdate(true).then(setUpdate).catch(() => undefined);
  }, []);

  const offerUpdate = (found: Update) =>
    setSheet({
      kind: "update",
      update: found,
      install: () => {
        setSheet({ kind: "updating", version: found.version });
        // Only comes back if it failed: a finished install restarts the app.
        api.installUpdate().catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }));
      },
    });

  const openPrefs = (current: Prefs, first: boolean, review = false) =>
    setSheet({
      kind: "prefs",
      first,
      review,
      prefs: current,
      save: (updates, usage) => {
        api
          .setPrefs(updates, usage)
          .then((next) => {
            setPrefs(next);
            setSheet(null);
            if (!updates) setUpdate(null);
            background();
          })
          .catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }));
      },
      backups: first ? null : showBackups,
      check: first
        ? null
        : () => {
            api
              .checkUpdate(false)
              .then((found) => {
                setUpdate(found);
                if (found) offerUpdate(found);
                else setSheet({ kind: "current", version: current.version });
              })
              .catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }));
          },
    });

  // Nothing is checked or sent until the first-run question has an answer.
  useEffect(() => {
    api
      .prefs()
      .then((current) => {
        setPrefs(current);
        if (!current.asked) openPrefs(current, true);
        else {
          background();
          // What usage numbers hold has changed since they said yes; ask again before sending the new part.
          if (current.review) openPrefs(current, false, true);
        }
      })
      .catch(() => undefined);
    // Runs once, when the window opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
  const nameKey = `${selectedKey ?? ""}|${chosenColour ?? ""}`;
  const chosenName = names[nameKey] ?? null;

  // The panel keeps showing the previous filament until the next one has loaded, so
  // switching never blanks it. Only the most recent request may fill it.
  const detailRequest = useRef(0);
  const loadDetail = useCallback(() => {
    if (!selectedKey) return;
    const request = ++detailRequest.current;
    api
      .detail(selectedKey, primaryPrinter, chosenBase, chosenColour, chosenName, borrowTuned)
      .then((next) => request === detailRequest.current && setDetail(next))
      .catch(() => request === detailRequest.current && setDetail(null));
  }, [selectedKey, primaryPrinter, chosenBase, chosenColour, chosenName, borrowTuned]);

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
    api
      .standard(selectedKey, activeModel, firstNozzle)
      .then((score) => request === sharedRequest.current && setStandard({ key: selectedKey, score }))
      .catch(() => request === sharedRequest.current && setStandard({ key: selectedKey, score: null }));
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
          alias: chosenName,
          borrow_tuned: borrowTuned,
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

  /** Takes out what Toona added. Each name is a filament name or one profile's full name. */
  const remove = (names: string[], label: string) =>
    setSheet({
      kind: "confirm-remove",
      label,
      confirm: () =>
        write(async () => {
          let stillInCloud = false;
          for (const name of names) stillInCloud = (await api.remove(name)).cloud_synced.length > 0 || stillInCloud;
          return { kind: "removed", label, stillInCloud };
        }, true),
    });

  /** Puts the slicer's folder back as it was in a backup. The answer offers to undo that. */
  const restoreNow = (path: string): Promise<void> =>
    write(async () => {
      const before = await api.restoreBackup(path);
      return { kind: "restored", undo: () => void restoreNow(before) };
    }, true);

  function showBackups() {
    api
      .backups()
      .then((backups) =>
        setSheet({
          kind: "backups",
          backups,
          restore: (backup: Backup) => setSheet({ kind: "confirm-restore", backup, confirm: () => void restoreNow(backup.path) }),
        }),
      )
      .catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }));
  }

  /** Takes profiles out by their full names, behind one backup: Toona's and the person's own. */
  const removeProfiles = ({ profileNames, label, own, synced }: Removal) =>
    setSheet({
      kind: "confirm-remove",
      label,
      own,
      synced,
      confirm: () =>
        write(async () => {
          const result = await api.removeMany(profileNames);
          const backup = result.backup;
          return { kind: "removed", label, stillInCloud: result.cloud_synced.length > 0, undo: backup ? () => void restoreNow(backup) : null };
        }, true),
    });

  /** Adds a filament that is in the slicer for one nozzle to more of them. */
  const addNozzles = (profileName: string, printerNames: string[], label: string) =>
    void write(async (allowSync) => {
      const result = await api.addNozzles(profileName, printerNames, borrowTuned, allowSync);
      return { kind: "added", alias: label, colourName: label, hex: null, backup: result.backup };
    }, syncOk);

  const seeShared = (key: string) => {
    setSelectedKey(key);
    setDetailOpen(true);
    setProfile(null);
    setView("browse");
  };

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

  /** Notes which product some of the person's own filaments are; a null key forgets it. */
  const linkNow = (links: { profileNames: string[]; key: string | null }[]) => {
    const pairs = links.flatMap((link) => link.profileNames.map((name) => [name, link.key] as const));
    Promise.all(pairs.map(([name, key]) => api.linkProduct(name, key)))
      .catch((raw) => setSheet({ kind: "error", message: asUiError(raw).message }))
      .finally(refreshInStudio);
  };

  const linkProduct = (profileNames: string[], label: string, current: string | null) => {
    const products = (catalogue?.items ?? []).filter((item) => !item.resin);
    setSheet({
      kind: "link",
      label,
      items: products,
      likely: guesses(label, products).slice(0, 3).map((guess) => guess.item),
      current,
      pick: (key) => {
        setSheet(null);
        linkNow([{ profileNames, key }]);
      },
    });
  };

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

  // `signedIn` is passed when this continues straight after signing in, as with sharing.
  const confirmStandard = async (outcome: "worked" | "problems", signedIn = user !== null) => {
    if (!selectedKey || !activeModel || !firstNozzle) return;
    if (!signedIn) return void signIn(() => confirmStandard(outcome, true));
    const key = selectedKey;
    try {
      const score = await api.confirmStandard(key, outcome, activeModel, firstNozzle);
      setStandard({ key, score });
    } catch (raw) {
      setSheet({ kind: "error", message: asUiError(raw).message });
    }
  };

  // Ask once for each product, printer and nozzle in the slicer how many profiles were shared for it.
  useEffect(() => {
    if (!community || !inStudio) return;
    for (const row of [...inStudio.added, ...(inStudio.own ?? [])]) {
      const key = row.catalogue_key;
      const { model, nozzle } = place(row.printer, printers);
      if (!isProduct(key) || !nozzle) continue;
      const id = sharedKey(key, model, nozzle);
      if (sharedAsked.current.has(id)) continue;
      sharedAsked.current.add(id);
      api
        .community(key, model, nozzle)
        .then((versions) => setSharedCounts((counts) => ({ ...counts, [id]: versions.filter((v) => !v.own).length })))
        .catch(() => undefined);
    }
  }, [community, inStudio, printers]);

  const selectFromBarcode = (key: string, variantName: string) => {
    setSelectedKey(key);
    setDetailOpen(true);
    setColour((current) => ({ ...current, [key]: variantName }));
  };

  const items: Card[] = catalogue?.items ?? [];
  const studioName = status?.slicer ?? "Bambu Studio";
  const slicerId = status?.id ?? "bambu-studio";
  // Every slicer is offered, found or not: one Toona can't find can be pointed out by hand.
  const slicers = status?.slicers ?? [];
  const addedCount = filamentCount(inStudio);

  // Printers, what is added and every open page belong to the slicer that was left.
  const adoptSlicer = (next: Status) => {
    setStatus(next);
    setModel(null);
    setProfile(null);
    loadCatalogue(false);
    refreshInStudio();
  };
  const slicerFailed = (raw: unknown) => setSheet({ kind: "error", message: asUiError(raw).message });

  const switchSlicer = (id: string) => {
    api.setSlicer(id).then(adoptSlicer).catch(slicerFailed);
  };

  /** Shows where a slicer's settings are looked for, and lets the person choose the folder. */
  const findSlicer = (id: string) => {
    const slicer = slicers.find((each) => each.id === id);
    if (!slicer) return;
    setSheet({
      kind: "find-slicer",
      name: slicer.name,
      folder: slicer.folder,
      found: slicer.found,
      choose: () => {
        api
          .chooseSlicerFolder(id)
          .then((next) => {
            if (!next) return;
            setSheet(null);
            adoptSlicer(next);
          })
          .catch(slicerFailed);
      },
      reset: slicer.chosen
        ? () => {
            api
              .resetSlicerFolder(id)
              .then((next) => {
                setSheet(null);
                adoptSlicer(next);
              })
              .catch(slicerFailed);
          }
        : null,
    });
  };

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
            <span>{slicerId === "bambu-studio" ? "In Studio" : `In ${studioName}`}</span>
            {addedCount > 0 && <span className="count">{addedCount}</span>}
          </button>
        </nav>

        <div className="spacer" />

        {status && !status.found && (
          <button type="button" className="notice notice-action" onClick={() => findSlicer(slicerId)}>
            <Warning size={18} aria-hidden />
            <span>{studioName} not found. Choose its folder</span>
          </button>
        )}
        {status?.found && status.running && (
          <div className="notice">
            <Warning size={18} aria-hidden />
            <span>Close {studioName} to make changes</span>
          </div>
        )}

        {update && (
          <button type="button" className="pill update" onClick={() => offerUpdate(update)}>
            <ArrowCircleUp size={18} aria-hidden />
            <span>Update to {update.version}</span>
          </button>
        )}

        {slicers.length > 0 && (
          <div className="pill pill-select">
            <span>{studioName}</span>
            <CaretDown size={14} aria-hidden />
            <select
              aria-label="Slicer"
              value={slicerId}
              onChange={(event) => (event.target.value === "folder" ? findSlicer(slicerId) : switchSlicer(event.target.value))}
            >
              {slicers.map((slicer) => (
                <option key={slicer.id} value={slicer.id}>
                  {slicer.found ? slicer.name : `${slicer.name} (not found)`}
                </option>
              ))}
              <option value="folder">Settings folder for {studioName}…</option>
            </select>
          </div>
        )}

        {activeModel && (
          <div className="pill pill-select">
            <PrinterIcon size={18} aria-hidden />
            <span>{shortModel(activeModel)}</span>
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

        {prefs && (
          <button type="button" className="pill icon-only" aria-label="Settings" title="Settings" onClick={() => openPrefs(prefs, false)}>
            <GearSix size={18} aria-hidden />
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
              name={chosenName}
              onName={(next) => {
                const all = { ...names };
                if (next) all[nameKey] = next;
                else delete all[nameKey];
                setNames(all);
                remember("toona.names", all);
              }}
              borrowTuned={borrowTuned}
              onBorrowTuned={(on) => {
                store("toona.borrowTuned", on);
                setBorrowTuned(on);
              }}
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
              onRemove={() => remove([variant?.alias ?? detail.alias], variant?.alias ?? detail.alias)}
              onExport={exportBundle}
              shared={shared.key === detail.key ? shared.versions : []}
              signedIn={user !== null}
              standard={standard.key === detail.key ? standard.score : null}
              onConfirmStandard={confirmStandard}
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
            advanced={advanced}
            items={items}
            printers={printers}
            activeModel={activeModel}
            filters={studioFilters}
            onFilters={(next) => {
              setStudioFilters(next);
              remember("toona.studio", { ...next, query: "" });
            }}
            sharedCounts={sharedCounts}
            onAddNozzles={addNozzles}
            onSeeShared={seeShared}
            onBrowse={() => setView("browse")}
            onRemove={removeProfiles}
            onFix={fix}
            onShare={community ? share : null}
            onOpen={openProfile}
            onLink={linkProduct}
            onLinkNow={(links: ProductLink[]) => linkNow(links)}
          />
        </main>
      )}

      <Sheets sheet={sheet} studioName={studioName} slicerId={slicerId} close={() => setSheet(null)} />
    </div>
  );
}
