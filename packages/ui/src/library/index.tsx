import '../polyfills.ts';
import { createRoot } from 'react-dom/client';

import { I18nProvider, matchSystemLanguage } from '../i18n.tsx';
import { MyScoresPage } from '../main/components/MyScoresPage.tsx';

import '../main/main.css';
import './library.css';

const language = matchSystemLanguage(navigator.languages);
document.documentElement.lang = language;

const root = document.getElementById('root');
if (!root) throw new Error('missing #root');
createRoot(root).render(
  <I18nProvider language={language}>
    <main className="library-shell">
      <MyScoresPage
        browser
        native={false}
        runtimeRunning
        runtimeInstanceId={null}
        startingService={false}
        notify={() => {}}
        onStartService={() => {}}
      />
    </main>
  </I18nProvider>,
);
