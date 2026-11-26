import { ChevronDown, LoaderCircle } from 'lucide-react';

import type {
  AgentIntegration,
  AgentNotice,
  AgentProblem,
  AgentsSnapshot,
} from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { agentIcon, displayPath, noticeKeys, stateKeys } from '../types.ts';

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
    .filter(
      (agent) =>
        agent.installed ||
        ((agent.hosts?.length ?? 0) > 0 && (agent.managed || agent.state === 'conflict')),
    )
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
                const on =
                  agent.hosts.length > 0 ? agent.managed : agent.installed && agent.enabled;
                const conflict = agent.state === 'conflict';
                const duplicateName = list.filter((item) => item.name === agent.name).length > 1;
                const instanceTitle = duplicateName
                  ? `${agent.name} · ${displayPath(agent.configRoot)}`
                  : agent.name;
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
                    disabled={!native || busy || pendingAgents.includes(agent.id)}
                    type="button"
                    aria-pressed={on}
                    aria-busy={pendingAgents.includes(agent.id) || !!agents?.syncing}
                    aria-label={
                      conflict
                        ? t('overrideAgent', { agent: agent.name })
                        : on
                          ? t('removeAgent', { agent: agent.name })
                          : t('installAgent', { agent: agent.name })
                    }
                    title={
                      conflict
                        ? `${agent.conflicts[0]?.message ?? t('agentConflict')} · ${t('agentOverrideHint')}`
                        : (agent.conflicts[0]?.message ??
                          `${instanceTitle} · ${t(stateKeys[agent.state])}`)
                    }
                    onClick={() => onToggle(agent)}
                  >
                    <span className="agent-chip-icon" aria-hidden="true">
                      {pendingAgents.includes(agent.id) || agents?.syncing ? (
                        <LoaderCircle className="spinner" />
                      ) : icon?.monochrome ? (
                        <span
                          className="agent-monochrome-icon"
                          style={{
                            maskImage: `url("${icon.src}")`,
                            WebkitMaskImage: `url("${icon.src}")`,
                          }}
                        />
                      ) : icon ? (
                        <img src={icon.src} alt="" draggable={false} />
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
          {connected.map((agent) =>
            agent.hosts.length > 0 ? (
              agent.hosts.map((host) => (
                <article
                  className={`agent-detail is-${host.state}${selected === agent.id ? ' is-selected' : ''}`}
                  key={`${agent.id}:${host.name}`}
                >
                  <div className="agent-detail-head">
                    <strong>{host.name}</strong>
                    <span>{t(stateKeys[host.state])}</span>
                  </div>
                  <dl>
                    <dt>{t('agentVersion')}</dt>
                    <dd>{host.version || t('unavailable')}</dd>
                    <dt>{t('integrationMethod')}</dt>
                    <dd>
                      {host.method === 'plugin'
                        ? t('localPlugin')
                        : host.method === 'skill'
                          ? 'Skill'
                          : 'MCP'}
                    </dd>
                    <dt>{t('targetConfigPath')}</dt>
                    <dd>
                      <code>{displayPath(host.configPath)}</code>
                    </dd>
                  </dl>
                  {host.error && <p className="error">{host.error}</p>}
                  {host.conflicts.map((item) => (
                    <p className="error" key={`${item.target}:${item.kind}`}>
                      {item.target}: {item.message}
                    </p>
                  ))}
                  {host.reloadHint && <p className="hint">{noticeText(host.reloadHint)}</p>}
                </article>
              ))
            ) : (
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
                  <dt>{t('agentConfigRoot')}</dt>
                  <dd>
                    <code>{displayPath(agent.configRoot)}</code>
                  </dd>
                  {agent.path && (
                    <>
                      <dt>{t('pluginPath')}</dt>
                      <dd>
                        <code>{displayPath(agent.path)}</code>
                      </dd>
                    </>
                  )}
                  <dt>{t('targetConfigPath')}</dt>
                  <dd>
                    <code>{displayPath(agent.configPath)}</code>
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
            ),
          )}
        </details>
      )}
    </section>
  );
}
