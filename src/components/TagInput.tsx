import { useState, type KeyboardEvent } from "react";

/** Saisie de mots-clés : Entrée ou virgule ajoute, Retour arrière retire. */
export function TagInput({ value, onChange, label }: { value: string[]; onChange: (v: string[]) => void; label: string }) {
  const [draft, setDraft] = useState("");
  const add = (raw: string) => {
    const tag = raw.trim();
    if (tag && !value.some((v) => v.toLowerCase() === tag.toLowerCase())) onChange([...value, tag]);
    setDraft("");
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      add(draft);
    } else if (e.key === "Backspace" && draft === "" && value.length > 0) {
      onChange(value.slice(0, -1));
    }
  };
  return (
    <div className="tag-input">
      {value.map((tag) => (
        <span key={tag} className="tag">
          {tag}
          <button type="button" aria-label={`× ${tag}`} onClick={() => onChange(value.filter((v) => v !== tag))}>
            ×
          </button>
        </span>
      ))}
      <input aria-label={label} value={draft} onChange={(e) => setDraft(e.target.value)} onKeyDown={onKeyDown} onBlur={() => add(draft)} />
    </div>
  );
}
