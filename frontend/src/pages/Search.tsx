import { useState, useEffect } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Icon } from '../components/Icon';
import { Loading } from '../components/Loading';
import { api, type SearchResult } from '../lib/api';

export function Search() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [query, setQuery] = useState(searchParams.get('q') || '');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [searched, setSearched] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const q = searchParams.get('q');
    if (q) { setQuery(q); doSearch(q); }
  }, [searchParams]);

  async function doSearch(q?: string) {
    const term = q || query;
    if (!term.trim()) return;
    setLoading(true);
    try {
      const res = await api.search(term);
      setResults(res);
      setSearched(true);
    } catch { /* */ }
    setLoading(false);
  }

  return (
    <div className="p-8 max-w-[800px] mx-auto">
      <h1 className="font-headline text-3xl font-bold mb-6">Search</h1>

      {/* Search input */}
      <div className="flex gap-3 mb-8">
        <div className="relative flex-1">
          <Icon name="search" className="absolute left-4 top-1/2 -translate-y-1/2 text-outline text-[20px]" />
          <input
            type="text"
            placeholder="Search the knowledge base..."
            className="w-full bg-surface-container border border-outline-variant/40 rounded-xl pl-12 pr-4 py-3 text-base text-on-surface placeholder:text-outline outline-none focus:border-primary/60"
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && doSearch()}
            autoFocus
          />
        </div>
        <button
          onClick={() => doSearch()}
          className="px-6 py-3 bg-primary text-on-primary rounded-xl font-medium hover:opacity-90 transition-opacity"
        >
          Search
        </button>
      </div>

      {/* Results */}
      {loading && <Loading text="Searching..." />}

      {searched && !loading && (
        <p className="text-sm text-on-surface-variant mb-4">
          Found {results.length} result{results.length !== 1 ? 's' : ''} for "{query}"
        </p>
      )}

      <div className="space-y-3">
        {results.map((r, i) => (
          <div
            key={r.slug}
            className="bg-surface-container rounded-xl p-5 border border-outline-variant/20 cursor-pointer hover:border-primary/40 transition-colors"
            onClick={() => navigate(`/wiki/${r.slug}`)}
          >
            <div className="flex items-start justify-between mb-1">
              <h3 className="font-headline font-semibold text-on-surface">
                <span className="text-on-surface-variant mr-2">{i + 1}.</span>
                {r.title}
              </h3>
              <span className="text-xs bg-surface-high px-2 py-0.5 rounded text-on-surface-variant flex-shrink-0 ml-3">
                {r.score}
              </span>
            </div>
            {r.summary && <p className="text-sm text-on-surface-variant mb-2">{r.summary}</p>}
            {r.snippet && (
              <p className="text-sm text-outline italic">...{r.snippet}...</p>
            )}
          </div>
        ))}
      </div>

      {searched && !loading && results.length === 0 && (
        <div className="text-center py-16 text-on-surface-variant">
          <Icon name="search_off" className="text-5xl mb-3 block" />
          <p>No results found. Try different keywords.</p>
        </div>
      )}
    </div>
  );
}
