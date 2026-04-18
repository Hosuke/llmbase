import { useEffect, useRef, useState } from 'react';
import { useTheme } from '../lib/theme';

let mermaidPromise: Promise<typeof import('mermaid').default> | null = null;

function loadMermaid() {
  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then(m => {
      m.default.initialize({ startOnLoad: false, securityLevel: 'strict', fontFamily: 'inherit' });
      return m.default;
    });
  }
  return mermaidPromise;
}

let idCounter = 0;
const nextId = () => `mermaid-${Date.now().toString(36)}-${++idCounter}`;

export function Mermaid({ source }: { source: string }) {
  const { theme } = useTheme();
  const [svg, setSvg] = useState<string>('');
  const [error, setError] = useState<string>('');
  const idRef = useRef<string>(nextId());

  useEffect(() => {
    let cancelled = false;
    setError('');
    setSvg('');
    (async () => {
      try {
        const mermaid = await loadMermaid();
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: 'strict',
          fontFamily: 'inherit',
          theme: theme === 'dark' ? 'dark' : 'default',
        });
        const { svg: rendered } = await mermaid.render(idRef.current, source);
        if (!cancelled) setSvg(rendered);
      } catch (e) {
        // Reset the cached promise on import failure so a transient
        // chunk-load error can recover on next mount instead of locking
        // every diagram into the loading state.
        mermaidPromise = null;
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => { cancelled = true; };
  }, [source, theme]);

  if (error) {
    return (
      <div className="mermaid-error">
        <div className="text-xs opacity-70 mb-1">mermaid render error: {error}</div>
        <pre><code>{source}</code></pre>
      </div>
    );
  }
  if (!svg) {
    return <div className="mermaid-loading shimmer" style={{ minHeight: 48 }} />;
  }
  return <div className="mermaid" dangerouslySetInnerHTML={{ __html: svg }} />;
}
