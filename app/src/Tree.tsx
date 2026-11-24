import { useEffect, useMemo, useRef, useState } from "react";
import { Warning } from "@phosphor-icons/react";
import type { Shared } from "./api";
import { settingLabel } from "./Changes";

export type Kind = "material" | "manufacturer" | "colour" | "community" | "you";

export const KINDS: Record<Kind, { name: string; colour: string }> = {
  material: { name: "Material", colour: "#6B7076" },
  manufacturer: { name: "Manufacturer", colour: "#15171A" },
  colour: { name: "Colour", colour: "#7C3AED" },
  community: { name: "Community", colour: "#2B4BEE" },
  you: { name: "You", colour: "#0B6B57" },
};

export interface TreeNode {
  id: string;
  parent?: string;
  kind: Kind;
  name: string;
  /** What this node changes over its parent, as short phrases. */
  adds: string[];
  /** Shown when it changes nothing. */
  note?: string;
  warn?: boolean;
}

interface Props {
  nodes: TreeNode[];
  /** The node to start on. */
  start: string;
  /** A button offered on one node, such as "Tune from here". */
  action?: { on: string; label: string; run: () => void };
}

const W = 168;
const H = 52;
const STEP = 212;
const ROW = 68;

/** Shared versions as tree nodes: each lists what it changes over the version it was made from. */
export function versionNodes(versions: Shared[], root?: string): TreeNode[] {
  const byId = new Map(versions.map((v) => [v.id, v]));
  return versions.map((version) => {
    const parent = version.parent_version !== null ? byId.get(version.parent_version) : undefined;
    const before = new Map((parent?.changes ?? []).map((c) => [c.key, c.to]));
    return {
      id: `v${version.id}`,
      // A version whose parent is hidden or missing hangs off the root.
      parent: parent ? `v${parent.id}` : root,
      kind: "community" as const,
      name: `@${version.author}`,
      adds: version.changes.filter((c) => before.get(c.key) !== c.to).map((c) => `${settingLabel(c.key)} ${c.to}`),
      note: version.note || (parent ? `Same settings as @${parent.author}` : undefined),
      warn: version.problems > version.worked,
    };
  });
}

/** A family tree: each node sits to the right of what it builds on, and the path to the picked one is drawn dark. */
export function Tree({ nodes, start, action }: Props) {
  const [picked, setPicked] = useState(start);

  const layout = useMemo(() => {
    const ids = new Set(nodes.map((n) => n.id));
    const children = new Map<string, TreeNode[]>();
    const roots: TreeNode[] = [];
    for (const node of nodes) {
      if (node.parent && ids.has(node.parent)) children.set(node.parent, [...(children.get(node.parent) ?? []), node]);
      else roots.push(node);
    }
    const at = new Map<string, { col: number; y: number }>();
    let row = 0;
    const place = (node: TreeNode, col: number): number => {
      const ys = (children.get(node.id) ?? []).map((kid) => place(kid, col + 1));
      // A parent sits level with the middle of its children.
      const y = ys.length ? (ys[0] + ys[ys.length - 1]) / 2 : row++ * ROW + H / 2;
      at.set(node.id, { col, y });
      return y;
    };
    roots.forEach((root) => place(root, 0));
    const cols = Math.max(0, ...[...at.values()].map((p) => p.col));
    return { at, width: cols * STEP + W, height: Math.max(1, row) * ROW - (ROW - H) };
  }, [nodes]);

  // In a narrow window the tree is wider than the view: start with the picked node in sight.
  const canvas = useRef<HTMLDivElement>(null);
  useEffect(() => {
    canvas.current?.querySelector('[aria-pressed="true"]')?.scrollIntoView({ block: "nearest", inline: "center" });
  }, []);

  const byId = new Map(nodes.map((n) => [n.id, n]));
  const chosen = byId.get(picked) ?? nodes[0];
  const onPath = new Set<string>();
  for (let node: TreeNode | undefined = chosen; node; node = node.parent ? byId.get(node.parent) : undefined) {
    onPath.add(node.id);
  }
  const used = (Object.keys(KINDS) as Kind[]).filter((kind) => nodes.some((n) => n.kind === kind));

  // Lines to nodes on the picked path are drawn last, so they sit on top.
  const links = nodes
    .filter((n) => n.parent && layout.at.has(n.parent))
    .sort((a, b) => Number(onPath.has(a.id)) - Number(onPath.has(b.id)));

  return (
    <div className="tree">
      <div className="tree-legend">
        {used.map((kind) => (
          <span key={kind}>
            <span className="tree-key" style={{ background: KINDS[kind].colour }} aria-hidden />
            {KINDS[kind].name}
          </span>
        ))}
      </div>
      <div className="tree-scroll">
        <div className="tree-canvas" ref={canvas} style={{ width: layout.width, height: layout.height }}>
          <svg width={layout.width} height={layout.height} aria-hidden="true">
            {links.map((node) => {
              const from = layout.at.get(node.parent!)!;
              const to = layout.at.get(node.id)!;
              const x1 = from.col * STEP + W;
              const x2 = to.col * STEP;
              const mid = x1 + (x2 - x1) / 2;
              return (
                <path
                  key={node.id}
                  d={`M${x1} ${from.y} H${mid} V${to.y} H${x2}`}
                  fill="none"
                  stroke={onPath.has(node.id) ? "#15171A" : "#C9C9C3"}
                  strokeWidth="2"
                />
              );
            })}
          </svg>
          {nodes.map((node) => {
            const place = layout.at.get(node.id)!;
            return (
              <button
                key={node.id}
                type="button"
                className={onPath.has(node.id) ? "tree-node" : "tree-node faded"}
                aria-pressed={node.id === chosen.id}
                style={{ left: place.col * STEP, top: place.y - H / 2, width: W, height: H }}
                onClick={() => setPicked(node.id)}
              >
                <span className="tree-key" style={{ background: KINDS[node.kind].colour }} aria-hidden />
                <span className="tree-name">{node.name}</span>
                {node.warn ? (
                  <Warning size={16} className="tree-warn" aria-label="More problems than successes" />
                ) : (
                  node.adds.length > 0 && <span className="tree-count mono">+{node.adds.length}</span>
                )}
              </button>
            );
          })}
        </div>
      </div>
      <div className="tree-detail">
        <div className="tree-detail-head">
          <span className="tree-key" style={{ background: KINDS[chosen.kind].colour }} aria-hidden />
          <span className="tree-detail-name">{chosen.name}</span>
          <span className="caps">{KINDS[chosen.kind].name}</span>
        </div>
        {chosen.adds.length > 0 && chosen.note && <div className="muted">{chosen.note}</div>}
        <div className="chips-row">
          {(chosen.adds.length ? chosen.adds : [chosen.note ?? "No changes"]).map((text) => (
            <span key={text} className="chip-note mono">
              {text}
            </span>
          ))}
        </div>
        {action && action.on === chosen.id && (
          <button type="button" className="pill-button" onClick={action.run}>
            {action.label}
          </button>
        )}
      </div>
    </div>
  );
}
