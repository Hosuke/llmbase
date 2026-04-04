import { useState, useEffect, useMemo } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Tag } from '../components/Tag';
import { Markdown } from '../components/Markdown';
import { Loading } from '../components/Loading';
import { Icon } from '../components/Icon';
import { useLang, localizeTitle, extractLangContent } from '../lib/lang';
import { api, type Article } from '../lib/api';

export function ArticleDetail() {
  const { slug } = useParams<{ slug: string }>();
  const navigate = useNavigate();
  const [article, setArticle] = useState<Article | null>(null);
  const [allArticles, setAllArticles] = useState<Article[]>([]);
  const [loading, setLoading] = useState(true);
  const { lang } = useLang();

  useEffect(() => {
    if (!slug) return;
    setLoading(true);
    Promise.all([
      api.getArticle(slug),
      api.getArticles(),
    ]).then(([a, all]) => {
      setArticle(a);
      setAllArticles(all);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, [slug]);

  const displayContent = useMemo(() => {
    if (!article?.content) return '';
    return extractLangContent(article.content, lang);
  }, [article?.content, lang]);

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
                  {a.title}
                </div>
              ))}
            </div>
          </div>
        )}
      </aside>
    </div>
  );
}
