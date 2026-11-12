import { Music2 } from 'lucide-react';

import { useI18n } from '../../i18n.tsx';

export function ScoreNotFound() {
  const { t } = useI18n();
  return (
    <section className="not-found" role="main">
      <Music2 aria-hidden="true" size={28} />
      <p className="not-found-code">404</p>
      <h1>{t('scoreNotFoundTitle')}</h1>
      <p>{t('scoreNotFoundBody')}</p>
      <a href="/">{t('appName')}</a>
    </section>
  );
}
