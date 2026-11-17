const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** Mesmas regras de `report::human_bytes` (CLI): unidades binárias até TB. */
export function humanBytes(n: number): string {
  let v = n;
  let unit = 0;
  while (v >= 1024 && unit < UNITS.length - 1) {
    v /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${v.toFixed(1)} ${UNITS[unit]}`;
}

const dateFormat = new Intl.DateTimeFormat("pt-BR", {
  dateStyle: "short",
  timeStyle: "short",
});

export function humanDate(rfc3339: string): string {
  const d = new Date(rfc3339);
  return Number.isNaN(d.getTime()) ? rfc3339 : dateFormat.format(d);
}
