import type { DiffLine } from "../bindings/DiffLine";

export function DiffView({ lines }: { lines: DiffLine[] }) {
  return (
    <pre className="diff">
      {lines.map((line, i) => (
        <div key={i} className={`diff-${line.tag}`}>
          <span className="diff-sign">{line.tag === "insert" ? "+" : line.tag === "delete" ? "−" : " "}</span>
          {line.text}
        </div>
      ))}
    </pre>
  );
}
