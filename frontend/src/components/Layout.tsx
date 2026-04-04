import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { useState, useEffect } from 'react';
import { Icon } from './Icon';
import { useTheme } from '../lib/theme';
import { useLang, LANG_LABELS, localizeTitle, type Lang } from '../lib/lang';
import { fetchBranding, getBranding, type Branding } from '../lib/branding';
import { api, type Article } from '../lib/api';

const NAV = [
  { to: '/', icon: 'dashboard', label: 'Dashboard' },
  { to: '/wiki', icon: 'auto_stories', label: 'Wiki' },
  { to: '/search', icon: 'search', label: 'Search' },
  { to: '/qa', icon: 'forum', label: 'Q&A' },
  { to: '/graph', icon: 'hub', label: 'Graph' },
  { to: '/ingest', icon: 'download', label: 'Ingest' },
  { to: '/health', icon: 'health_and_safety', label: 'Health' },
];

const LANGS: Lang[] = ['en', 'zh', 'ja'];

export function Layout() {
  const navigate = useNavigate();
  const { theme, toggle } = useTheme();
  const { lang, setLang } = useLang();
  const [branding, setBranding] = useState<Branding>(getBranding());
  const [articles, setArticles] = useState<Article[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [sidebarOpen, setSidebarOpen] = useState(true);

  useEffect(() => {
    fetchBranding().then(setBranding);
    api.getArticles().then(setArticles).catch(() => {});
  }, []);

  const handleGlobalSearch = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && searchQuery.trim()) {
      navigate(`/search?q=${encodeURIComponent(searchQuery)}`);
    }
  };

  return (
    <div className="flex h-screen overflow-hidden bg-bg">
      {/* Sidebar */}
      <aside className={`${sidebarOpen ? 'w-[260px]' : 'w-16'} bg-surface-container border-r border-outline-variant/30 flex flex-col flex-shrink-0 transition-all duration-200 card-shadow`}>
        {/* Logo */}
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

        {/* Nav */}
        <nav className="flex-1 overflow-y-auto p-2.5">
          {NAV.map(n => (
            <NavLink
              key={n.to}
              to={n.to}
              end={n.to === '/'}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3.5 py-2.5 rounded-lg text-sm mb-0.5 transition-colors ${
                  isActive
                    ? 'bg-primary-container/30 text-primary font-medium'
                    : 'text-on-surface-variant hover:bg-surface-high'
                }`
              }
            >
              <Icon name={n.icon} className="text-[20px]" />
              {sidebarOpen && n.label}
            </NavLink>
          ))}

          {sidebarOpen && articles.length > 0 && (
            <>
              <div className="text-[11px] text-on-surface-variant tracking-widest uppercase px-3.5 pt-5 pb-1.5">
                Articles ({articles.length})
              </div>
              {articles.slice(0, 30).map(a => (
                <div
                  key={a.slug}
                  className="px-3.5 py-1.5 text-sm text-on-surface-variant hover:text-on-surface hover:bg-surface-high rounded cursor-pointer truncate transition-colors"
                  onClick={() => navigate(`/wiki/${a.slug}`)}
                >
                  {localizeTitle(a.title, lang)}
                </div>
              ))}
            </>
          )}
        </nav>

        {/* Bottom */}
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
              value={searchQuery} onChange={e => setSearchQuery(e.target.value)} onKeyDown={handleGlobalSearch} />
          </div>

          {/* Language switcher */}
          <div className="flex items-center bg-surface-high rounded-lg p-0.5">
            {LANGS.map(l => (
              <button key={l} onClick={() => setLang(l)}
                className={`px-2.5 py-1 rounded-md text-xs font-medium transition-colors ${
                  lang === l ? 'bg-primary text-on-primary' : 'text-on-surface-variant hover:text-on-surface'
                }`}>
                {LANG_LABELS[l]}
              </button>
            ))}
          </div>

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

        {/* Content */}
        <main className="flex-1 overflow-y-auto bg-bg">
          <Outlet />
        </main>

        {/* Footer */}
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
