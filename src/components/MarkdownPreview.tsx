// Aperçu Markdown. Le HTML brut est ignoré (les marqueurs internes
// `<!-- hf:… -->` restent invisibles), aucune image n'est chargée (une image
// distante serait une requête réseau) et les liens https passent par le
// dialogue de consentement ; les autres schémas sont neutralisés.
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { api } from "../lib/api";

export function MarkdownPreview({ source, onError }: { source: string; onError?: (e: unknown) => void }) {
  return (
    <div className="md-preview">
      <Markdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        components={{
          a: ({ href, children }) => (
            <a
              href={href}
              onClick={(event) => {
                event.preventDefault();
                if (href && /^https:\/\//i.test(href)) {
                  api.openExternalLink(href).catch((e: unknown) => onError?.(e));
                }
              }}
            >
              {children}
            </a>
          ),
          img: ({ alt }) => <span className="md-image-placeholder">[{alt || "image"}]</span>,
        }}
      >
        {source}
      </Markdown>
    </div>
  );
}
