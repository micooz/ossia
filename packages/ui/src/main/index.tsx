import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';

import { ToastRegion, useToasts } from '../components/Toast.tsx';
import { describe } from '../errors.ts';
import type { ManagementPreferences, UiLanguage } from '../generated/protocol.ts';
import { I18nProvider, matchSystemLanguage } from '../i18n.tsx';
import { MainContent } from './MainContent.tsx';
import { platformName } from './types.ts';

import './main.css';

if ('__TAURI_INTERNALS__' in window) {
  document.addEventListener('contextmenu', (event) => event.preventDefault());
}

function Main() {
  const native = '__TAURI_INTERNALS__' in window;
  const detectedLanguage = matchSystemLanguage(navigator.languages);
  const toast = useToasts();
  const [preferences, setPreferences] = useState<ManagementPreferences | null>(null);
  const latestLanguage = useRef<UiLanguage | null>(null);

  useEffect(() => {
    document.body.dataset.platform = !native ? 'browser' : platformName();
  }, [native]);

  useEffect(() => {
    if (!native) return;
    let live = true;
    void invoke<ManagementPreferences>('management_preferences')
      .then(async (loaded) => {
        if (!loaded.language) {
          const initialized = await invoke<ManagementPreferences>('management_preferences_change', {
            change: { language: detectedLanguage },
          });
          if (live)
            setPreferences({
              ...initialized,
              language: latestLanguage.current ?? initialized.language,
            });
        } else if (live) {
          setPreferences({ ...loaded, language: latestLanguage.current ?? loaded.language });
        }
      })
      .catch((error) => {
        if (live) toast.show(describe(error), 'error');
      });
    return () => {
      live = false;
    };
  }, [detectedLanguage, native]);

  useEffect(() => {
    if (!native) return;
    let live = true;
    let dispose: (() => void) | undefined;
    void listen<UiLanguage>('ui-language-changed', (event) => {
      if (live) {
        latestLanguage.current = event.payload;
        setPreferences((current) => current && { ...current, language: event.payload });
      }
    }).then((unlisten) => {
      if (live) dispose = unlisten;
      else unlisten();
    });
    return () => {
      live = false;
      dispose?.();
    };
  }, [native]);

  const theme = preferences?.theme ?? 'system';
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme === 'system' ? 'light dark' : theme;
  }, [theme]);

  const language = preferences?.language ?? detectedLanguage;
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  return (
    <I18nProvider language={language}>
      <MainContent
        native={native}
        preferences={preferences}
        setPreferences={setPreferences}
        notify={toast.show}
      />
      <ToastRegion toasts={toast.toasts} onDismiss={toast.dismiss} />
    </I18nProvider>
  );
}

const root = document.getElementById('root');
if (!root) throw new Error('missing #root');
createRoot(root).render(<Main />);
