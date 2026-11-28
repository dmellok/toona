import { useState } from "react";
import { CaretDown, Check, DownloadSimple, GitBranch, SealCheck, ThumbsDown, ThumbsUp } from "@phosphor-icons/react";
import type { Shared, Standard } from "./api";
import { Changes } from "./Changes";

interface Props {
  versions: Shared[];
  signedIn: boolean;
  /** Why none of them can be added right now, for example the colour is already in the slicer. */
  blocked: string | null;
  busy: boolean;
  studioName: string;
  onAdd: (version: Shared) => void;
  onConfirm: (version: Shared, outcome: "worked" | "problems") => void;
  /** Shows the version among the ones it was made from and the edits made of it. */
  onHistory: (version: Shared) => void;
}

const BADGES: Record<string, string> = { official: "Official", tested: "Tested" };

const count = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);

interface ScoreProps {
  score: Standard;
  /** The profile is in the slicer, so this person can have printed with it. */
  canAnswer: boolean;
  busy: boolean;
  onConfirm: (outcome: "worked" | "problems") => void;
}

/** How Toona's own profile printed for other people on the chosen printer and nozzle, and a way to say so. */
export function StandardScore({ score, canAnswer, busy, onConfirm }: ScoreProps) {
  const total = score.worked + score.problems;
  const said =
    total === 0
      ? canAnswer
        ? "Printed with it? Say how it went."
        : "No one has said how it printed on this printer yet."
      : `Printed well for ${score.worked} of ${count(total, "person", "people")} on this printer.`;
  return (
    <section className="community" aria-label="How Toona's profile printed">
      <div className="label">Toona's profile</div>
      <div className="standard-score">
        <span>{said}</span>
        {canAnswer && (
          <span className="shared-votes" role="group" aria-label="Did it work for you?">
            <button
              type="button"
              className="icon-button"
              aria-label="It worked"
              title="I printed with this and it worked"
              aria-pressed={score.mine === "worked"}
              disabled={busy}
              onClick={() => onConfirm("worked")}
            >
              <ThumbsUp size={18} aria-hidden />
            </button>
            <button
              type="button"
              className="icon-button"
              aria-label="I had problems"
              title="I printed with this and had problems"
              aria-pressed={score.mine === "problems"}
              disabled={busy}
              onClick={() => onConfirm("problems")}
            >
              <ThumbsDown size={18} aria-hidden />
            </button>
          </span>
        )}
      </div>
    </section>
  );
}

/** Profiles other people shared for this filament on the chosen printer and nozzle. */
export function Community({ versions, signedIn, blocked, busy, studioName, onAdd, onConfirm, onHistory }: Props) {
  const [open, setOpen] = useState<number | null>(null);
  if (versions.length === 0) return null;

  return (
    <section className="community" aria-label="Shared by others">
      <div className="label">Shared by others</div>
      <ul className="shared">
        {versions.map((version) => {
          const badge = version.badges.find((kind) => BADGES[kind]);
          const cannot = version.unusable ?? blocked;
          return (
            <li key={version.id}>
              <div className="shared-head">
                <span className="shared-author">@{version.author}</span>
                {badge && (
                  <span className={`badge badge-${badge}`}>
                    <SealCheck size={14} weight="fill" aria-hidden />
                    {BADGES[badge]}
                  </span>
                )}
                <span className="shared-score">
                  {count(version.worked, "worked", "worked")}
                  {version.problems > 0 ? ` · ${count(version.problems, "problem", "problems")}` : ""}
                </span>
              </div>
              {version.note && <div className="shared-note">{version.note}</div>}
              <div className="shared-actions">
                <button
                  type="button"
                  className="secondary"
                  aria-expanded={open === version.id}
                  onClick={() => setOpen(open === version.id ? null : version.id)}
                >
                  <span>{count(version.changes.length, "change", "changes")}</span>
                  <CaretDown size={14} aria-hidden />
                </button>
                <button type="button" className="icon-button shared-history" aria-label="History" title="History" onClick={() => onHistory(version)}>
                  <GitBranch size={18} aria-hidden />
                </button>
                {version.added ? (
                  <span className="shared-added">
                    <Check size={16} aria-hidden />
                    In {studioName}
                  </span>
                ) : (
                  <button
                    type="button"
                    className="secondary"
                    disabled={busy || cannot !== null}
                    title={cannot ?? undefined}
                    onClick={() => onAdd(version)}
                  >
                    <DownloadSimple size={16} aria-hidden />
                    <span>Add this one</span>
                  </button>
                )}
                {signedIn && !version.own && (
                  <span className="shared-votes" role="group" aria-label="Did it work for you?">
                    <button
                      type="button"
                      className="icon-button"
                      aria-label="It worked"
                      title="I printed with this and it worked"
                      aria-pressed={version.mine === "worked"}
                      disabled={busy}
                      onClick={() => onConfirm(version, "worked")}
                    >
                      <ThumbsUp size={18} aria-hidden />
                    </button>
                    <button
                      type="button"
                      className="icon-button"
                      aria-label="I had problems"
                      title="I printed with this and had problems"
                      aria-pressed={version.mine === "problems"}
                      disabled={busy}
                      onClick={() => onConfirm(version, "problems")}
                    >
                      <ThumbsDown size={18} aria-hidden />
                    </button>
                  </span>
                )}
              </div>
              {open === version.id && (
                <div className="shared-changes">
                  <div className="muted">Built on {version.built_on}, with these changes:</div>
                  <Changes changes={version.changes} />
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
