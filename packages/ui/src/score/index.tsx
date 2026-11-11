import { createRoot } from 'react-dom/client';

import { describe } from '../errors.ts';
import { matchSystemLanguage, translate } from '../i18n.tsx';
import { resolveSession } from './endpoint.ts';
import { ScorePage } from './ScorePage.tsx';

import './score.css';

const element = document.getElementById('root');
if (!element) throw new Error('missing #root');
const root = createRoot(element);
const fallbackLanguage = matchSystemLanguage(navigator.languages);
document.documentElement.lang = fallbackLanguage;
root.render(
  <main className="start panel">
    <h1>Ossia</h1>
    <p>{translate(fallbackLanguage, 'bootConnecting')}</p>
  </main>,
);
resolveSession(fallbackLanguage)
  .then(({ session }) => root.render(<ScorePage session={session} />))
  .catch((e) =>
    root.render(
      <main className="start panel">
        <h1>{translate(fallbackLanguage, 'openScoreTitle')}</h1>
        <p role="alert">{describe(e)}</p>
      </main>,
    ),
  );
