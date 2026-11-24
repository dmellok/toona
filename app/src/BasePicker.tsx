import { useEffect, useMemo, useRef, useState } from "react";
import { Check, MagnifyingGlass } from "@phosphor-icons/react";
import type { Base } from "./api";

interface Props {
  /** The preset Toona would choose by itself. */
  auto: Base | null;
  /** Every preset the slicer offers for the printer. */
  bases: string[];
  /** The preset in use when the user chose one; null for the automatic choice. */
  value: string | null;
  /** Names of bases chosen before, most recent first. */
  recent: string[];
  onPick: (preset: string | null) => void;
  onClose: () => void;
}

interface Option {
  id: string;
  preset: string | null;
  label: string;
  note?: string;
  group: string;
}

const presetLabel = (preset: string) => preset.split(" @")[0];

/** Choosing the preset a profile is built on, by typing part of its name. */
export function BasePicker({ auto, bases, value, recent, onPick, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLUListElement>(null);

  const options = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const matches = (label: string) => words.every((word) => label.toLowerCase().includes(word));
    const out: Option[] = [];
    const autoLabel = auto ? auto.label : "Automatic";
    if (matches(`automatic ${autoLabel}`)) {
      out.push({ id: "auto", preset: null, label: autoLabel, note: auto ? `Automatic · ${auto.source}` : "Automatic", group: "Suggested" });
    }
    const used = new Set<string>();
    for (const name of recent) {
      const preset = bases.find((base) => presetLabel(base) === name);
      if (preset && matches(name) && !used.has(preset)) {
        used.add(preset);
        out.push({ id: `recent ${preset}`, preset, label: name, group: "Used before" });
      }
    }
    for (const preset of bases) {
      const label = presetLabel(preset);
      if (!used.has(preset) && matches(label)) out.push({ id: preset, preset, label, group: "All presets for this printer" });
    }
    return out;
  }, [auto, bases, recent, query]);

  // The panel's Add button stays pinned over its lower edge; open clear of it.
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // In a block: newer browsers return a promise from scrollIntoView, which an effect must not return.
    box.current?.scrollIntoView({ block: "center" });
  }, []);

  useEffect(() => setActive(0), [query]);
  useEffect(() => {
    list.current?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const onKey = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((current) => Math.min(options.length - 1, Math.max(0, current + step)));
    } else if (event.key === "Enter" && options[active]) {
      event.preventDefault();
      onPick(options[active].preset);
    } else if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
  };

  return (
    <div className="base-picker" ref={box}>
      <div className="base-search">
        <MagnifyingGlass size={18} aria-hidden />
        <input
          id="base-search"
          autoFocus
          role="combobox"
          aria-label="Base profile"
          aria-expanded="true"
          aria-controls="base-options"
          aria-activedescendant={options[active] ? `base-option-${active}` : undefined}
          value={query}
          placeholder="Type to search, such as PETG HF"
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKey}
        />
      </div>
      <ul className="base-options" id="base-options" role="listbox" ref={list}>
        {options.length === 0 && <li className="base-none muted">No preset matches.</li>}
        {options.map((option, index) => {
          const chosen = option.preset === value;
          return (
            <li key={option.id} role="presentation">
              {(index === 0 || options[index - 1].group !== option.group) && <div className="base-group caps">{option.group}</div>}
              <button
                type="button"
                id={`base-option-${index}`}
                role="option"
                aria-selected={chosen}
                data-active={index === active}
                tabIndex={-1}
                onMouseEnter={() => setActive(index)}
                onClick={() => onPick(option.preset)}
              >
                <span>{option.label}</span>
                {option.note && <span className="muted">{option.note}</span>}
                {chosen && <Check size={16} aria-label="In use" />}
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
