import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Icon } from '../components/Icon';
import { Shimmer } from '../components/Loading';
import { useLang } from '../lib/lang';
import { api, type Trail } from '../lib/api';

export function Trails() {
  const navigate = useNavigate();
  const { lang } = useLang();
  const zh = lang === 'zh' || lang === 'zh-en';
  const [trails, setTrails] = useState<Trail[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    api.getTrails().then(setTrails).catch(() => {});
    setLoading(false);
  }, []);

  const deleteTrail = (id: string) => {
    api.deleteTrail(id).then(() => {
      setTrails(prev => prev.filter(t => t.id !== id));
    });
  };

  const stepIcon = (type: string) =>
    type === 'article' ? 'article' : type === 'query' ? 'forum' : 'search';

  const stepLabel = (step: Trail['steps'][0]) => {
    if (step.type === 'article') return step.title || step.slug || '—';
    if (step.type === 'query') return step.question || '—';
    return step.question || '—';
  };

  return (
    <div className="p-8 max-w-[900px] mx-auto">
      <h1 className="font-headline text-3xl font-bold mb-2">{zh ? '探索路径' : 'Research Trails'}</h1>
      <p className="text-on-surface-variant text-sm mb-8">
        {zh ? '记录你的研究路线——从一个概念出发，层层深入。' : 'Record your research journey — follow threads of inquiry.'}
      </p>

      {loading && <Shimmer lines={4} />}

      {!loading && trails.length === 0 && (
        <div className="text-center py-16 text-on-surface-variant">
          <Icon name="route" className="text-5xl mb-3 block" />
          <p>{zh ? '还没有探索路径。浏览文章时点击右下角按钮开始记录。' : 'No trails yet. Click the button on the bottom-right while browsing to start recording.'}</p>
        </div>
      )}

      <div className="space-y-4">
        {trails.map(trail => (
          <div key={trail.id} className="bg-surface-container rounded-xl border border-outline-variant/20 overflow-hidden">
            {/* Trail header */}
            <div className="flex items-center gap-3 px-5 py-4 cursor-pointer hover:bg-surface-container-highest/30 transition-colors"
              onClick={() => setExpanded(expanded === trail.id ? null : trail.id)}>
              <Icon name={expanded === trail.id ? 'expand_more' : 'chevron_right'} className="text-primary text-[18px]" />
              <div className="flex-1 min-w-0">
                <div className="font-medium text-sm truncate">{trail.name}</div>
                <div className="text-xs text-outline">
                  {trail.steps.length} {zh ? '步' : 'steps'} &middot; {new Date(trail.updated).toLocaleDateString()}
                </div>
              </div>
              <button onClick={e => { e.stopPropagation(); deleteTrail(trail.id); }}
                className="text-outline hover:text-error transition-colors p-1">
                <Icon name="delete_outline" className="text-[16px]" />
              </button>
            </div>

            {/* Trail steps */}
            {expanded === trail.id && (
              <div className="px-5 pb-4 border-t border-outline-variant/10">
                <div className="relative ml-3 mt-3">
                  {/* Vertical line */}
                  <div className="absolute left-0 top-0 bottom-0 w-px bg-outline-variant/30" />

                  {trail.steps.map((step, i) => (
                    <div key={i} className="relative flex items-start gap-3 mb-3 pl-5">
                      {/* Dot on the line */}
                      <div className={`absolute left-[-4px] top-1.5 w-2 h-2 rounded-full ${
                        step.type === 'article' ? 'bg-primary' :
                        step.type === 'query' ? 'bg-secondary' : 'bg-tertiary'
                      }`} />
                      <Icon name={stepIcon(step.type)} className="text-[14px] text-on-surface-variant mt-0.5 flex-shrink-0" />
                      <div className="min-w-0 flex-1">
                        <div className="text-sm text-on-surface truncate cursor-pointer hover:text-primary transition-colors"
                          onClick={() => step.slug && navigate(`/wiki/${step.slug}`)}>
                          {stepLabel(step)}
                        </div>
                        <div className="text-[10px] text-outline">
                          {new Date(step.ts).toLocaleTimeString()}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
