const BASE = '';

export interface Article {
  slug: string;
  title: string;
  summary: string;
  tags: string[];
  content?: string;
}

export interface SearchResult {
  slug: string;
  title: string;
  summary: string;
  score: number;
  snippet: string;
  matched_terms: string[];
}

export interface Stats {
  raw_count: number;
  article_count: number;
  output_count: number;
  total_words: number;
}

export interface RawDoc {
  path: string;
  title: string;
  type: string;
  compiled: boolean;
  ingested_at: string;
}

export interface LintResults {
  structural: string[];
  broken_links: string[];
  orphans: string[];
  missing_metadata: string[];
  total_issues: number;
}

async function get<T>(url: string): Promise<T> {
  const res = await fetch(BASE + url);
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}

async function post<T>(url: string, body?: unknown): Promise<T> {
  const res = await fetch(BASE + url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}

export interface Collection {
  id: string;
  label: string;
  count: number;
  articles: { slug: string; title: string; summary: string }[];
}

export interface TaxonomyCategory {
  id: string;
  label: string;
  count: number;
  total: number;
  articles: { slug: string; title: string }[];
  children: TaxonomyCategory[];
}

export const api = {
  getCollections: () => get<{ collections: Collection[] }>('/api/collections').then(d => d.collections),
  getTaxonomy: (lang: string) => get<{ categories: TaxonomyCategory[] }>(`/api/taxonomy?lang=${lang}`).then(d => d.categories),
  getStats: () => get<Stats>('/api/stats'),
  getArticles: () => get<{ articles: Article[] }>('/api/articles').then(d => d.articles),
  getArticle: (slug: string) => get<Article>('/api/articles/' + slug),
  search: (q: string, topK = 10) => get<{ results: SearchResult[] }>(`/api/search?q=${encodeURIComponent(q)}&top_k=${topK}`).then(d => d.results),
  ask: (question: string, deep = false, fileBack = true, tone = 'default') => post<{ answer: string }>('/api/ask', { question, deep, file_back: fileBack, tone }),
  getTones: () => get<{ tones: { id: string; label: string; label_zh: string; icon: string }[] }>('/api/tones').then(d => d.tones),
  getSources: () => get<{ documents: RawDoc[] }>('/api/sources').then(d => d.documents),
  ingest: (source: string) => post<{ status: string; path: string }>('/api/ingest', { source }),
  compile: () => post<{ status: string; articles_created: number }>('/api/compile', {}),
  lint: (deep = false) => post<{ results?: LintResults; report?: string }>('/api/lint', { deep }),
  rebuildIndex: () => post<{ article_count: number }>('/api/index/rebuild', {}),
};
