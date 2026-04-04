import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';

export type Lang = 'en' | 'zh' | 'ja';

const LABELS: Record<Lang, string> = { en: 'EN', zh: '中', ja: '日' };
const FULL_LABELS: Record<Lang, string> = { en: 'English', zh: '中文', ja: '日本語' };

const LangContext = createContext<{
  lang: Lang;
  setLang: (l: Lang) => void;
}>({ lang: 'zh', setLang: () => {} });

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(() => {
    if (typeof window === 'undefined') return 'zh';
    return (localStorage.getItem('llmbase-lang') as Lang) || 'zh';
  });

  const setLang = (l: Lang) => {
    setLangState(l);
    localStorage.setItem('llmbase-lang', l);
  };

  return (
    <LangContext.Provider value={{ lang, setLang }}>
      {children}
    </LangContext.Provider>
  );
}

export const useLang = () => useContext(LangContext);
export { LABELS as LANG_LABELS, FULL_LABELS as LANG_FULL_LABELS };

/**
 * Extract the localized part from a bilingual title like "English Title / 中文标题"
 */
export function localizeTitle(title: string, lang: Lang): string {
  if (!title) return '';
  const parts = title.split('/').map(s => s.trim());
  if (parts.length < 2) return title;

  // Detect which part is which language
  const hasCJK = (s: string) => /[\u4e00-\u9fff\u3400-\u4dbf]/.test(s);
  const hasJP = (s: string) => /[\u3040-\u309f\u30a0-\u30ff]/.test(s);

  if (lang === 'zh' || lang === 'ja') {
    // Prefer CJK part
    const cjk = parts.find(p => hasCJK(p));
    return cjk || parts[parts.length - 1];
  }
  // English: prefer non-CJK part
  const en = parts.find(p => !hasCJK(p) && !hasJP(p));
  return en || parts[0];
}

/**
 * Extract a language section from trilingual article content
 */
export function extractLangContent(content: string, lang: Lang): string {
  const headers: Record<Lang, string[]> = {
    en: ['## English'],
    zh: ['## 中文'],
    ja: ['## 日本語'],
  };

  const markers = headers[lang];
  for (const marker of markers) {
    const idx = content.indexOf(marker);
    if (idx === -1) continue;
    const start = idx + marker.length;
    const nextH2 = content.indexOf('\n## ', start);
    return (nextH2 === -1 ? content.slice(start) : content.slice(start, nextH2)).trim();
  }
  return content;
}
