// Petits composants d'interface partagés.
import { useEffect, useRef, type ReactNode } from "react";

import { useI18n } from "../i18n";

export function Button({
  children,
  onClick,
  kind = "secondary",
  disabled,
  type = "button",
  title,
}: {
  children: ReactNode;
  onClick?: () => void;
  kind?: "primary" | "secondary" | "danger" | "ghost";
  disabled?: boolean;
  type?: "button" | "submit";
  title?: string;
}) {
  return (
    <button type={type} className={`btn btn-${kind}`} onClick={onClick} disabled={disabled} title={title}>
      {children}
    </button>
  );
}

export function Field({ label, hint, error, children }: { label: string; hint?: string; error?: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint && !error && <span className="field-hint">{hint}</span>}
      {error && (
        <span className="field-error" role="alert">
          {error}
        </span>
      )}
    </label>
  );
}

export function Banner({ kind, children, onClose }: { kind: "error" | "info" | "success" | "warning"; children: ReactNode; onClose?: () => void }) {
  const { t } = useI18n();
  return (
    <div className={`banner banner-${kind}`} role={kind === "error" ? "alert" : "status"}>
      <div className="banner-body">{children}</div>
      {onClose && (
        <button type="button" className="banner-close" onClick={onClose} aria-label={t("common.close")}>
          ×
        </button>
      )}
    </div>
  );
}

export function Modal({ title, children, onClose, wide }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const ref = useRef<HTMLDialogElement>(null);
  const { t } = useI18n();
  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open && typeof dialog.showModal === "function") dialog.showModal();
  }, []);
  return (
    <dialog ref={ref} className={`modal ${wide ? "modal-wide" : ""}`} onCancel={onClose} aria-label={title}>
      <header className="modal-header">
        <h2>{title}</h2>
        <button type="button" className="banner-close" onClick={onClose} aria-label={t("common.close")}>
          ×
        </button>
      </header>
      <div className="modal-body">{children}</div>
    </dialog>
  );
}

export function Tabs<T extends string>({ tabs, active, onChange }: { tabs: { id: T; label: string }[]; active: T; onChange: (id: T) => void }) {
  return (
    <div className="tabs" role="tablist">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={tab.id === active}
          className={`tab ${tab.id === active ? "tab-active" : ""}`}
          onClick={() => onChange(tab.id)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

export function Progress({ value, max, label }: { value: number; max: number | null; label?: string }) {
  const percent = max ? Math.min(100, Math.round((value / max) * 100)) : null;
  return (
    <div className="progress" aria-label={label}>
      <div className={`progress-bar ${percent === null ? "progress-indeterminate" : ""}`} style={{ width: `${percent ?? 30}%` }} />
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="empty">{children}</p>;
}
