import { ArrowUpCircle, ExternalLink } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import type {
  ManagementPreferences,
  UpdateChannel,
  UpdateNotes,
  UpdateSnapshot,
  UpdateSource,
  UpdateState,
} from '../generated/protocol.ts';
import { useI18n } from '../i18n.tsx';
import icon from './assets/ossia-icon.svg';
import { Segmented } from './ui.tsx';
import { UpdateNotesDialog } from './UpdateNotesDialog.tsx';

const NOTICE_STATES = new Set<UpdateState>(['latest', 'no_release', 'source']);
const NOTICE_DURATION = 3_000;
const SHOW_UPDATE_FEED_SETTINGS = true;

export function AboutPage({
  preferences,
  preferencesBusy,
  native,
  update,
  onCheck,
  onApply,
  onCancel,
  onNotes,
  onAutoDownload,
  onChannel,
  onSource,
  onSetUpdateToken,
  onOpenExternal,
}: {
  preferences: ManagementPreferences | null;
  preferencesBusy: boolean;
  native: boolean;
  update: UpdateSnapshot | null;
  onCheck: () => void;
  onApply: () => void;
  onCancel: () => void;
  onNotes: () => Promise<UpdateNotes | null>;
  onAutoDownload: (enabled: boolean) => void;
  onChannel: (channel: UpdateChannel) => void;
  onSource: (source: UpdateSource) => void;
  onSetUpdateToken: (token: string | null) => Promise<boolean>;
  onOpenExternal: (url: string) => void;
}) {
  const { language, t } = useI18n();
  const [notes, setNotes] = useState<UpdateNotes | null>(null);
  const [notesBusy, setNotesBusy] = useState(false);
  const [notesUnavailable, setNotesUnavailable] = useState(false);
  const [token, setToken] = useState('');
  const [tokenBusy, setTokenBusy] = useState(false);
  const [tokenAction, setTokenAction] = useState<'save' | 'remove'>();
  const [tokenError, setTokenError] = useState('');
  const [notice, setNotice] = useState<UpdateState | null>(null);
  const previousState = useRef<UpdateState | null>(update?.state ?? null);

  const debug = update?.isDebugBuild ?? false;
  const actionable =
    update?.state === 'found' ||
    update?.state === 'ready' ||
    (update?.state === 'error' && update.latest !== null);
  const busy =
    update?.state === 'checking' ||
    update?.state === 'downloading' ||
    update?.state === 'installing';
  const percent =
    update?.state === 'downloading' && update.total
      ? `${Math.floor(((update.done ?? 0) / update.total) * 100)}%`
      : '';
  const message =
    debug && update?.state === 'ready'
      ? t('updateDebugReady')
      : update
        ? stateMessage(update.state, t)
        : t('updateIdle');
  const detail =
    update?.state === 'found' || update?.state === 'ready'
      ? update.latest
      : update?.state === 'error'
        ? update.error
        : [update?.latest, percent].filter(Boolean).join(' ') || null;
  const statusText = detail ? `${message} ${detail}` : message;
  const label =
    notice !== null
      ? stateMessage(notice, t)
      : debug && update?.state === 'ready'
        ? t('updateDebugReady')
        : actionable
          ? t(debug ? 'updateDebugDownload' : 'updateAction')
          : update?.state === 'downloading'
            ? [t('updateDownloading'), percent].filter(Boolean).join(' ')
            : update?.state === 'installing'
              ? t('updateInstalling')
              : update?.state === 'checking'
                ? t('updateChecking')
                : t('updateCheck');
  const mode =
    notice !== null ||
    (!update?.latest &&
      update?.state !== 'found' &&
      update?.state !== 'ready' &&
      update?.state !== 'downloading' &&
      update?.state !== 'installing')
      ? 'check'
      : busy
        ? 'progress'
        : 'update';
  const notesDisabled = !native || notesBusy || !preferences;

  useEffect(() => {
    const current = update?.state;
    if (!current) return;
    if (NOTICE_STATES.has(current) && previousState.current !== current) {
      previousState.current = current;
      setNotice(current);
      const timer = window.setTimeout(() => setNotice(null), NOTICE_DURATION);
      return () => window.clearTimeout(timer);
    }
    previousState.current = current;
    if (!NOTICE_STATES.has(current)) setNotice(null);
  }, [update?.state]);

  async function openNotes() {
    if (notesBusy) return;
    if (update?.notes) {
      setNotes(update.notes);
      return;
    }
    setNotesBusy(true);
    setNotesUnavailable(false);
    try {
      const loaded = await onNotes();
      if (loaded) setNotes(loaded);
      else setNotesUnavailable(true);
    } finally {
      setNotesBusy(false);
    }
  }

  async function saveToken(next: string | null) {
    if (next !== null && !next.trim()) {
      setTokenError(t('githubTokenRequired'));
      return;
    }
    setTokenAction(next === null ? 'remove' : 'save');
    setTokenBusy(true);
    setTokenError('');
    try {
      if (await onSetUpdateToken(next)) {
        setToken('');
      } else {
        setTokenError(t(next === null ? 'githubTokenRemoveFailed' : 'githubTokenSaveFailed'));
      }
    } finally {
      setTokenBusy(false);
      setTokenAction(undefined);
    }
  }

  return (
    <section className="page-section about-page" aria-labelledby="about-title">
      <div className="about-mark">
        <img src={icon} alt="" />
      </div>
      <h1 id="about-title">{t('aboutTitle')}</h1>
      <p className="page-description">{t('aboutBody')}</p>
      <div className="settings-list about-list">
        <div className="settings-row">
          <div className="setting-copy">
            <span className="setting-label">{t('version')}</span>
            {update?.state === 'error' && update.error && (
              <span className="setting-note">
                {t('updateFailed')}: <span>{update.error}</span>
              </span>
            )}
            {debug && (
              <span className="setting-note">
                {t('devVersionDescription', {
                  version: preferences?.appVersion ?? '—',
                })}
              </span>
            )}
          </div>
          <div className="update-version-actions">
            <span className="setting-value">{preferences?.appVersion ?? '—'}</span>
            <span className="visually-hidden" role="status">
              {statusText}
            </span>
            <button
              className="text-button"
              type="button"
              disabled={busy || (debug && update?.state === 'ready')}
              aria-busy={busy}
              onClick={() => (actionable ? onApply() : onCheck())}
            >
              {mode !== 'check' && (
                <span className="update-action-icon" aria-hidden="true">
                  <ArrowUpCircle />
                </span>
              )}
              <span>{label}</span>
            </button>
            {update?.state === 'downloading' && (
              <button className="secondary-button" type="button" onClick={onCancel}>
                {t('cancel')}
              </button>
            )}
          </div>
        </div>
        <div className="settings-row update-notes-row">
          <div className="setting-copy">
            <span className="setting-label">{t('updateNotes')}</span>
            {debug && <span className="setting-note">{t('devUpdateNotesDescription')}</span>}
            {notesUnavailable && <span className="setting-note">{t('noUpdateNotes')}</span>}
          </div>
          <button
            className="secondary-button"
            type="button"
            disabled={notesDisabled}
            aria-busy={notesBusy}
            onClick={() => void openNotes()}
          >
            {notesBusy ? t('loading') : t('updateOpenNotes')}
          </button>
        </div>
        <div className="settings-row">
          <div className="setting-copy">
            <span className="setting-label">{t('updateAutoDownload')}</span>
          </div>
          <button
            className="switch"
            type="button"
            role="switch"
            aria-label={t('updateAutoDownload')}
            aria-checked={preferences?.autoDownload ?? true}
            disabled={!native || !preferences || preferencesBusy}
            onClick={() => onAutoDownload(!(preferences?.autoDownload ?? true))}
          >
            <span />
          </button>
        </div>
        {SHOW_UPDATE_FEED_SETTINGS && (
          <>
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label">{t('updateFeedSource')}</span>
                <span className="setting-note">{t('updateFeedSourceDescription')}</span>
              </div>
              <Segmented
                ariaLabel={t('updateFeedSource')}
                value={preferences?.updateSource ?? 'ossia'}
                disabled={!native || !preferences || preferencesBusy || busy}
                compact
                options={[
                  { value: 'ossia', label: 'Ossia' },
                  { value: 'github', label: 'GitHub' },
                ]}
                onChange={(value) => onSource(value as UpdateSource)}
              />
            </div>
            {preferences?.updateSource === 'github' && (
              <div className="settings-row github-token-row update-token-row">
                <div className="setting-copy">
                  <div className="github-token-heading">
                    <label className="setting-label" htmlFor="update-github-token">
                      {t('githubToken')}
                    </label>
                    <button
                      className="github-token-link"
                      type="button"
                      aria-label={t('githubTokenCreate')}
                      title={t('githubTokenCreate')}
                      onClick={() =>
                        onOpenExternal('https://github.com/settings/personal-access-tokens/new')
                      }
                    >
                      <ExternalLink aria-hidden="true" />
                    </button>
                  </div>
                  <span className="setting-note">{t('githubTokenDescription')}</span>
                </div>
                <div className="github-token-control">
                  {preferences?.githubTokenMasked ? (
                    <div className="github-token-configured">
                      <code className="github-token-masked">{preferences.githubTokenMasked}</code>
                      <button
                        className="secondary-button"
                        type="button"
                        disabled={!native || tokenBusy}
                        onClick={() => void saveToken(null)}
                      >
                        {tokenAction === 'remove' ? t('githubTokenRemoving') : t('remove')}
                      </button>
                    </div>
                  ) : (
                    <form
                      className="github-token-form"
                      onSubmit={(event) => {
                        event.preventDefault();
                        void saveToken(token);
                      }}
                    >
                      <input
                        className="github-token-input"
                        id="update-github-token"
                        type="password"
                        autoComplete="off"
                        autoCapitalize="off"
                        spellCheck={false}
                        disabled={!native || !preferences || tokenBusy}
                        value={token}
                        onChange={(event) => {
                          setToken(event.target.value);
                          setTokenError('');
                        }}
                      />
                      <button
                        className="secondary-button"
                        type="submit"
                        disabled={!native || !preferences || tokenBusy}
                      >
                        {tokenAction === 'save' ? t('githubTokenSaving') : t('save')}
                      </button>
                    </form>
                  )}
                  {tokenError && (
                    <p className="github-token-error" role="alert">
                      {tokenError}
                    </p>
                  )}
                </div>
              </div>
            )}
          </>
        )}
        <div className="settings-row">
          <div className="setting-copy">
            <span className="setting-label">{t('updateChannel')}</span>
            <span className="setting-note">{t('updateChannelDescription')}</span>
          </div>
          <Segmented
            ariaLabel={t('updateChannel')}
            value={preferences?.updateChannel ?? 'stable'}
            disabled={!native || !preferences || preferencesBusy || busy}
            compact
            options={[
              { value: 'stable', label: t('updateChannelStable') },
              { value: 'beta', label: t('updateChannelBeta') },
            ]}
            onChange={(value) => onChannel(value as UpdateChannel)}
          />
        </div>
        <div className="settings-row">
          <span className="setting-label">{t('community')}</span>
          <button
            className="text-button"
            type="button"
            disabled={!native}
            onClick={() => onOpenExternal('https://github.com/micooz/ossia')}
          >
            GitHub
            <span className="update-action-icon" aria-hidden="true">
              <ExternalLink />
            </span>
          </button>
        </div>
      </div>
      {notes && (
        <UpdateNotesDialog
          notes={notes}
          language={language}
          onClose={() => setNotes(null)}
          onOpenLink={onOpenExternal}
        />
      )}
    </section>
  );
}

function stateMessage(state: UpdateState, t: ReturnType<typeof useI18n>['t']) {
  switch (state) {
    case 'idle':
      return t('updateIdle');
    case 'checking':
      return t('updateChecking');
    case 'auth_required':
      return t('updateTokenRequired');
    case 'latest':
      return t('updateLatest');
    case 'source':
      return t('updateSource');
    case 'no_release':
      return t('updateNoRelease');
    case 'found':
      return t('updateFound');
    case 'downloading':
      return t('updateDownloading');
    case 'ready':
      return t('updateReady');
    case 'installing':
      return t('updateInstalling');
    case 'error':
      return t('updateFailed');
  }
}
