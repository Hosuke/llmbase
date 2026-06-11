import { useState, useEffect, useMemo } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Tag } from '../components/Tag';
import { Markdown } from '../components/Markdown';
import { Loading } from '../components/Loading';
import { Icon } from '../components/Icon';
import { useLang, localizeTitle, extractLangContent } from '../lib/lang';
import { useTrail } from '../lib/trail';
import { api, type Article } from '../lib/api';

export function ArticleDetail() {
  const { slug } = useParams<{ slug: string }>();
  const navigate = useNavigate();
  const [article, setArticle] = useState<Article | null>(null);
  const [allArticles, setAllArticles] = useState<Article[]>([]);
  const [loading, setLoading] = useState(true);
  const { lang, contract } = useLang();
  const { recordStep } = useTrail();

  // Auto-record article visit to trail
  useEffect(() => {
    if (article && slug) {
      recordStep({ type: 'article', slug, title: article.title });
    }
  }, [slug, article?.title]);

  useEffect(() => {
    if (!slug) return;
    setLoading(true);
    Promise.all([
      api.getArticle(slug).catch(async () => {
        // If direct slug fails, try alias resolution via backend
        // (backend /api/articles/<slug> already resolves aliases,
        // but URL-encoded CJK might need decoding)
        const decoded = decodeURIComponent(slug);
        if (decoded !== slug) {
          return api.getArticle(decoded).catch(() => null);
        }
        return null;
      }),
      api.getArticles(),
    ]).then(([a, all]) => {
      if (a && 'slug' in a && a.slug !== slug) {
        // Backend resolved to a different slug — redirect
        navigate(`/wiki/${a.slug}`, { replace: true });
      }
      setArticle(a);
      setAllArticles(all);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, [slug]);

  const displayContent = useMemo(() => {
    if (!article?.content) return '';
    return extractLangContent(article.content, lang);
  }, [article?.content, lang, contract]);

  // Extract headings for TOC from displayed content
  const headings = useMemo(() => {
    return [...displayContent.matchAll(/^(#{1,3})\s+(.+)$/gm)].map(m => ({
      level: m[1].length,
      text: m[2],
      id: m[2].toLowerCase().replace(/[^\w]+/g, '-'),
    }));
  }, [displayContent]);

  const related = useMemo(() => {
    if (!article?.tags) return [];
    return allArticles.filter(a =>
      a.slug !== slug && a.tags?.some(t => article.tags.includes(t))
    ).slice(0, 5);
  }, [article, allArticles, slug]);

  if (loading) return <Loading text="Loading article..." />;
  if (!article) return <div className="p-8 text-center text-on-surface-variant">Article not found</div>;

  return (
    <div className="flex">
      {/* Article content */}
      <div className="flex-1 p-8 max-w-[780px] mx-auto">
        {/* Breadcrumb */}
        <div className="flex items-center gap-2 text-sm text-on-surface-variant mb-6">
          <span className="cursor-pointer hover:text-primary" onClick={() => navigate('/wiki')}>Wiki</span>
          <span>/</span>
          <span className="text-on-surface">{article.title}</span>
        </div>

        {/* Header */}
        <h1 className="font-headline text-3xl font-bold mb-3">{localizeTitle(article.title, lang)}</h1>
        {article.summary && (
          <p className="text-on-surface-variant font-body text-lg italic mb-4">{article.summary}</p>
        )}
        <div className="flex flex-wrap items-center gap-2 mb-4">
          {article.tags?.map(t => <Tag key={t} label={t} />)}
        </div>

        <hr className="border-outline-variant/30 mb-8" />

        {/* Content */}
        <Markdown content={displayContent} />

        {/* Sources / References */}
        {article.sources && article.sources.length > 0 && (
          <div className="mt-10 pt-6 border-t border-outline-variant/30">
            <h3 className="text-xs uppercase tracking-widest text-on-surface-variant mb-3 flex items-center gap-2">
              <Icon name="menu_book" className="text-[14px]" />
              {lang === 'zh' || lang === 'zh-en' ? '引用来源' : 'Sources'}
            </h3>
            <div className="space-y-2">
              {article.sources.map((src: { plugin?: string; url?: string; title?: string; work_id?: string }, i: number) => (
                <div key={i} className="flex items-start gap-2 text-sm">
                  <span className="px-1.5 py-0.5 text-[10px] bg-primary/10 text-primary rounded font-mono flex-shrink-0">
                    {src.plugin || 'ref'}
                  </span>
                  {src.url ? (
                    <a href={src.url} target="_blank" rel="noopener noreferrer"
                      className="text-on-surface-variant hover:text-primary transition-colors truncate">
                      {src.title || src.url}
                      {src.work_id && <span className="text-outline ml-1">({src.work_id})</span>}
                    </a>
                  ) : (
                    <span className="text-on-surface-variant">{src.title || 'Unknown source'}</span>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Right sidebar */}
      <aside className="w-[240px] border-l border-outline-variant/30 p-5 hidden lg:block flex-shrink-0 sticky top-0 h-screen overflow-y-auto">
        {/* TOC */}
        {headings.length > 0 && (
          <div className="mb-8">
            <h4 className="text-xs uppercase tracking-widest text-on-surface-variant mb-3">On this page</h4>
            <nav className="space-y-1">
              {headings.map((h, i) => (
                <a
                  key={i}
                  href={`#${h.id}`}
                  className="block text-sm text-on-surface-variant hover:text-primary transition-colors truncate"
                  style={{ paddingLeft: `${(h.level - 1) * 12}px` }}
                >
                  {h.text}
                </a>
              ))}
            </nav>
          </div>
        )}

        {/* Backlinks — "Cited by" */}
        {article.backlinks && article.backlinks.length > 0 && (
          <div className="mb-8">
            <h4 className="text-xs uppercase tracking-widest text-on-surface-variant mb-3 flex items-center gap-1.5">
              <Icon name="link" className="text-[12px]" />
              {lang === 'zh' || lang === 'zh-en' ? '被引用' : 'Cited by'} ({article.backlinks.length})
            </h4>
            <div className="space-y-1.5">
              {article.backlinks.map(bl => (
                <div
                  key={bl.slug}
                  className="text-sm text-on-surface-variant hover:text-primary cursor-pointer transition-colors truncate"
                  onClick={() => navigate(`/wiki/${bl.slug}`)}
                >
                  {localizeTitle(bl.title, lang)}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Related */}
        {related.length > 0 && (
          <div className="mb-8">
            <h4 className="text-xs uppercase tracking-widest text-on-surface-variant mb-3">Related</h4>
            <div className="space-y-1.5">
              {related.map(a => (
                <div
                  key={a.slug}
                  className="text-sm text-on-surface-variant hover:text-primary cursor-pointer transition-colors truncate"
                  onClick={() => navigate(`/wiki/${a.slug}`)}
                >
                  {localizeTitle(a.title, lang)}
                </div>
              ))}
            </div>
          </div>
        )}
      </aside>
    </div>
  );
}
