import { ChevronDown, LoaderCircle } from 'lucide-react';

import type {
  AgentIntegration,
  AgentNotice,
  AgentProblem,
  AgentsSnapshot,
} from '../generated/protocol.ts';
import { useI18n } from '../i18n.tsx';
import { agentIcon, noticeKeys, stateKeys } from './types.ts';

export function AgentsPage({
  native,
  agents,
  list,
  busy,
  selected,
  pendingAgents,
  problems,
  onToggle,
}: {
  native: boolean;
  agents: AgentsSnapshot | null;
  list: AgentIntegration[];
  busy: boolean;
  selected: string | null;
  pendingAgents: string[];
  problems: AgentProblem[];
  onToggle: (agent: AgentIntegration) => void;
}) {
  const { t } = useI18n();
  function noticeText(notice: AgentNotice) {
    return t(
      noticeKeys[notice.kind],
      notice.kind === 'restart-session' ? { agent: notice.agent } : {},
    );
  }
  const connected = list
    .filter((agent) => agent.installed)
    .sort((left, right) => {
      const order = agents?.connectionOrder ?? [];
      const leftIndex = order.indexOf(left.id);
      const rightIndex = order.indexOf(right.id);
      return (
        (leftIndex < 0 ? order.length : leftIndex) - (rightIndex < 0 ? order.length : rightIndex)
      );
    });
  return (
    <section className="page-section" aria-labelledby="agents-title">
      <div className="page-intro">
        <h1 id="agents-title">{t('agentTitle')}</h1>
        <p>{t('agentDescription')}</p>
      </div>
      <section className="settings-list agent-list" aria-busy={native && agents === null}>
        {list.length > 0 && (
          <>
            <div className="agent-heading">
              <div>
                <h2>{t('supportedAgents')}</h2>
              </div>
            </div>
            <div className="agent-strip">
              {list.map((agent) => {
                const icon = agentIcon(agent);
                const on = agent.installed && agent.enabled;
                return (
                  <button
                    key={agent.id}
                    className={[
                      'agent-chip',
                      on ? 'is-on' : '',
                      `is-${agent.state}`,
                      agent.conflicts.length > 0 ? 'has-conflict' : '',
                      selected === agent.id ? 'is-selected' : '',
                    ]
                      .filter(Boolean)
                      .join(' ')}
                    disabled={!native || busy}
                    type="button"
                    aria-pressed={on}
                    aria-busy={pendingAgents.includes(agent.id) || !!agents?.syncing}
                    aria-label={
                      on
                        ? t('removeAgent', { agent: agent.name })
                        : t('installAgent', { agent: agent.name })
                    }
                    title={
                      agent.conflicts[0]?.message ?? `${agent.name} · ${t(stateKeys[agent.state])}`
                    }
                    onClick={() => onToggle(agent)}
                  >
                    <span className="agent-chip-icon" aria-hidden="true">
                      {pendingAgents.includes(agent.id) || agents?.syncing ? (
                        <LoaderCircle className="spinner" />
                      ) : icon ? (
                        <img src={icon} alt="" draggable={false} />
                      ) : (
                        agent.name.slice(0, 1)
                      )}
                    </span>
                    <span className="agent-chip-name">{agent.name}</span>
                    <span className="agent-chip-state">{t(stateKeys[agent.state])}</span>
                  </button>
                );
              })}
            </div>
          </>
        )}
        {agents?.syncing && <p className="hint">{t('agentSyncing')}</p>}
        {agents?.error && <p className="error">{agents.error.message}</p>}
        {list.length === 0 && (
          <p className="hint">{native && agents === null ? t('agentDetecting') : t('noAgents')}</p>
        )}
        {problems.length > 0 && (
          <div className="agent-problems" role="alert">
            {problems.map((problem) => (
              <p key={`${problem.agentId}:${problem.message}`}>
                {problem.agentId}: {problem.message}
              </p>
            ))}
          </div>
        )}
      </section>
      {connected.length > 0 && (
        <details className="agent-details" open>
          <summary>
            <ChevronDown aria-hidden="true" />
            {t('integrationDetails')}
          </summary>
          {connected.map((agent) => (
            <div
              className={`agent-detail is-${agent.state}${selected === agent.id ? ' is-selected' : ''}`}
              key={agent.id}
            >
              <div className="agent-detail-head">
                <strong>{agent.name}</strong>
                <span>{t(stateKeys[agent.state])}</span>
              </div>
              <dl>
                <dt>{t('agentVersion')}</dt>
                <dd>
                  {agent.version ?? t('unavailable')}
                  {agent.profile && ` (${agent.profile})`}
                </dd>
                <dt>{t('integrationMethod')}</dt>
                <dd>
                  {agent.method === 'plugin'
                    ? t('localPlugin')
                    : agent.method === 'skill'
                      ? 'Skill'
                      : 'MCP'}
                </dd>
                {agent.path && (
                  <>
                    <dt>{t('pluginPath')}</dt>
                    <dd>
                      <code>{agent.path}</code>
                    </dd>
                  </>
                )}
                <dt>{t('targetConfigPath')}</dt>
                <dd>
                  <code>{agent.configPath}</code>
                </dd>
              </dl>
              {agent.message && <p className="warn">{noticeText(agent.message)}</p>}
              {agent.conflicts.map((item) => (
                <p className="error" key={`${item.target}:${item.kind}`}>
                  {item.target}: {item.message}
                </p>
              ))}
              {agent.reloadHint && <p className="hint">{noticeText(agent.reloadHint)}</p>}
            </div>
          ))}
        </details>
      )}
    </section>
  );
}
