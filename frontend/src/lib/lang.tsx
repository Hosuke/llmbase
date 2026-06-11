import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';

export type Lang = string;

export interface LangSection {
  code: string;
  key: string;
  header: string;
  label: string;
  icon: string;
  title_hint: string;
}

export interface LangView {
  code: string;
  show: string[];
  label: string;
  icon: string;
  title_hint: string;
  primary: string;
}

export interface LanguageContract {
  name: string;
  sections: LangSection[];
  views: LangView[];
  default_lang: string;
  api_default_lang: string;
  codes: string[];
  single_section: boolean;
  bare_body: boolean;
}

export interface LangOption {
  value: Lang;
  label: string;
  icon: string;
}

interface LangContextValue {
  lang: Lang;
  setLang: (l: Lang) => void;
  options: LangOption[];
  contract: LanguageContract | null;
}

let _contract: LanguageContract | null = null;

export async function fetchLanguages(): Promise<LanguageContract | null> {
  if (_contract) return _contract;
  try {
    const res = await fetch('/api/languages');
    if (res.ok) {
      _contract = await res.json();
      return _contract;
    }
  } catch { /* identity fallback */ }
  return null;
}

export function getLanguageContract(): LanguageContract | null {
  return _contract;
}

const LangContext = createContext<LangContextValue>({
  lang: '',
  setLang: () => {},
  options: [],
  contract: null,
});

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang | null>(() => {
    if (typeof window === 'undefined') return null;
    return localStorage.getItem('llmbase-lang');
  });
  const [contract, setContract] = useState<LanguageContract | null>(getLanguageContract());

  const setLang = (l: Lang) => {
    setLangState(l);
    localStorage.setItem('llmbase-lang', l);
  };

  useEffect(() => {
    let active = true;
    fetchLanguages().then(nextContract => {
      if (!active || !nextContract) return;
      setContract(nextContract);

      const stored = localStorage.getItem('llmbase-lang');
      // Legacy pre-contract patch path: preserve whatever lang the deployment
      // historically used; bare_body rendering is identity and extract/localize
      // already fall back gracefully for unknown codes.
      const preservePatchedLang = nextContract.name === '_patched' && stored !== null;
      if (stored === null || (!preservePatchedLang && !nextContract.codes.includes(stored))) {
        setLang(nextContract.default_lang);
      }
    });
    return () => { active = false; };
  }, []);

  const options: LangOption[] = contract
    ? [...contract.sections, ...contract.views].map(item => ({
        value: item.code,
        label: item.label,
        icon: item.icon,
      }))
    : [];

  return (
    <LangContext.Provider value={{ lang: lang ?? '', setLang, options, contract }}>
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

  const contract = getLanguageContract();
  if (!contract || contract.single_section) return title;

  const parts = title.split('/').map(s => s.trim());
  if (parts.length < 2) return title;

  const hasCJK = (s: string) => /[\u4e00-\u9fff\u3400-\u4dbf]/.test(s);
  const item = contract.sections.find(section => section.code === lang)
    ?? contract.views.find(view => view.code === lang);
  const hint = item?.title_hint ?? 'full';

  if (hint === 'full') return title;
  if (hint === 'cjk') {
    const cjk = parts.find(p => hasCJK(p));
    return cjk || parts[parts.length - 1];
  }
  if (hint === 'latin') {
    const latin = parts.find(p => !hasCJK(p));
    return latin || parts[0];
  }
  return title;
}

/**
 * Extract language section(s) from article content.
 */
export function extractLangContent(content: string, lang: Lang): string {
  const contract = getLanguageContract();
  if (!contract || contract.bare_body) return content;

  const view = contract.views.find(item => item.code === lang);
  if (view) {
    const blocks = view.show.map(code => {
      const section = contract.sections.find(item => item.code === code);
      if (!section) return null;
      const body = _extractSection(content, section.header);
      if (!body) return null;
      return `${section.header}\n\n${body}`;
    });
    if (blocks.every((block): block is string => block !== null)) {
      return blocks.join('\n\n---\n\n');
    }
    return content;
  }

  const section = contract.sections.find(item => item.code === lang);
  if (section) {
    const body = _extractSection(content, section.header);
    if (body) return body;
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
