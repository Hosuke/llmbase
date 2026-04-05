import { useState, useEffect } from 'react';
import { Icon } from '../components/Icon';
import { Markdown } from '../components/Markdown';
import { Shimmer } from '../components/Loading';
import { useLang } from '../lib/lang';
import { api, type LintResults } from '../lib/api';

export function Health() {
  const { lang } = useLang();
  const [results, setResults] = useState<LintResults | null>(null);
  const [deepReport, setDeepReport] = useState('');
  const [fixes, setFixes] = useState<string[]>([]);
  const [lastCheck, setLastCheck] = useState<string | null>(null);
  const [loadingBasic, setLoadingBasic] = useState(false);
  const [loadingDeep, setLoadingDeep] = useState(false);
  const [loadingFix, setLoadingFix] = useState(false);
  const [loadingClean, setLoadingClean] = useState(false);
  const [cleanResult, setCleanResult] = useState<{ removed: number; slugs: string[] } | null>(null);

  const zh = lang === 'zh' || lang === 'zh-en';

  // Load cached health report on mount
  useEffect(() => {
    api.getHealth().then(res => {
      if (res.report) {
        setResults(res.report.results);
        setFixes(res.report.fixes_applied || []);
        setLastCheck(res.report.checked_at);
      }
    }).catch(() => {});
  }, []);

  async function runBasic() {
    setLoadingBasic(true);
    try {
      const res = await api.lint(false);
      if (res.results) setResults(res.results);
    } catch { /* */ }
    setLoadingBasic(false);
  }

  async function runDeep() {
    setLoadingDeep(true);
    try {
      const res = await api.lint(true);
      if (res.report) setDeepReport(res.report);
    } catch { /* */ }
    setLoadingDeep(false);
  }

  async function runFix() {
    setLoadingFix(true);
    try {
      const res = await api.lintFix();
      setFixes(res.fixes);
      // Re-run check to see the result
      const check = await api.lint(false);
      if (check.results) setResults(check.results);
    } catch { /* */ }
    setLoadingFix(false);
  }

  async function runClean() {
    setLoadingClean(true);
    try {
      const res = await api.cleanWiki();
      setCleanResult(res);
      // Re-run check
      const check = await api.lint(false);
      if (check.results) setResults(check.results);
    } catch { /* */ }
    setLoadingClean(false);
  }

  const allCategories = results ? [
    { key: 'structural', label: zh ? '结构' : 'Structural', icon: 'architecture', issues: results.structural, color: 'text-primary' },
    { key: 'broken_links', label: zh ? '断链' : 'Broken Links', icon: 'link_off', issues: results.broken_links, color: 'text-error' },
    { key: 'orphans', label: zh ? '孤立文章' : 'Orphans', icon: 'visibility_off', issues: results.orphans, color: 'text-secondary' },
    { key: 'missing_metadata', label: zh ? '缺失元数据' : 'Missing Metadata', icon: 'label_off', issues: results.missing_metadata, color: 'text-on-surface-variant' },
    { key: 'duplicates', label: zh ? '重复词条' : 'Duplicates', icon: 'content_copy', issues: (results as any).duplicates || [], color: 'text-tertiary' },
    { key: 'stubs', label: zh ? '垃圾文章' : 'Garbage Stubs', icon: 'delete_sweep', issues: (results as any).stubs || [], color: 'text-error' },
    { key: 'uncategorized', label: zh ? '未分类' : 'Uncategorized', icon: 'category', issues: (results as any).uncategorized || [], color: 'text-on-surface-variant' },
  ] : [];

  const categories = allCategories.filter(c => c.issues && c.issues.length > 0);

  return (
    <div className="p-8 max-w-[900px] mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h1 className="font-headline text-3xl font-bold">{zh ? '知识库健康' : 'Wiki Health'}</h1>
        {lastCheck && (
          <span className="text-[11px] text-outline">
            {zh ? '上次检查' : 'Last check'}: {new Date(lastCheck).toLocaleString()}
          </span>
        )}
      </div>

      {/* Actions — Diagnose + Repair */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 mb-8">
        <button onClick={runBasic} disabled={loadingBasic}
          className="flex items-center gap-2 px-4 py-3 bg-surface-container border border-outline-variant/30 rounded-xl text-sm hover:border-primary/50 transition-colors disabled:opacity-50">
          <Icon name="health_and_safety" className="text-primary text-[18px]" />
          {loadingBasic ? (zh ? '检查中...' : 'Checking...') : (zh ? '健康检查' : 'Check')}
        </button>
        <button onClick={runClean} disabled={loadingClean}
          className="flex items-center gap-2 px-4 py-3 bg-surface-container border border-outline-variant/30 rounded-xl text-sm hover:border-error/50 transition-colors disabled:opacity-50">
          <Icon name="delete_sweep" className="text-error text-[18px]" />
          {loadingClean ? (zh ? '清理中...' : 'Cleaning...') : (zh ? '清理垃圾' : 'Clean')}
        </button>
        <button onClick={runFix} disabled={loadingFix}
          className="flex items-center gap-2 px-4 py-3 bg-primary/10 border border-primary/20 rounded-xl text-sm hover:bg-primary/20 transition-colors disabled:opacity-50">
          <Icon name="auto_fix_high" className="text-primary text-[18px]" />
          {loadingFix ? (zh ? '修复中...' : 'Fixing...') : (zh ? '自动修复' : 'Auto Fix')}
        </button>
        <button onClick={runDeep} disabled={loadingDeep}
          className="flex items-center gap-2 px-4 py-3 bg-surface-container border border-outline-variant/30 rounded-xl text-sm hover:border-secondary/50 transition-colors disabled:opacity-50">
          <Icon name="psychology" className="text-secondary text-[18px]" />
          {loadingDeep ? (zh ? '分析中...' : 'Analyzing...') : (zh ? '深度分析' : 'Deep Analysis')}
        </button>
      </div>

      {/* Clean result */}
      {cleanResult && (
        <div className="bg-tertiary-container/20 border border-tertiary/20 rounded-xl px-5 py-3 mb-6 text-sm">
          <Icon name="check_circle" className="text-tertiary text-[16px] mr-2" />
          {zh ? `已清理 ${cleanResult.removed} 篇垃圾文章` : `Cleaned ${cleanResult.removed} garbage article(s)`}
          {cleanResult.slugs.length > 0 && (
            <span className="text-outline ml-2">({cleanResult.slugs.slice(0, 5).join(', ')})</span>
          )}
        </div>
      )}

      {/* Fix results */}
      {fixes.length > 0 && (
        <div className="bg-surface-container rounded-xl border border-outline-variant/20 mb-6 p-4">
          <h3 className="text-xs uppercase tracking-widest text-on-surface-variant mb-2">
            {zh ? '修复记录' : 'Fixes Applied'} ({fixes.length})
          </h3>
          <div className="space-y-1 max-h-40 overflow-y-auto">
            {fixes.map((f, i) => (
              <div key={i} className="text-sm text-on-surface-variant flex items-start gap-2">
                <Icon name="check" className="text-tertiary text-[14px] mt-0.5 flex-shrink-0" />
                {f}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Overview cards */}
      {results && (
        <>
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
            {allCategories.slice(0, 4).map(c => (
              <div key={c.key} className="bg-surface-container rounded-xl p-4 border border-outline-variant/20 text-center">
                <Icon name={c.icon} className={`text-2xl ${c.color} mb-1`} />
                <div className="text-2xl font-bold">{c.issues?.length ?? 0}</div>
                <div className="text-xs text-on-surface-variant">{c.label}</div>
              </div>
            ))}
          </div>

          {/* Overall status */}
          <div className={`rounded-xl px-5 py-4 mb-8 flex items-center gap-3 ${
            results.total_issues === 0
              ? 'bg-tertiary-container/20 border border-tertiary/20'
              : 'bg-surface-container border border-outline-variant/20'
          }`}>
            <Icon
              name={results.total_issues === 0 ? 'check_circle' : 'warning'}
              className={`text-2xl ${results.total_issues === 0 ? 'text-tertiary' : 'text-error'}`}
            />
            <span className="text-sm">
              {results.total_issues === 0
                ? (zh ? '所有检查通过！知识库健康。' : 'All checks passed! Wiki is healthy.')
                : (zh ? `发现 ${results.total_issues} 个问题` : `${results.total_issues} issue${results.total_issues > 1 ? 's' : ''} found`)
              }
            </span>
          </div>

          {/* Issue details */}
          {categories.map(c => (
            <div key={c.key} className="mb-6">
              <h3 className="text-sm font-semibold mb-2 flex items-center gap-2">
                <Icon name={c.icon} className={`text-[16px] ${c.color}`} />
                {c.label} ({c.issues.length})
              </h3>
              <div className="bg-surface-container rounded-xl border border-outline-variant/20 divide-y divide-outline-variant/10 max-h-60 overflow-y-auto">
                {c.issues.slice(0, 50).map((issue: string, i: number) => (
                  <div key={i} className="px-5 py-2.5 text-sm text-on-surface-variant">{issue}</div>
                ))}
                {c.issues.length > 50 && (
                  <div className="px-5 py-2.5 text-xs text-outline">...and {c.issues.length - 50} more</div>
                )}
              </div>
            </div>
          ))}
        </>
      )}

      {loadingBasic && <Shimmer lines={4} />}

      {/* Deep report */}
      {loadingDeep && (
        <div className="bg-surface-container rounded-xl p-6 border border-outline-variant/20 mt-6">
          <Shimmer lines={8} />
        </div>
      )}

      {deepReport && !loadingDeep && (
        <div className="mt-6">
          <h2 className="font-headline text-xl font-semibold mb-3 flex items-center gap-2">
            <Icon name="psychology" className="text-secondary" />
            {zh ? '深度分析' : 'Deep Analysis'}
          </h2>
          <div className="bg-surface-container rounded-xl p-6 border border-outline-variant/20">
            <Markdown content={deepReport} />
          </div>
        </div>
      )}

      {!results && !loadingBasic && !deepReport && !loadingDeep && (
        <div className="text-center py-16 text-on-surface-variant">
          <Icon name="health_and_safety" className="text-5xl mb-3 block" />
          <p>{zh ? '点击上方按钮检查知识库健康状态' : 'Run a health check to see wiki status'}</p>
        </div>
      )}
    </div>
  );
}
