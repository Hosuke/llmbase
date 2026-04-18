const BASE = '';

export interface Article {
  slug: string;
  title: string;
  summary: string;
  tags: string[];
  content?: string;
  sources?: { plugin?: string; url?: string; title?: string; work_id?: string }[];
  backlinks?: { slug: string; title: string }[];
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
  link_count: number;
  health_score: number;
}

export interface XiCi {
  text: string;
  themes: string[];
  lang: string;
  generated_at: string | null;
  article_count: number;
}

export interface RawDoc {
  path: string;
  title: string;
  type: string;
  compiled: boolean;
  ingested_at: string;
}

export interface TrailStep {
  type: 'article' | 'query' | 'search';
  slug?: string;
  title?: string;
  question?: string;
  ts: string;
}

export interface Trail {
  id: string;
  name: string;
  created: string;
  updated: string;
  steps: TrailStep[];
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

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

async function post<T>(url: string, body?: unknown): Promise<T> {
  const res = await fetch(BASE + url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new ApiError(res.status, `API error: ${res.status}`);
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
  ask: (
    question: string,
    deep = false,
    fileBack = true,
    tone = 'default',
    promote = false,
  ) =>
    post<{
      answer: string;
      consulted?: { slug: string; title: string }[];
      promotion?: {
        promoted: boolean;
        reason?: string;
        slug?: string;
        title?: string;
        path?: string;
        merged?: boolean;
      };
    }>('/api/ask', { question, deep, file_back: fileBack, tone, promote }),
  getTones: () => get<{ tones: { id: string; label: string; label_zh: string; icon: string }[] }>('/api/tones').then(d => d.tones),
  getAliases: () => get<{ aliases: Record<string, string> }>('/api/aliases').then(d => d.aliases),
  getTrails: () => get<{ trails: Trail[] }>('/api/trails').then(d => d.trails),
  saveTrailStep: (trailId: string | null, step: TrailStep, name?: string) =>
    post<{ trail: Trail }>('/api/trails', { trail_id: trailId, step, name }),
  deleteTrail: (id: string) => post<{ status: string }>(`/api/trails/${id}/delete`, {}),
  getEntities: () => get<{ people: any[]; events: any[]; places: any[]; article_count?: number }>('/api/entities'),
  extractEntities: () => post<{ people: any[]; events: any[]; places: any[] }>('/api/entities/extract', {}),
  getXiCi: (lang: string) => get<XiCi>(`/api/xici?lang=${lang}`),
  generateXiCi: (lang: string) => post<XiCi>('/api/xici/generate', { lang }),
  getSources: () => get<{ documents: RawDoc[] }>('/api/sources').then(d => d.documents),
  ingest: (source: string) => post<{ status: string; path: string }>('/api/ingest', { source }),
  compile: () => post<{ status: string; articles_created: number }>('/api/compile', {}),
  getWorkerStatus: () => get<{ busy: boolean }>('/api/worker/status'),
  lint: (deep = false) => post<{ results?: LintResults; report?: string }>('/api/lint', { deep }),
  lintFix: () => post<{ fixes: string[]; fix_count: number }>('/api/lint/fix', {}),
  cleanWiki: () => post<{ removed: number; slugs: string[] }>('/api/wiki/clean', {}),
  getHealth: () => get<{ report: { checked_at: string; results: LintResults; fixes_applied: string[] } | null }>('/api/health'),
  rebuildIndex: () => post<{ article_count: number }>('/api/index/rebuild', {}),
};
