import { createContext, useCallback, useContext, useMemo, type ReactNode } from "react";

import type { Lang } from "../bindings/Lang";
import { isCmdError } from "../lib/api";
import { en } from "./en";
import { fr, type MessageKey } from "./fr";

const dictionaries: Record<Lang, Record<MessageKey, string>> = { fr, en };

export type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string;

/** Remplace `{nom}` par la valeur correspondante. */
export function format(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => (name in vars ? String(vars[name]) : match));
}

export function translate(lang: Lang, key: MessageKey, vars?: Record<string, string | number>): string {
  return format(dictionaries[lang][key] ?? key, vars);
}

/** Message lisible pour une erreur de commande (code stable → texte traduit). */
export function errorMessage(lang: Lang, error: unknown): string {
  if (isCmdError(error)) {
    const key = `error.${error.code}` as MessageKey;
    if (key in dictionaries[lang]) return translate(lang, key, { message: error.message });
    return translate(lang, "error.generic", { message: error.message });
  }
  return translate(lang, "error.generic", { message: String(error) });
}

type I18n = { lang: Lang; t: Translate; err: (error: unknown) => string };

const I18nContext = createContext<I18n>({
  lang: "fr",
  t: (key, vars) => translate("fr", key, vars),
  err: (e) => errorMessage("fr", e),
});

export function I18nProvider({ lang, children }: { lang: Lang; children: ReactNode }) {
  const t = useCallback<Translate>((key, vars) => translate(lang, key, vars), [lang]);
  const err = useCallback((e: unknown) => errorMessage(lang, e), [lang]);
  const value = useMemo(() => ({ lang, t, err }), [lang, t, err]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  return useContext(I18nContext);
}
