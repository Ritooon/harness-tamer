import type { Issue } from "../bindings/Issue";
import type { SectionId } from "../bindings/SectionId";
import { useI18n } from "../i18n";
import type { MessageKey } from "../i18n/fr";

export function IssuesList({ issues }: { issues: Issue[] }) {
  const { t } = useI18n();
  if (issues.length === 0) return <p className="ok-text">{t("agents.no_issues")}</p>;
  return (
    <ul className="issues">
      {issues.map((issue, i) => (
        <li key={i} className={`issue issue-${issue.kind}`}>
          {issue.section && <strong>{t(`section.${issue.section as SectionId}` as MessageKey)} — </strong>}
          {t(`issue.${issue.kind}` as MessageKey, { detail: issue.detail })}
        </li>
      ))}
    </ul>
  );
}
