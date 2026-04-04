import { createContext, useContext, useState, type ReactNode } from 'react';

export type Lang = 'zh' | 'en' | 'ja' | 'zh-en';

export const LANG_OPTIONS: { value: Lang; label: string; icon: string }[] = [
  { value: 'zh', label: '中文', icon: '中' },
  { value: 'en', label: 'English', icon: 'EN' },
  { value: 'ja', label: '日本語', icon: '日' },
  { value: 'zh-en', label: '中英双语', icon: '双' },
];

const LangContext = createContext<{
  lang: Lang;
  setLang: (l: Lang) => void;
}>({ lang: 'zh', setLang: () => {} });

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(() => {
    if (typeof window === 'undefined') return 'zh';
    return (localStorage.getItem('llmbase-lang') as Lang) || 'zh-en';
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

/**
 * Extract the localized part from a bilingual title like "English Title / 中文标题"
 */
export function localizeTitle(title: string, lang: Lang): string {
  if (!title) return '';
  const parts = title.split('/').map(s => s.trim());
  if (parts.length < 2) return title;

  const hasCJK = (s: string) => /[\u4e00-\u9fff\u3400-\u4dbf]/.test(s);

  if (lang === 'zh-en') return title; // Show both

  if (lang === 'zh' || lang === 'ja') {
    const cjk = parts.find(p => hasCJK(p));
    return cjk || parts[parts.length - 1];
  }
  const en = parts.find(p => !hasCJK(p));
  return en || parts[0];
}

/**
 * Extract language section(s) from trilingual article content
 */
export function extractLangContent(content: string, lang: Lang): string {
  if (lang === 'zh-en') {
    // Bilingual: show both English and Chinese sections
    const en = _extractSection(content, '## English');
    const zh = _extractSection(content, '## 中文');
    if (en && zh) return `## English\n\n${en}\n\n---\n\n## 中文\n\n${zh}`;
    return content;
  }

  const headers: Record<string, string> = {
    en: '## English',
    zh: '## 中文',
    ja: '## 日本語',
  };

  const marker = headers[lang];
  if (marker) {
    const section = _extractSection(content, marker);
    if (section) return section;
  }
  return content;
}

function _extractSection(content: string, marker: string): string | null {
  const idx = content.indexOf(marker);
  if (idx === -1) return null;
  const start = idx + marker.length;
  const nextH2 = content.indexOf('\n## ', start);
  return (nextH2 === -1 ? content.slice(start) : content.slice(start, nextH2)).trim();
}
