import { useState, useEffect, useRef, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import * as d3 from 'd3';
import { Icon } from '../components/Icon';
import { Loading } from '../components/Loading';
import { useLang, localizeTitle } from '../lib/lang';
import { api, type Article } from '../lib/api';

interface Node extends d3.SimulationNodeDatum {
  id: string; title: string; localTitle: string;
  tags: string[]; size: number; summary: string;
  linkCount: number; cluster: number;
}
interface Link extends d3.SimulationLinkDatum<Node> {
  source: string | Node; target: string | Node; weight: number;
}

const PALETTE = ['#60a5fa', '#34d399', '#fbbf24', '#f87171', '#a78bfa', '#38bdf8', '#fb923c', '#e879f9'];

export function Graph() {
  const navigate = useNavigate();
  const { lang, contract } = useLang();
  const zh = lang === 'zh' || lang === 'zh-en';
  const svgRef = useRef<SVGSVGElement>(null);
  const [articles, setArticles] = useState<Article[]>([]);
  const [loading, setLoading] = useState(true);
  const [showLabels, setShowLabels] = useState(false); // Off by default for large graphs
  const [hovered, setHovered] = useState<Node | null>(null);
  const [selectedTag, setSelectedTag] = useState<string | null>(null);
  const [linkThreshold, setLinkThreshold] = useState(2); // Min shared tags to show a link
  const [visibleCount, setVisibleCount] = useState<number | null>(null);

  useEffect(() => {
    api.getArticles().then(a => { setArticles(a); setLoading(false); }).catch(() => setLoading(false));
  }, []);

  // Top tags (by frequency)
  const topTags = useMemo(() => {
    const counts: Record<string, number> = {};
    articles.forEach(a => a.tags?.forEach(t => { if (!t.startsWith('category:')) counts[t] = (counts[t] || 0) + 1; }));
    return Object.entries(counts).sort((a, b) => b[1] - a[1]).slice(0, 12).map(([t]) => t);
  }, [articles]);

  const tagColors: Record<string, string> = {};
  topTags.forEach((t, i) => { tagColors[t] = PALETTE[i % PALETTE.length]; });

  useEffect(() => {
    if (!svgRef.current || articles.length === 0) return;

    setHovered(null); // Clear stale hover on rebuild
    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();

    const width = svgRef.current.clientWidth;
    const height = svgRef.current.clientHeight;

    // Filter by tag
    const filtered = selectedTag
      ? articles.filter(a => a.tags?.includes(selectedTag))
      : articles;

    // Build links via inverted index (O(T * avg_articles_per_tag) instead of O(n²))
    const links: Link[] = [];
    const tagToSlugs: Record<string, string[]> = {};
    for (const a of filtered) {
      for (const t of a.tags || []) {
        if (!t.startsWith('category:')) {
          (tagToSlugs[t] ??= []).push(a.slug);
        }
      }
    }
    // Count shared tags per pair
    const pairWeights: Record<string, number> = {};
    for (const slugs of Object.values(tagToSlugs)) {
      if (slugs.length > 100) continue; // Skip overly generic tags
      for (let i = 0; i < slugs.length; i++) {
        for (let j = i + 1; j < slugs.length; j++) {
          const key = slugs[i] < slugs[j] ? `${slugs[i]}|${slugs[j]}` : `${slugs[j]}|${slugs[i]}`;
          pairWeights[key] = (pairWeights[key] || 0) + 1;
        }
      }
    }
    for (const [key, weight] of Object.entries(pairWeights)) {
      if (weight >= linkThreshold) {
        const [a, b] = key.split('|');
        links.push({ source: a, target: b, weight });
      }
    }

    // Count links per node
    const linkCounts: Record<string, number> = {};
    links.forEach(l => {
      const s = typeof l.source === 'string' ? l.source : l.source.id;
      const t = typeof l.target === 'string' ? l.target : l.target.id;
      linkCounts[s] = (linkCounts[s] || 0) + 1;
      linkCounts[t] = (linkCounts[t] || 0) + 1;
    });

    // Cluster by primary tag
    const clusterMap: Record<string, number> = {};
    topTags.forEach((t, i) => { clusterMap[t] = i; });

    const nodes: Node[] = filtered.map(a => {
      const primaryTag = a.tags?.find(t => t in clusterMap) || a.tags?.[0] || '';
      return {
        id: a.slug,
        title: a.title,
        localTitle: localizeTitle(a.title, lang),
        tags: a.tags || [],
        summary: a.summary || '',
        size: Math.max(4, Math.min(5 + (linkCounts[a.slug] || 0) * 1.5, 22)),
        linkCount: linkCounts[a.slug] || 0,
        cluster: clusterMap[primaryTag] ?? -1,
      };
    });

    // Only show nodes that have at least 1 link (hide isolates in large graphs)
    const connectedIds = new Set<string>();
    links.forEach(l => {
      connectedIds.add(typeof l.source === 'string' ? l.source : l.source.id);
      connectedIds.add(typeof l.target === 'string' ? l.target : l.target.id);
    });
    const visibleNodes = filtered.length > 100
      ? nodes.filter(n => connectedIds.has(n.id))
      : nodes;
    setVisibleCount(visibleNodes.length);

    const g = svg.append('g');

    // Zoom
    svg.call(
      d3.zoom<SVGSVGElement, unknown>()
        .scaleExtent([0.1, 8])
        .on('zoom', (e) => g.attr('transform', e.transform))
    );

    // Adaptive force parameters based on graph size
    const nodeCount = visibleNodes.length;
    const chargeStrength = nodeCount > 200 ? -80 : nodeCount > 50 ? -150 : -250;
    const linkDistance = nodeCount > 200 ? 40 : nodeCount > 50 ? 70 : 100;

    const simulation = d3.forceSimulation(visibleNodes)
      .force('link', d3.forceLink<Node, Link>(links).id(d => d.id)
        .distance(d => linkDistance / Math.max(d.weight, 1))
        .strength(d => Math.min(d.weight * 0.3, 1)))
      .force('charge', d3.forceManyBody().strength(chargeStrength))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(d => (d as Node).size + 3))
      .force('x', d3.forceX(width / 2).strength(0.03))
      .force('y', d3.forceY(height / 2).strength(0.03));

    // Links
    const link = g.append('g')
      .selectAll('line')
      .data(links)
      .join('line')
      .attr('stroke', '#4b5563')
      .attr('stroke-width', d => Math.min(d.weight * 0.5, 3))
      .attr('stroke-opacity', d => Math.min(0.15 + d.weight * 0.1, 0.5));

    // Nodes
    const node = g.append('g')
      .selectAll('circle')
      .data(visibleNodes)
      .join('circle')
      .attr('r', d => d.size)
      .attr('fill', d => {
        const primary = d.tags.find(t => t in tagColors);
        return primary ? tagColors[primary] : '#6b7280';
      })
      .attr('fill-opacity', 0.8)
      .attr('stroke', d => {
        const primary = d.tags.find(t => t in tagColors);
        return primary ? tagColors[primary] : '#6b7280';
      })
      .attr('stroke-width', 1.5)
      .attr('stroke-opacity', 0.3)
      .style('cursor', 'pointer')
      .on('click', (_, d) => navigate(`/wiki/${d.id}`))
      .call(d3.drag<SVGCircleElement, Node>()
        .on('start', (e, d) => { if (!e.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
        .on('drag', (e, d) => { d.fx = e.x; d.fy = e.y; })
        .on('end', (e, d) => { if (!e.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; })
      );

    // Labels (only for nodes with enough connections, or when showLabels=true)
    const label = g.append('g')
      .selectAll('text')
      .data(visibleNodes)
      .join('text')
      .text(d => {
        const t = d.localTitle;
        return t.length > 10 ? t.slice(0, 10) + '…' : t;
      })
      .attr('font-size', d => d.linkCount > 5 ? 11 : 10)
      .attr('font-family', zh ? "'Noto Serif SC', serif" : 'Inter, sans-serif')
      .attr('fill', '#d1d5db')
      .attr('text-anchor', 'middle')
      .attr('dy', d => d.size + 12)
      .style('pointer-events', 'none')
      .style('display', d => showLabels || d.linkCount >= 3 ? 'block' : 'none');

    // Hover: highlight connected nodes
    node.on('mouseenter', function (_, d) {
      setHovered(d);
      const connected = new Set<string>([d.id]);
      links.forEach(l => {
        const s = typeof l.source === 'string' ? l.source : l.source.id;
        const t = typeof l.target === 'string' ? l.target : l.target.id;
        if (s === d.id) connected.add(t);
        if (t === d.id) connected.add(s);
      });
      node.attr('fill-opacity', n => connected.has(n.id) ? 1 : 0.08);
      node.attr('stroke-opacity', n => connected.has(n.id) ? 0.8 : 0.03);
      link.attr('stroke-opacity', l => {
        const s = typeof l.source === 'string' ? l.source : (l.source as Node).id;
        const t = typeof l.target === 'string' ? l.target : (l.target as Node).id;
        return s === d.id || t === d.id ? 0.9 : 0.02;
      });
      link.attr('stroke-width', l => {
        const s = typeof l.source === 'string' ? l.source : (l.source as Node).id;
        const t = typeof l.target === 'string' ? l.target : (l.target as Node).id;
        return s === d.id || t === d.id ? 2.5 : 0.5;
      });
      label.style('opacity', n => connected.has(n.id) ? 1 : 0.03)
           .style('display', n => connected.has(n.id) ? 'block' : 'none');
    }).on('mouseleave', () => {
      setHovered(null);
      node.attr('fill-opacity', 0.8).attr('stroke-opacity', 0.3);
      link.attr('stroke-opacity', d => Math.min(0.15 + d.weight * 0.1, 0.5))
          .attr('stroke-width', d => Math.min(d.weight * 0.5, 3));
      label.style('opacity', 1)
           .style('display', d => showLabels || d.linkCount >= 3 ? 'block' : 'none');
    });

    simulation.on('tick', () => {
      link.attr('x1', d => (d.source as Node).x!).attr('y1', d => (d.source as Node).y!)
          .attr('x2', d => (d.target as Node).x!).attr('y2', d => (d.target as Node).y!);
      node.attr('cx', d => d.x!).attr('cy', d => d.y!);
      label.attr('x', d => d.x!).attr('y', d => d.y!);
    });

    return () => { simulation.stop(); };
  }, [articles, showLabels, selectedTag, linkThreshold, lang, contract, navigate]);

  if (loading) return <Loading text="Building graph..." />;

  return (
    <div className="h-full flex flex-col">
      {/* Controls */}
      <div className="flex items-center gap-3 p-4 border-b border-outline-variant/30 flex-wrap">
        <h1 className="font-headline text-lg font-bold flex items-center gap-2">
          <Icon name="hub" className="text-primary" />
          {zh ? '知识图谱' : 'Knowledge Graph'}
        </h1>
        <div className="flex-1" />

        {/* Tag filter */}
        <div className="flex items-center gap-1 flex-wrap">
          <button
            onClick={() => setSelectedTag(null)}
            className={`px-2 py-0.5 rounded-full text-[11px] transition-colors ${
              !selectedTag ? 'bg-primary text-on-primary' : 'bg-surface-high text-on-surface-variant hover:bg-surface-highest'
            }`}>
            {zh ? '全部' : 'All'}
          </button>
          {topTags.slice(0, 10).map(t => (
            <button key={t} onClick={() => setSelectedTag(selectedTag === t ? null : t)}
              className={`px-2 py-0.5 rounded-full text-[11px] transition-colors ${
                selectedTag === t ? 'text-on-primary' : 'text-on-surface-variant hover:bg-surface-highest'
              }`}
              style={selectedTag === t ? { backgroundColor: tagColors[t] } : { backgroundColor: 'var(--c-surface-high)' }}>
              {t}
            </button>
          ))}
        </div>

        {/* Link threshold slider */}
        <div className="flex items-center gap-2 text-xs text-on-surface-variant">
          <span>{zh ? '连接密度' : 'Density'}</span>
          <input type="range" min={1} max={4} value={linkThreshold}
            onChange={e => setLinkThreshold(+e.target.value)}
            className="w-16 h-1 accent-primary" />
          <span className="w-3 text-center">{linkThreshold}</span>
        </div>

        <span className="text-xs text-outline">
          {visibleCount ?? articles.length} {zh ? '篇' : 'nodes'}
        </span>

        <label className="flex items-center gap-1.5 text-xs text-on-surface-variant cursor-pointer">
          <input type="checkbox" checked={showLabels} onChange={e => setShowLabels(e.target.checked)} className="rounded" />
          {zh ? '标签' : 'Labels'}
        </label>
      </div>

      {/* Graph + Info panel */}
      <div className="flex-1 relative flex">
        <svg ref={svgRef} className="flex-1 h-full" />

        {hovered && (
          <div className="absolute bottom-4 left-4 bg-surface-container border border-outline-variant/30 rounded-xl p-4 max-w-[280px] shadow-xl">
            <h3 className="font-serif font-semibold text-on-surface mb-1">{hovered.localTitle}</h3>
            {hovered.summary && (
              <p className="text-xs text-on-surface-variant mb-2 line-clamp-2">{hovered.summary}</p>
            )}
            <div className="flex items-center gap-3 text-[11px] text-outline">
              <span>{hovered.linkCount} {zh ? '连接' : 'connections'}</span>
              <span>{hovered.tags.filter(t => !t.startsWith('category:')).slice(0, 3).join(', ')}</span>
            </div>
          </div>
        )}

        {articles.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-on-surface-variant">
            <Icon name="hub" className="text-5xl mb-3 block" />
            <p>{zh ? '没有文章可以可视化' : 'No articles to visualize'}</p>
          </div>
        )}
      </div>
    </div>
  );
}
