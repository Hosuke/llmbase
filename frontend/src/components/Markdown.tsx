import { useState, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useNavigate } from 'react-router-dom';
import type { Components } from 'react-markdown';
import { api } from '../lib/api';

// Global alias cache — loaded once, shared across all Markdown instances
let aliasCache: Record<string, string> | null = null;
let aliasFetchPromise: Promise<Record<string, string>> | null = null;

function getAliases(): Promise<Record<string, string>> {
  if (aliasCache) return Promise.resolve(aliasCache);
  if (!aliasFetchPromise) {
    aliasFetchPromise = api.getAliases()
      .then(a => { aliasCache = a; return a; })
      .catch(() => { aliasCache = {}; return {}; });
  }
  return aliasFetchPromise;
}

/** Resolve a wiki-link target to a canonical slug using aliases */
function resolveTarget(target: string, aliases: Record<string, string>): string {
  const key = target.trim().toLowerCase();
  if (aliases[key]) return aliases[key];
  // Fallback: simple normalization for ASCII slugs
  return key.replace(/\s+/g, '-');
}

// Transform wiki-links [[target|label]] before passing to react-markdown
function transformWikiLinks(text: string, aliases: Record<string, string>): string {
  return text.replace(/\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]/g, (_, target, label) => {
    const slug = resolveTarget(target, aliases);
    return `[${label || target}](/wiki/${slug})`;
  });
}

export function Markdown({ content, className = '' }: { content: string; className?: string }) {
  const navigate = useNavigate();
  const [aliases, setAliases] = useState<Record<string, string>>(aliasCache || {});

  useEffect(() => {
    if (!aliasCache) {
      getAliases().then(setAliases);
    }
  }, []);

  const transformed = transformWikiLinks(content, aliases);

  const components: Components = {
    a({ href, children }) {
      if (href?.startsWith('/wiki/')) {
        return (
          <span
            className="wiki-link"
            onClick={(e) => { e.preventDefault(); navigate(href); }}
          >
            {children}
          </span>
        );
      }
      return <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>;
    },
  };

  return (
    <div className={`prose-article ${className}`}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {transformed}
      </ReactMarkdown>
    </div>
  );
}
