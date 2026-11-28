import { ChevronDown, LoaderCircle } from 'lucide-react';

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '../../components/ui/tooltip.tsx';
import type {
  AgentIntegration,
  AgentNotice,
  AgentProblem,
  AgentsSnapshot,
} from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { agentIcon, displayPath, noticeKeys, stateKeys } from '../types.ts';
import { AgentGuideDialog } from './AgentGuideDialog.tsx';

function AgentDetailStatus({ label, messages }: { label: string; messages: string[] }) {
  if (messages.length === 0) return <span>{label}</span>;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button type="button" className="agent-detail-status">
          {label}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom" align="end">
        {messages.map((message, index) => (
          <div key={index}>{message}</div>
        ))}
      </TooltipContent>
    </Tooltip>
  );
}

export function AgentsPage({
  native,
  agents,
  loadError,
  list,
  busy,
  selected,
  pendingAgents,
  problems,
  onToggle,
}: {
  native: boolean;
  agents: AgentsSnapshot | null;
  loadError: string;
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
        ((agent.hosts?.length ?? 0) > 0 &&
          (agent.managed || agent.state === 'conflict' || agent.hosts.some((host) => host.error))),
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
      <section
        className="settings-list agent-list"
        aria-busy={native && agents === null && !loadError}
      >
        {list.length > 0 && (
          <>
            <div className="agent-heading">
              <div>
                <h2>{t('supportedAgents')}</h2>
              </div>
            </div>
            <TooltipProvider>
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
                  const chip = (
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
                      aria-pressed={agent.error && !agent.managed ? undefined : on}
                      aria-disabled={agent.error && !agent.managed ? true : undefined}
                      aria-busy={pendingAgents.includes(agent.id) || !!agents?.syncing}
                      aria-label={
                        agent.error
                          ? agent.managed
                            ? t('forgetAgent', { agent: agent.name })
                            : agent.name
                          : conflict
                            ? t('overrideAgent', { agent: agent.name })
                            : on
                              ? t('removeAgent', { agent: agent.name })
                              : t('installAgent', { agent: agent.name })
                      }
                      title={
                        agent.error
                          ? undefined
                          : conflict
                            ? `${agent.conflicts[0]?.message ?? t('agentConflict')} · ${t('agentOverrideHint')}`
                            : (agent.conflicts[0]?.message ??
                              `${instanceTitle} · ${t(stateKeys[agent.state])}`)
                      }
                      onClick={() => {
                        if (!agent.error || agent.managed) onToggle(agent);
                      }}
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
                  return agent.error ? (
                    <Tooltip key={agent.id}>
                      <TooltipTrigger asChild>{chip}</TooltipTrigger>
                      <TooltipContent side="bottom" align="start">
                        {agent.error}
                        {agent.managed && <> · {t('forgetAgentHint')}</>}
                      </TooltipContent>
                    </Tooltip>
                  ) : (
                    chip
                  );
                })}
              </div>
            </TooltipProvider>
          </>
        )}
        {agents?.syncing && <p className="hint">{t('agentSyncing')}</p>}
        {list.length === 0 && (
          <p className="hint">
            {native && agents === null && !loadError ? t('agentDetecting') : t('noAgents')}
          </p>
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
        <TooltipProvider>
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
                      <AgentDetailStatus
                        label={t(stateKeys[host.state])}
                        messages={[
                          ...(host.error ? [host.error] : []),
                          ...host.conflicts.map((item) => `${item.target}: ${item.message}`),
                        ]}
                      />
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
                    <AgentDetailStatus
                      label={t(stateKeys[agent.state])}
                      messages={agent.conflicts.map((item) => `${item.target}: ${item.message}`)}
                    />
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
                  {agent.reloadHint && <p className="hint">{noticeText(agent.reloadHint)}</p>}
                </div>
              ),
            )}
          </details>
        </TooltipProvider>
      )}
      <AgentGuideDialog native={native} />
    </section>
  );
}
