import '../polyfills.ts';
import { createRoot } from 'react-dom/client';

import { matchSystemLanguage } from '../i18n.tsx';
import { ScorePage } from './ScorePage.tsx';

import './score.css';

const element = document.getElementById('root');
if (!element) throw new Error('missing #root');
const root = createRoot(element);
const fallbackLanguage = matchSystemLanguage(navigator.languages);
document.documentElement.lang = fallbackLanguage;
root.render(<ScorePage fallbackLanguage={fallbackLanguage} />);
