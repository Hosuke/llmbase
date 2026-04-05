import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Icon } from '../components/Icon';
import { Shimmer } from '../components/Loading';
import { Markdown } from '../components/Markdown';
import { useLang } from '../lib/lang';
import { api, type Stats, type XiCi } from '../lib/api';

export function Dashboard() {
  const navigate = useNavigate();
  const { lang } = useLang();
  const [stats, setStats] = useState<Stats | null>(null);
  const [xici, setXiCi] = useState<XiCi | null>(null);
  const [generating, setGenerating] = useState(false);

  useEffect(() => {
    api.getStats().then(setStats).catch(() => {});
  }, []);

  useEffect(() => {
    api.getXiCi(lang).then(setXiCi).catch(() => {});
  }, [lang]);

  async function regenerate() {
    setGenerating(true);
    try {
      const result = await api.generateXiCi(lang);
      setXiCi(result);
    } catch {}
    setGenerating(false);
  }

  const timeAgo = (iso: string | null) => {
    if (!iso) return '';
    const diff = Date.now() - new Date(iso).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  };

  return (
    <div className="p-8 max-w-[1100px] mx-auto">

      {/* Xi Ci — Guided Introduction */}
      <div className="mb-8 bg-surface-container rounded-xl border border-outline-variant/20 overflow-hidden">
        <div className="p-6 pb-4">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-2">
              <Icon name="auto_stories" className="text-primary text-[20px]" />
              <span className="text-xs uppercase tracking-widest text-on-surface-variant">
                {lang === 'zh' || lang === 'zh-en' ? '系辞 — 导读' : lang === 'ja' ? '系辞 — 導読' : 'Xi Ci — Guided Reading'}
              </span>
            </div>
            <div className="flex items-center gap-3">
              {xici?.generated_at && (
                <span className="text-[11px] text-outline">{timeAgo(xici.generated_at)}</span>
              )}
              <button
                onClick={regenerate}
                disabled={generating}
                className="flex items-center gap-1 px-2.5 py-1 text-xs text-on-surface-variant hover:text-primary rounded-lg hover:bg-surface-container-highest/50 transition-colors disabled:opacity-50"
              >
                <Icon name={generating ? 'hourglass_empty' : 'refresh'} className="text-[14px]" />
                {generating ? (lang === 'zh' || lang === 'zh-en' ? '生成中...' : 'Generating...') : ''}
              </button>
            </div>
          </div>

          {/* The prose */}
          {generating ? (
            <Shimmer lines={4} />
          ) : xici?.text ? (
            <div className="font-serif text-[15px] leading-relaxed text-on-surface/90 mb-4">
              <Markdown content={xici.text} />
            </div>
          ) : (
            <div className="text-sm text-on-surface-variant italic py-4">
              {stats?.article_count
                ? (lang === 'zh' || lang === 'zh-en'
                    ? '点击刷新按钮生成知识库导读'
                    : 'Click refresh to generate a guided introduction')
                : (lang === 'zh' || lang === 'zh-en'
                    ? '知识库为空。请先导入文档并编译。'
                    : 'Knowledge base is empty. Ingest and compile documents first.')}
            </div>
          )}

          {/* Theme tags */}
          {xici?.themes && xici.themes.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {xici.themes.map(t => (
                <span key={t} className="px-2.5 py-0.5 text-[11px] bg-primary/10 text-primary rounded-full">
                  {t}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
        {[
          { icon: 'description', label: 'Raw Documents', value: stats?.raw_count, color: 'text-secondary' },
          { icon: 'article', label: 'Wiki Articles', value: stats?.article_count, color: 'text-primary' },
          { icon: 'link', label: 'Knowledge Links', value: stats?.link_count, color: 'text-tertiary' },
          { icon: 'health_and_safety', label: 'Health Score',
            value: stats ? `${stats.health_score}%` : undefined, color: 'text-on-surface' },
        ].map(s => (
          <div key={s.label} className="bg-surface-container rounded-xl p-5 border border-outline-variant/20">
            <div className="flex items-center gap-2 mb-2">
              <Icon name={s.icon} className={`text-[20px] ${s.color}`} />
              <span className="text-xs text-on-surface-variant uppercase tracking-wider">{s.label}</span>
            </div>
            <div className={`text-3xl font-bold font-label ${s.color}`}>
              {stats ? (s.value ?? '-') : <Shimmer lines={1} />}
            </div>
          </div>
        ))}
      </div>

      {/* Quick Actions + Agent API */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 mb-8">
        {/* Actions */}
        <div className="flex flex-wrap gap-3">
          <button onClick={() => navigate('/qa')} className="flex items-center gap-2 px-5 py-3 bg-primary/10 border border-primary/20 rounded-xl text-sm hover:bg-primary/20 transition-colors">
            <Icon name="forum" className="text-primary text-[18px]" /> Ask a Question
          </button>
          <button onClick={() => navigate('/ingest')} className="flex items-center gap-2 px-5 py-3 bg-surface-container border border-outline-variant/30 rounded-xl text-sm hover:border-secondary/50 transition-colors">
            <Icon name="add_link" className="text-secondary text-[18px]" /> Ingest
          </button>
          <button onClick={() => navigate('/health')} className="flex items-center gap-2 px-5 py-3 bg-surface-container border border-outline-variant/30 rounded-xl text-sm hover:border-tertiary/50 transition-colors">
            <Icon name="health_and_safety" className="text-tertiary text-[18px]" /> Health Check
          </button>
        </div>

        {/* Agent API Status */}
        <div className="bg-surface-container rounded-xl p-4 border border-outline-variant/20">
          <div className="flex items-center gap-2 mb-2">
            <Icon name="api" className="text-primary text-[16px]" />
            <span className="text-xs uppercase tracking-widest text-on-surface-variant">Agent API</span>
          </div>
          <div className="flex items-center gap-4 text-sm">
            <div className="flex items-center gap-1.5">
              <span className="w-2 h-2 rounded-full bg-green-500" />
              <code className="text-xs text-on-surface-variant">:5556</code>
            </div>
            <div className="flex items-center gap-1.5 text-outline">
              <span className="w-2 h-2 rounded-full bg-outline/30" />
              <span className="text-[11px]">mem9 <span className="text-[10px] opacity-60">soon</span></span>
            </div>
            <div className="flex items-center gap-1.5 text-outline">
              <span className="w-2 h-2 rounded-full bg-outline/30" />
              <span className="text-[11px]">db9 <span className="text-[10px] opacity-60">soon</span></span>
            </div>
          </div>
        </div>
      </div>

      {/* Quick link to wiki */}
      {stats && stats.article_count > 0 && (
        <div className="flex items-center justify-between">
          <span className="text-xs uppercase tracking-widest text-on-surface-variant">
            {stats.article_count} articles &middot; {stats.total_words.toLocaleString()} words
          </span>
          <button onClick={() => navigate('/wiki')} className="text-sm text-primary hover:underline">
            Browse Wiki &rarr;
          </button>
        </div>
      )}
    </div>
  );
}
