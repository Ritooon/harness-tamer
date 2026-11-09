import type { Lang } from "../bindings/Lang";

/** Taille lisible : « 22,1 Go » en français, « 22.1 GB » en anglais. */
export function formatBytes(bytes: number, lang: Lang): string {
  const units = lang === "fr" ? ["o", "Ko", "Mo", "Go", "To"] : ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toLocaleString(lang === "fr" ? "fr-FR" : "en-US", {
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  })} ${units[unit]}`;
}

export function formatDate(iso: string, lang: Lang): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(lang === "fr" ? "fr-FR" : "en-US", { dateStyle: "medium", timeStyle: "short" });
}

/** Retire les marqueurs internes `<!-- hf:… -->` (aperçu, copie). */
export function stripMarkers(markdown: string): string {
  return markdown
    .split("\n")
    .filter((line) => !/^\s*<!--\s*hf:(section\s+id="[^"]*"|end)\s*-->\s*$/.test(line))
    .join("\n")
    .replace(/\n{3,}/g, "\n\n");
}
