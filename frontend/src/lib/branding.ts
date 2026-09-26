/**
 * Branding configuration — fetched from backend /api/branding.
 * Configure via config.yaml `branding:` section in each project.
 */

export interface Branding {
  name: string;
  nameShort: string;
  tagline: string;
  poweredBy: { label: string; url: string };
}

const DEFAULT_BRANDING: Branding = {
  name: 'LLMBase',
  nameShort: 'L',
  tagline: 'Knowledge Base',
  poweredBy: { label: 'Powered by LLMBase', url: 'https://github.com/Hosuke/llmbase' },
};

let _cached: Branding | null = null;

export async function fetchBranding(): Promise<Branding> {
  if (_cached) return _cached;
  try {
    const res = await fetch('/api/branding');
    if (res.ok) {
      const data = await res.json();
      const branding: Branding = { ...DEFAULT_BRANDING, ...data };
      _cached = branding;
      return branding;
    }
  } catch { /* fallback */ }
  _cached = DEFAULT_BRANDING;
  return _cached;
}

export function getBranding(): Branding {
  return _cached || DEFAULT_BRANDING;
}
