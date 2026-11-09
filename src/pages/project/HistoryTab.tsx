// Historique : chaque version est une copie complète ; restaurer crée une
// nouvelle version, rien n'est écrasé.
import { useState } from "react";

import type { DiffLine } from "../../bindings/DiffLine";
import type { SectionId } from "../../bindings/SectionId";
import type { VersionMeta } from "../../bindings/VersionMeta";
import { DiffView } from "../../components/DiffView";
import { MarkdownPreview } from "../../components/MarkdownPreview";
import { Banner, Button, Empty } from "../../components/ui";
import { useI18n, type Translate } from "../../i18n";
import type { MessageKey } from "../../i18n/fr";
import { api } from "../../lib/api";
import { formatDate, stripMarkers } from "../../lib/format";
import type { ProjectDetail } from "../../lib/types";

export function originLabel(v: VersionMeta, t: Translate): string {
  switch (v.origin) {
    case "generation":
      return t("history.origin.generation");
    case "section_regeneration":
      return t("history.origin.section_regeneration", {
        sections: v.sections.map((s) => t(`section.${s as SectionId}` as MessageKey)).join(", "),
      });
    case "manual_edit":
      return t("history.origin.manual_edit");
    case "restore":
      return t("history.origin.restore", { from: v.restored_from ?? "?" });
  }
}

export function HistoryTab({ detail, onChanged }: { detail: ProjectDetail; onChanged: () => void }) {
  const { t, lang, err } = useI18n();
  const id = detail.project.id;
  const [selected, setSelected] = useState<number | null>(null);
  const [view, setView] = useState<{ kind: "content"; text: string } | { kind: "diff"; lines: DiffLine[] } | null>(null);
  const [message, setMessage] = useState<{ kind: "error" | "success"; text: string } | null>(null);
  const current = detail.versions[0]?.number;

  const show = async (number: number, kind: "content" | "diff") => {
    setSelected(number);
    try {
      setView(
        kind === "content"
          ? { kind, text: stripMarkers(await api.readVersion(id, number)) }
          : { kind, lines: await api.diffVersions(id, number, null) },
      );
    } catch (e) {
      setMessage({ kind: "error", text: err(e) });
    }
  };

  const restore = async (number: number) => {
    try {
      const v = await api.restoreVersion(id, number);
      setMessage({ kind: "success", text: t("history.restored", { from: number, number: v.number }) });
      setView(null);
      onChanged();
    } catch (e) {
      setMessage({ kind: "error", text: err(e) });
    }
  };

  if (detail.versions.length === 0) return <Empty>{t("history.empty")}</Empty>;
  return (
    <div className="history">
      {message && (
        <Banner kind={message.kind} onClose={() => setMessage(null)}>
          {message.text}
        </Banner>
      )}
      <ol className="versions">
        {detail.versions.map((v) => (
          <li key={v.number} className={selected === v.number ? "selected" : ""}>
            <div>
              <strong>{t("history.version", { number: v.number })}</strong> · {formatDate(v.created_at, lang)}
              <div className="muted small">
                {originLabel(v, t)}
                {v.model && ` · ${t("history.model", { model: v.model })}`}
              </div>
              {v.note && <div className="small">« {v.note} »</div>}
            </div>
            <div className="inline">
              <Button kind="ghost" onClick={() => show(v.number, "content")}>
                {t("history.view")}
              </Button>
              {v.number !== current && (
                <>
                  <Button kind="ghost" onClick={() => show(v.number, "diff")}>
                    {t("history.diff")}
                  </Button>
                  <Button onClick={() => restore(v.number)}>{t("history.restore")}</Button>
                </>
              )}
            </div>
          </li>
        ))}
      </ol>
      {view && <div className="panel">{view.kind === "content" ? <MarkdownPreview source={view.text} /> : <DiffView lines={view.lines} />}</div>}
    </div>
  );
}
