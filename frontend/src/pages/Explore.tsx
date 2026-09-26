import { useState, useEffect, useRef, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import * as d3 from 'd3';
import { Icon } from '../components/Icon';
import { Shimmer } from '../components/Loading';
import { useLang } from '../lib/lang';
import { api } from '../lib/api';

type Tab = 'timeline' | 'people' | 'map';

interface Person { name: string; name_local?: string; dates?: string; role?: string; articles: string[] }
interface Event { name: string; name_local?: string; date?: string; description?: string; articles: string[] }
interface Place { name: string; name_local?: string; coords?: [number, number] | null; articles: string[] }

/** Parse date strings like "c.372-289 BCE", "1190 CE", "24th century BCE" into a numeric year. */
function parseYear(dateStr?: string): number | null {
  if (!dateStr) return null;
  const s = dateStr.replace(/c\.\s*/i, '').replace(/\s+/g, '');
  const bce = /bce|bc/i.test(s);

  // Handle "24th century" / "24th-23rd century" → 2400
  const centuryMatch = s.match(/(\d+)(?:st|nd|rd|th)\s*[-–]?\s*(?:\d+(?:st|nd|rd|th)\s*)?century/i);
  if (centuryMatch) {
    const century = parseInt(centuryMatch[1]);
    const year = (century - 1) * 100; // 24th century = 2300s
    return bce ? -year : year;
  }

  // "372-289 BCE" → take first number
  const match = s.match(/(\d+)/);
  if (!match) return null;
  const year = parseInt(match[1]);
  return bce ? -year : year;
}

export function Explore() {
  const navigate = useNavigate();
  const { lang } = useLang();
  const zh = lang === 'zh' || lang === 'zh-en';
  const [tab, setTab] = useState<Tab>('timeline');
  const [people, setPeople] = useState<Person[]>([]);
  const [events, setEvents] = useState<Event[]>([]);
  const [places, setPlaces] = useState<Place[]>([]);
  const [loading, setLoading] = useState(true);
  const [articleCount, setArticleCount] = useState(0);
  const [filter, setFilter] = useState<'all' | 'people' | 'events'>('all');
  const [hovered, setHovered] = useState<{ name: string; dates: string; role?: string; x: number; y: number } | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    api.getEntities().then(data => {
      setPeople(data.people || []);
      setEvents(data.events || []);
      setPlaces(data.places || []);
      setArticleCount(data.article_count || 0);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, []);

  const displayName = (entity: { name: string; name_local?: string }) =>
    (zh && entity.name_local) ? entity.name_local : entity.name;

  // Build timeline items with parsed years
  const timelineItems = useMemo(() => {
    const items: { name: string; localName: string; year: number; type: 'person' | 'event'; dates: string; role?: string; description?: string; slug?: string }[] = [];

    if (filter === 'all' || filter === 'people') {
      for (const p of people) {
        const year = parseYear(p.dates);
        if (year !== null) {
          items.push({
            name: p.name, localName: p.name_local || p.name,
            year, type: 'person', dates: p.dates || '',
            role: p.role, slug: p.articles[0],
          });
        }
      }
    }
    if (filter === 'all' || filter === 'events') {
      for (const e of events) {
        const year = parseYear(e.date);
        if (year !== null) {
          items.push({
            name: e.name, localName: e.name_local || e.name,
            year, type: 'event', dates: e.date || '',
            description: e.description, slug: e.articles[0],
          });
        }
      }
    }

    return items.sort((a, b) => a.year - b.year);
  }, [people, events, filter]);

  // D3 horizontal timeline
  useEffect(() => {
    if (tab !== 'timeline' || !svgRef.current || timelineItems.length === 0) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();

    const width = svgRef.current.clientWidth;
    const height = 420;
    const midY = height / 2 + 10;
    const margin = { top: 30, right: 50, bottom: 55, left: 50 };

    svg.attr('viewBox', `0 0 ${width} ${height}`);

    // ─── SVG Filters (glow effects) ────────────────────────
    const defs = svg.append('defs');
    const glowBlue = defs.append('filter').attr('id', 'glow-blue');
    glowBlue.append('feGaussianBlur').attr('stdDeviation', 3).attr('result', 'blur');
    glowBlue.append('feMerge').selectAll('feMergeNode').data(['blur', 'SourceGraphic'])
      .enter().append('feMergeNode').attr('in', d => d);

    const glowAmber = defs.append('filter').attr('id', 'glow-amber');
    glowAmber.append('feGaussianBlur').attr('stdDeviation', 3).attr('result', 'blur');
    glowAmber.append('feMerge').selectAll('feMergeNode').data(['blur', 'SourceGraphic'])
      .enter().append('feMergeNode').attr('in', d => d);

    // Gradient stems
    for (const [id, color] of [['stem-blue', '#60a5fa'], ['stem-amber', '#fbbf24']]) {
      const grad = defs.append('linearGradient').attr('id', id).attr('x1', 0).attr('y1', 0).attr('x2', 0).attr('y2', 1);
      grad.append('stop').attr('offset', '0%').attr('stop-color', color).attr('stop-opacity', 0.8);
      grad.append('stop').attr('offset', '100%').attr('stop-color', color).attr('stop-opacity', 0.1);
    }

    const minYear = d3.min(timelineItems, d => d.year) ?? -500;
    const maxYear = d3.max(timelineItems, d => d.year) ?? 2000;
    const padding = Math.max(80, (maxYear - minYear) * 0.08);

    const x = d3.scaleLinear()
      .domain([minYear - padding, maxYear + padding])
      .range([margin.left, width - margin.right]);

    // ─── Era background bands ──────────────────────────────
    const eras = [
      { label: zh ? '上古' : 'Ancient', start: -800, end: -200, color: '#1e3a5f' },
      { label: zh ? '古典' : 'Classical', start: -200, end: 200, color: '#1a3d2e' },
      { label: zh ? '中古' : 'Medieval', start: 200, end: 1000, color: '#3d2e1a' },
      { label: zh ? '近世' : 'Early Modern', start: 1000, end: 1800, color: '#2e1a3d' },
    ];

    const eraG = svg.append('g').attr('class', 'eras');
    // Always render all eras (data-bound) so zoom indices stay aligned
    const eraRects = eraG.selectAll('rect').data(eras).enter().append('rect');
    const eraTexts = eraG.selectAll('text').data(eras).enter().append('text');

    function updateEras(xScale: d3.ScaleLinear<number, number>) {
      eraRects.each(function (era) {
        const ex1 = Math.max(xScale(era.start), margin.left);
        const ex2 = Math.min(xScale(era.end), width - margin.right);
        const w = Math.max(0, ex2 - ex1);
        d3.select(this)
          .attr('x', ex1).attr('y', margin.top)
          .attr('width', w).attr('height', height - margin.top - margin.bottom)
          .attr('fill', era.color).attr('opacity', w > 0 ? 0.15 : 0).attr('rx', 4);
      });
      eraTexts.each(function (era) {
        const ex1 = Math.max(xScale(era.start), margin.left);
        const ex2 = Math.min(xScale(era.end), width - margin.right);
        const w = ex2 - ex1;
        d3.select(this)
          .attr('x', (ex1 + ex2) / 2).attr('y', margin.top + 14)
          .attr('text-anchor', 'middle')
          .attr('fill', era.color.replace(/1/g, '8')).attr('opacity', w > 30 ? 0.6 : 0)
          .style('font-size', '10px').style('letter-spacing', '1px')
          .text(era.label.toUpperCase());
      });
    }
    updateEras(x);

    // ─── Axis ──────────────────────────────────────────────
    const axisG = svg.append('g')
      .attr('transform', `translate(0,${height - margin.bottom})`);

    const formatYear = (d: number) => d < 0 ? `${Math.abs(d)} BCE` : `${d} CE`;

    axisG.call(
      d3.axisBottom(x).tickFormat(d => formatYear(d as number)).tickSize(-6)
    );
    axisG.selectAll('text').attr('fill', '#6b7280').style('font-size', '10px').style('font-family', 'Inter, sans-serif');
    axisG.selectAll('.tick line').attr('stroke', '#374151').attr('opacity', 0.5);
    axisG.select('.domain').attr('stroke', '#374151').attr('opacity', 0.3);

    // ─── Center timeline line ──────────────────────────────
    svg.append('line')
      .attr('class', 'center-line')
      .attr('x1', margin.left).attr('x2', width - margin.right)
      .attr('y1', midY).attr('y2', midY)
      .attr('stroke', '#374151').attr('stroke-width', 1.5).attr('opacity', 0.4);

    // ─── Stagger algorithm (6 rows) ───────────────────────
    const ROW_COUNT = 6;
    const ROW_HEIGHT = 42;
    const usedSlots: Set<string> = new Set();
    function getPosition(xPos: number): { cy: number; above: boolean } {
      const col = Math.round(xPos / 50);
      for (let row = 0; row < ROW_COUNT; row++) {
        const key = `${col}-${row}`;
        if (!usedSlots.has(key)) {
          usedSlots.add(key);
          const above = row % 2 === 0;
          const tier = Math.floor(row / 2) + 1;
          return { cy: midY + (above ? -tier * ROW_HEIGHT : tier * ROW_HEIGHT), above };
        }
      }
      const row = Math.floor(Math.random() * ROW_COUNT);
      const above = row % 2 === 0;
      return { cy: midY + (above ? -(Math.floor(row/2)+1) * ROW_HEIGHT : (Math.floor(row/2)+1) * ROW_HEIGHT), above };
    }

    // ─── Draw entities ─────────────────────────────────────
    const DOT_R = 7;
    const items = svg.selectAll('.tl-item')
      .data(timelineItems)
      .enter()
      .append('g')
      .attr('class', 'tl-item')
      .attr('cursor', 'pointer')
      .on('click', (_, d) => { if (d.slug) navigate(`/wiki/${d.slug}`); });

    items.each(function (d) {
      const g = d3.select(this);
      const cx = x(d.year);
      const { cy, above } = getPosition(cx);
      const isPerson = d.type === 'person';
      const color = isPerson ? '#60a5fa' : '#fbbf24';

      // Stem (gradient opacity)
      const stemId = isPerson ? 'url(#stem-blue)' : 'url(#stem-amber)';
      g.append('line')
        .attr('class', 'stem')
        .attr('x1', cx).attr('x2', cx)
        .attr('y1', midY).attr('y2', cy)
        .attr('stroke', stemId).attr('stroke-width', 1.5);

      // Dot with glow
      g.append('circle')
        .attr('class', 'dot')
        .attr('cx', cx).attr('cy', cy).attr('r', DOT_R)
        .attr('fill', color)
        .attr('stroke', '#0f1419').attr('stroke-width', 2)
        .attr('filter', isPerson ? 'url(#glow-blue)' : 'url(#glow-amber)');

      // Label (serif font)
      const label = zh
        ? (d.localName.length > 8 ? d.localName.slice(0, 8) + '…' : d.localName)
        : (d.name.length > 14 ? d.name.slice(0, 14) + '…' : d.name);

      g.append('text')
        .attr('class', 'label')
        .attr('x', cx).attr('y', cy + (above ? -14 : 18))
        .attr('text-anchor', 'middle')
        .attr('fill', '#e5e7eb')
        .style('font-size', '11.5px')
        .style('font-family', "'Noto Serif', 'Songti SC', serif")
        .text(label);

      // Small year tag
      g.append('text')
        .attr('class', 'year-tag')
        .attr('x', cx).attr('y', cy + (above ? -26 : 30))
        .attr('text-anchor', 'middle')
        .attr('fill', '#6b7280')
        .style('font-size', '9px')
        .style('font-family', 'Inter, sans-serif')
        .text(d.dates || '');
    });

    // ─── Hover interaction ─────────────────────────────────
    let currentTransform = d3.zoomIdentity;

    items.on('mouseenter', function (_event, d) {
      const cx = currentTransform.rescaleX(x)(d.year);
      setHovered({ name: zh ? d.localName : d.name, dates: d.dates, role: d.role || d.description, x: cx, y: 10 });
      d3.select(this).select('.dot').transition().duration(200).attr('r', DOT_R + 4).attr('stroke-width', 3);
      d3.select(this).select('.label').transition().duration(200).attr('fill', '#ffffff');
    }).on('mouseleave', function () {
      setHovered(null);
      d3.select(this).select('.dot').transition().duration(200).attr('r', DOT_R).attr('stroke-width', 2);
      d3.select(this).select('.label').transition().duration(200).attr('fill', '#e5e7eb');
    });

    // ─── Zoom + pan ────────────────────────────────────────
    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.3, 12])
      .on('zoom', (event) => {
        currentTransform = event.transform;
        setHovered(null); // Dismiss tooltip during zoom to prevent drift
        const newX = event.transform.rescaleX(x);

        // Update axis
        axisG.call(d3.axisBottom(newX).tickFormat(d => formatYear(d as number)).tickSize(-6));
        axisG.selectAll('text').attr('fill', '#6b7280').style('font-size', '10px').style('font-family', 'Inter, sans-serif');
        axisG.selectAll('.tick line').attr('stroke', '#374151').attr('opacity', 0.5);
        axisG.select('.domain').attr('stroke', '#374151').attr('opacity', 0.3);

        // Update era bands (data-bound, no index mismatch)
        updateEras(newX);

        // Update entities
        items.each(function (d) {
          const g = d3.select(this);
          const cx = newX(d.year);
          g.select('.stem').attr('x1', cx).attr('x2', cx);
          g.select('.dot').attr('cx', cx);
          g.select('.label').attr('x', cx);
          g.select('.year-tag').attr('x', cx);
        });

        // Update center line (class-selected, not positional)
        svg.select('.center-line')
          .attr('x1', newX(minYear - padding))
          .attr('x2', newX(maxYear + padding));
      });

    svg.call(zoom);

  }, [tab, timelineItems, zh, navigate]);

  const tabs: { id: Tab; label: string; icon: string }[] = [
    { id: 'timeline', label: zh ? '时间线' : 'Timeline', icon: 'timeline' },
    { id: 'people', label: zh ? '人物' : 'People', icon: 'groups' },
    { id: 'map', label: zh ? '地图' : 'Map', icon: 'map' },
  ];

  const isEmpty = people.length === 0 && events.length === 0 && places.length === 0;

  return (
    <div className="p-8 max-w-[1100px] mx-auto">
      <h1 className="font-headline text-3xl font-bold mb-6">{zh ? '探索' : 'Explore'}</h1>

      {/* Tabs */}
      <div className="flex gap-1 mb-6 border-b border-outline-variant/30">
        {tabs.map(t => (
          <button key={t.id} onClick={() => setTab(t.id)}
            className={`flex items-center gap-2 px-4 py-2.5 text-sm transition-colors border-b-2 ${
              tab === t.id
                ? 'border-primary text-primary font-medium'
                : 'border-transparent text-on-surface-variant hover:text-on-surface'
            }`}>
            <Icon name={t.icon} className="text-[16px]" />
            {t.label}
          </button>
        ))}
      </div>

      {loading && <Shimmer lines={8} />}

      {!loading && isEmpty && (
        <div className="text-center py-16 text-on-surface-variant">
          <Icon name="explore" className="text-5xl mb-3 block" />
          <p className="mb-4">{zh ? '尚未提取实体。请在设置中启用 entities 功能。' : 'No entities extracted yet. Enable entities in config.'}</p>
          <code className="text-xs bg-surface-container px-3 py-1.5 rounded-lg">entities: {'{'} enabled: true {'}'}</code>
        </div>
      )}

      {/* Timeline Tab — D3 horizontal axis */}
      {!loading && tab === 'timeline' && !isEmpty && (
        <div>
          <div className="flex items-center justify-between mb-4">
            <div className="flex gap-2">
              {(['all', 'people', 'events'] as const).map(f => (
                <button key={f} onClick={() => setFilter(f)}
                  className={`px-3 py-1 text-xs rounded-full transition-colors ${
                    filter === f ? 'bg-primary/15 text-primary' : 'bg-surface-container text-on-surface-variant'
                  }`}>
                  {f === 'all' ? (zh ? '全部' : 'All') :
                   f === 'people' ? (zh ? '人物' : 'People') : (zh ? '事件' : 'Events')}
                </button>
              ))}
            </div>
            <span className="text-[10px] text-outline">{zh ? '滚轮缩放，拖拽平移' : 'Scroll to zoom, drag to pan'}</span>
          </div>

          <div className="relative bg-[#141414] rounded-xl border border-outline-variant/15 overflow-hidden">
            <svg ref={svgRef} className="w-full" style={{ height: 420 }} />
            {hovered && (
              <div className="absolute bg-[#1e1e1e] border border-outline-variant/25 rounded-xl px-4 py-3 shadow-2xl pointer-events-none backdrop-blur-sm"
                style={{ left: Math.min(Math.max(hovered.x - 80, 10), 700), top: hovered.y }}>
                <div className="font-medium font-serif">{hovered.name}</div>
                <div className="text-xs text-outline mt-0.5 font-mono">{hovered.dates}</div>
                {hovered.role && <div className="text-xs text-on-surface-variant mt-1">{hovered.role}</div>}
              </div>
            )}
          </div>

          {timelineItems.length === 0 && (
            <div className="text-center py-8 text-on-surface-variant text-sm">
              {zh ? '没有可用日期的实体。实体需要包含日期信息才能显示在时间线上。' : 'No entities with parseable dates.'}
            </div>
          )}
        </div>
      )}

      {/* People Tab */}
      {!loading && tab === 'people' && !isEmpty && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {people.map((p, i) => (
            <div key={i} className="bg-surface-container rounded-xl p-5 border border-outline-variant/20 hover:border-primary/30 transition-colors cursor-pointer"
              onClick={() => p.articles[0] && navigate(`/wiki/${p.articles[0]}`)}>
              <div className="font-medium mb-1">{displayName(p)}</div>
              {p.name_local && p.name !== p.name_local && (
                <div className="text-xs text-on-surface-variant mb-2">{zh ? p.name : p.name_local}</div>
              )}
              <div className="flex items-center justify-between text-xs text-outline">
                <span>{p.dates || '—'}</span>
                <span>{p.role}</span>
              </div>
              <div className="mt-2 text-[10px] text-outline">{p.articles.length} {zh ? '篇相关文章' : 'related articles'}</div>
            </div>
          ))}
        </div>
      )}

      {/* Map Tab */}
      {!loading && tab === 'map' && (
        <div className="bg-surface-container rounded-xl p-8 border border-outline-variant/20 text-center">
          <Icon name="map" className="text-5xl text-on-surface-variant mb-3 block" />
          <p className="text-on-surface-variant mb-2">{zh ? '地图视图' : 'Map View'}</p>
          <p className="text-xs text-outline">
            {places.length > 0
              ? `${places.length} ${zh ? '个地点已提取' : 'places extracted'}`
              : (zh ? '需要在实体数据中包含坐标信息' : 'Requires coordinates in entity data')}
          </p>
          {places.length > 0 && (
            <div className="mt-4 space-y-2 max-w-md mx-auto text-left">
              {places.map((p, i) => (
                <div key={i} className="flex items-center gap-3 text-sm">
                  <Icon name="place" className="text-error text-[16px]" />
                  <span>{displayName(p)}</span>
                  <span className="text-[10px] text-outline ml-auto">
                    {p.coords ? `${p.coords[0].toFixed(1)}, ${p.coords[1].toFixed(1)}` : '—'}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Stats banner */}
      {!isEmpty && (
        <div className="mt-8 text-center text-xs text-outline">
          {zh ? '实体提取' : 'Entity extraction'}: {people.length} {zh ? '人物' : 'people'}, {events.length} {zh ? '事件' : 'events'}, {places.length} {zh ? '地点' : 'places'} — {zh ? '来自' : 'from'} {articleCount} {zh ? '篇文章' : 'articles'}
        </div>
      )}
    </div>
  );
}
