import { exerciseTitle } from "../exercises";
import { formatDate, type ExerciceResultat } from "../model";

const W = 320;
const H = 130;
const PAD = { left: 34, right: 10, top: 10, bottom: 22 };

export function percent(r: Pick<ExerciceResultat, "score" | "total">): number {
  return r.total > 0 ? Math.round((100 * r.score) / r.total) : 0;
}

/** Séries par exercice (au moins deux passations), de la plus ancienne à la plus récente. */
export function series(results: readonly ExerciceResultat[]): [string, ExerciceResultat[]][] {
  const by = new Map<string, ExerciceResultat[]>();
  for (const r of results) by.set(r.exercice, [...(by.get(r.exercice) ?? []), r]);
  return [...by.entries()]
    .filter(([, list]) => list.length >= 2)
    .map(([id, list]) => [id, [...list].sort((a, b) => a.date.localeCompare(b.date))] as [string, ExerciceResultat[]]);
}

function Chart({ id, list }: { id: string; list: ExerciceResultat[] }) {
  const first = list[0];
  const last = list[list.length - 1];
  if (!first || !last) return null;
  const x = (i: number) => PAD.left + (i * (W - PAD.left - PAD.right)) / Math.max(1, list.length - 1);
  const y = (p: number) => PAD.top + ((100 - p) * (H - PAD.top - PAD.bottom)) / 100;
  const points = list.map((r, i) => `${x(i)},${y(percent(r))}`).join(" ");
  const delta = percent(last) - percent(first);
  const summary = `${exerciseTitle(id)} : ${percent(first)} % le ${formatDate(first.date)}, ${percent(last)} % le ${formatDate(last.date)} (${delta >= 0 ? "+" : ""}${delta} points), ${list.length} passations.`;
  return (
    <figure className="progress-chart">
      <figcaption>
        <strong>{exerciseTitle(id)}</strong>{" "}
        <span className={delta > 0 ? "badge ok" : "badge"}>
          {delta >= 0 ? "+" : ""}
          {delta} points
        </span>
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={summary}>
        {[0, 50, 100].map((p) => (
          <g key={p}>
            <line className="axis" x1={PAD.left} x2={W - PAD.right} y1={y(p)} y2={y(p)} />
            <text className="tick" x={PAD.left - 6} y={y(p) + 4} textAnchor="end">
              {p} %
            </text>
          </g>
        ))}
        <polyline className="line" points={points} />
        {list.map((r, i) => (
          <circle key={r.id ?? i} className="dot" cx={x(i)} cy={y(percent(r))} r={4}>
            <title>{`${formatDate(r.date)} · niveau ${r.niveau} · ${r.score}/${r.total}`}</title>
          </circle>
        ))}
        <text className="tick" x={PAD.left} y={H - 6}>
          {formatDate(first.date)}
        </text>
        <text className="tick" x={W - PAD.right} y={H - 6} textAnchor="end">
          {formatDate(last.date)}
        </text>
      </svg>
    </figure>
  );
}

/** Courbes de progression des exercices d'un patient. */
export function Progression({ results }: { results: readonly ExerciceResultat[] }) {
  const all = series(results);
  if (all.length === 0) return null;
  return (
    <div className="progress-grid" aria-label="Progression par exercice">
      {all.map(([id, list]) => (
        <Chart key={id} id={id} list={list} />
      ))}
    </div>
  );
}
