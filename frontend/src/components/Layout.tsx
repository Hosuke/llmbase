import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { useState, useEffect, useRef } from 'react';
import { Icon } from './Icon';
import { useTheme } from '../lib/theme';
import { useLang, type Lang, localizeTitle } from '../lib/lang';
import { fetchBranding, getBranding, type Branding } from '../lib/branding';
import { api, type Article, type TaxonomyCategory } from '../lib/api';

/** Recursive sidebar category node — supports arbitrary depth */
function CategoryNode({ cat, depth, expandedCats, toggleCat, navigate, lang }: {
  cat: TaxonomyCategory; depth: number;
  expandedCats: Set<string>; toggleCat: (id: string) => void;
  navigate: (path: string) => void; lang: Lang;
}) {
  const pl = 3.5 + depth * 2.5; // progressive indentation (rem units)
  const hasChildren = cat.children.length > 0 || cat.articles.length > 0;
  const isTopLevel = depth === 0;

  return (
    <div>
      <div
        className={`flex items-center gap-2 pr-3.5 py-${isTopLevel ? '2' : '1.5'} text-sm ${
          isTopLevel ? 'font-medium text-on-surface' : 'text-on-surface-variant hover:text-on-surface'
        } hover:bg-surface-high rounded cursor-pointer transition-colors`}
        style={{ paddingLeft: `${pl * 4}px` }}
        onClick={() => toggleCat(cat.id)}>
        {hasChildren ? (
          <Icon name={expandedCats.has(cat.id) ? 'expand_more' : 'chevron_right'}
            className={`text-[${isTopLevel ? 16 : 14}px] ${isTopLevel ? 'text-primary' : ''}`} />
        ) : (
          <span style={{ width: isTopLevel ? 16 : 14 }} />
        )}
        <span className="truncate flex-1">{cat.label}</span>
        <span className={`text-[${isTopLevel ? 11 : 10}px] text-outline`}>{cat.total}</span>
      </div>

      {expandedCats.has(cat.id) && (
        <>
          {cat.children.map(child => (
            <CategoryNode key={child.id} cat={child} depth={depth + 1}
              expandedCats={expandedCats} toggleCat={toggleCat}
              navigate={navigate} lang={lang} />
          ))}
          {cat.articles.map(a => (
            <div key={a.slug}
              className="pr-3.5 py-1 text-sm text-on-surface-variant hover:text-on-surface hover:bg-surface-high rounded cursor-pointer truncate transition-colors"
              style={{ paddingLeft: `${(pl + 2.5) * 4}px` }}
              onClick={() => navigate(`/wiki/${a.slug}`)}>
              {localizeTitle(a.title, lang)}
            </div>
          ))}
        </>
      )}
    </div>
  );
}

const NAV = [
  { to: '/', icon: 'dashboard', label: 'Dashboard' },
  { to: '/wiki', icon: 'auto_stories', label: 'Wiki' },
  { to: '/search', icon: 'search', label: 'Search' },
  { to: '/qa', icon: 'forum', label: 'Q&A' },
  { to: '/graph', icon: 'hub', label: 'Graph' },
  { to: '/explore', icon: 'explore', label: 'Explore' },
  { to: '/trails', icon: 'route', label: 'Trails' },
  { to: '/ingest', icon: 'download', label: 'Ingest' },
  { to: '/health', icon: 'health_and_safety', label: 'Health' },
];

export function Layout() {
  const navigate = useNavigate();
  const { theme, toggle } = useTheme();
  const { lang, setLang, options, contract } = useLang();
  const [branding, setBranding] = useState<Branding>(getBranding());
  const [articles, setArticles] = useState<Article[]>([]);
  const [taxonomy, setTaxonomy] = useState<TaxonomyCategory[]>([]);
  const [expandedCats, setExpandedCats] = useState<Set<string>>(new Set());
  const [searchQuery, setSearchQuery] = useState('');
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [langOpen, setLangOpen] = useState(false);
  const langRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    fetchBranding().then(b => {
      setBranding(b);
      document.title = b.name || 'LLMBase';
    });
    api.getArticles().then(setArticles).catch(() => {});
  }, []);

  // Reload taxonomy when language changes
  useEffect(() => {
    if (!lang) return;
    const view = contract?.views.find(v => v.code === lang);
    const l = view?.primary ?? lang;
    api.getTaxonomy(l).then(setTaxonomy).catch(() => {});
  }, [lang, contract]);

  const toggleCat = (id: string) => {
    setExpandedCats(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  // Close lang dropdown on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (langRef.current && !langRef.current.contains(e.target as Node)) setLangOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, []);

  const currentLangOption = options.find(o => o.value === lang) ?? { label: '', icon: '' };

  return (
    <div className="flex h-screen overflow-hidden bg-bg">
      {/* Sidebar */}
      <aside className={`${sidebarOpen ? 'w-[260px]' : 'w-16'} bg-surface-container border-r border-outline-variant/30 flex flex-col flex-shrink-0 transition-all duration-200 card-shadow`}>
        <div className="p-5 border-b border-outline-variant/30 cursor-pointer" onClick={() => navigate('/')}>
          {sidebarOpen ? (
            <>
              <h1 className="text-lg font-bold text-primary tracking-tight font-headline">{branding.name}</h1>
              <p className="text-[11px] text-on-surface-variant tracking-widest uppercase mt-0.5">{branding.tagline}</p>
            </>
          ) : (
            <div className="text-primary font-bold text-center text-lg font-headline">{branding.nameShort}</div>
          )}
        </div>

        <nav className="flex-1 overflow-y-auto p-2.5">
          {NAV.map(n => (
            <NavLink key={n.to} to={n.to} end={n.to === '/'}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3.5 py-2.5 rounded-lg text-sm mb-0.5 transition-colors ${
                  isActive ? 'bg-primary-container/30 text-primary font-medium' : 'text-on-surface-variant hover:bg-surface-high'
                }`
              }>
              <Icon name={n.icon} className="text-[20px]" />
              {sidebarOpen && n.label}
            </NavLink>
          ))}

          {sidebarOpen && taxonomy.length > 0 && (
            <>
              <div className="text-[11px] text-on-surface-variant tracking-widest uppercase px-3.5 pt-5 pb-1.5">
                {articles.length} articles
              </div>
              {taxonomy.map(cat => (
                <CategoryNode key={cat.id} cat={cat} depth={0}
                  expandedCats={expandedCats} toggleCat={toggleCat}
                  navigate={navigate} lang={lang} />
              ))}
            </>
          )}
        </nav>

        <div className="border-t border-outline-variant/30">
          {sidebarOpen && (
            <div className="px-5 py-3">
              <a href={branding.poweredBy.url} target="_blank" rel="noopener noreferrer"
                className="text-[11px] text-outline hover:text-primary transition-colors">
                {branding.poweredBy.label}
              </a>
            </div>
          )}
          <div className="p-2.5 pt-0">
            <button onClick={() => setSidebarOpen(!sidebarOpen)}
              className="w-full flex items-center justify-center py-2 rounded-lg hover:bg-surface-high text-on-surface-variant transition-colors">
              <Icon name={sidebarOpen ? 'chevron_left' : 'chevron_right'} />
            </button>
          </div>
        </div>
      </aside>

      {/* Main */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Top bar */}
        <header className="h-14 bg-surface-container border-b border-outline-variant/30 flex items-center px-5 gap-3 flex-shrink-0 card-shadow">
          <div className="flex-1 relative">
            <Icon name="search" className="absolute left-3 top-1/2 -translate-y-1/2 text-outline text-[18px]" />
            <input type="text" placeholder="Search across documents... ⌘K"
              className="w-full bg-surface-high border border-outline-variant/40 rounded-lg pl-10 pr-4 py-2 text-sm text-on-surface placeholder:text-outline outline-none focus:border-primary/60 transition-colors"
              value={searchQuery} onChange={e => setSearchQuery(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter' && searchQuery.trim()) navigate(`/search?q=${encodeURIComponent(searchQuery)}`); }} />
          </div>

          {/* Language selector dropdown */}
          {options.length > 1 && (
            <div className="relative" ref={langRef}>
              <button onClick={() => setLangOpen(!langOpen)}
                className="flex items-center gap-1.5 px-3 py-2 bg-surface-high border border-outline-variant/40 rounded-lg text-sm hover:border-primary/50 transition-colors">
                <Icon name="translate" className="text-[16px] text-primary" />
                <span className="text-on-surface">{currentLangOption.label}</span>
                <Icon name="expand_more" className="text-[16px] text-on-surface-variant" />
              </button>

              {langOpen && (
                <div className="absolute right-0 top-full mt-1 bg-surface-container border border-outline-variant/40 rounded-xl shadow-lg z-50 py-1 min-w-[160px]">
                  {options.map(opt => (
                    <button key={opt.value}
                      onClick={() => { setLang(opt.value); setLangOpen(false); }}
                      className={`w-full text-left px-4 py-2.5 text-sm flex items-center gap-3 transition-colors ${
                        lang === opt.value
                          ? 'bg-primary-container/30 text-primary font-medium'
                          : 'text-on-surface-variant hover:bg-surface-high'
                      }`}>
                      <span className="w-6 text-center font-medium">{opt.icon}</span>
                      {opt.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Theme toggle */}
          <button onClick={toggle}
            className="p-2 rounded-lg hover:bg-surface-high text-on-surface-variant transition-colors"
            title={`Switch to ${theme === 'dark' ? 'light' : 'dark'} mode`}>
            <Icon name={theme === 'dark' ? 'light_mode' : 'dark_mode'} className="text-[20px]" />
          </button>

          <button onClick={() => { api.compile().then(() => api.getArticles().then(setArticles)); }}
            className="flex items-center gap-2 px-4 py-2 bg-primary text-on-primary rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
            <Icon name="auto_awesome" className="text-[18px]" />
            Compile
          </button>
        </header>

        <main className="flex-1 overflow-y-auto bg-bg">
          <Outlet />
        </main>

        <footer className="h-8 bg-surface-container border-t border-outline-variant/20 flex items-center justify-center flex-shrink-0">
          <a href={branding.poweredBy.url} target="_blank" rel="noopener noreferrer"
            className="text-[11px] text-outline hover:text-primary transition-colors">
            {branding.poweredBy.label}
          </a>
        </footer>
      </div>
    </div>
  );
}
