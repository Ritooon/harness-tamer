// Texte produit en direct par le modèle (et sa réflexion, repliée).
import { useI18n } from "../i18n";

export function StreamView({ text, reasoning }: { text: string; reasoning: string }) {
  const { t } = useI18n();
  return (
    <div className="stream">
      {reasoning && (
        <details>
          <summary>{t("agents.thinking")}</summary>
          <pre className="reasoning">{reasoning}</pre>
        </details>
      )}
      <pre className="stream-text" aria-live="polite">
        {text}
      </pre>
    </div>
  );
}
