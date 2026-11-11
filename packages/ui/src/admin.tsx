import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';

import { describe } from './api.ts';
import codexIcon from './assets/agents/codex.svg';
import dshIcon from './assets/agents/dsh.svg';
import type {
  AgentIntegration,
  AgentProblem,
  AgentState,
  AgentsMutation,
  AgentsSnapshot,
  DoctorReport,
  ManagementSnapshot,
} from './generated/protocol.ts';

import './styles.css';

const stateLabels: Record<AgentState, string> = {
  available: '未接入',
  installed: '已接入',
  disabled: '已禁用',
  broken: '需修复',
  conflict: '冲突',
  unavailable: '不可用',
  'update-required': '需更新',
};

function agentIcon(agent: AgentIntegration) {
  if (agent.icon === 'codex') return codexIcon;
  if (agent.icon === 'dsh') return dshIcon;
  return null;
}

function Admin() {
  const [state, setState] = useState<ManagementSnapshot | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [error, setError] = useState('');
  const [working, setWorking] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [agents, setAgents] = useState<AgentsSnapshot | null>(null);
  const [agentProblems, setAgentProblems] = useState<AgentProblem[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const native = '__TAURI_INTERNALS__' in window;

  async function loadAgents() {
    try {
      setAgents(await invoke<AgentsSnapshot>('management_agents_state'));
    } catch (caught) {
      setError(describe(caught));
    }
  }

  useEffect(() => {
    if (!native) return;
    void loadAgents();
    let live = true;
    let unlisten: (() => void) | undefined;
    void listen('agents-updated', () => {
      if (live) void loadAgents();
    }).then((dispose) => {
      unlisten = dispose;
    });
    const refresh = async () => {
      try {
        const data = await invoke<ManagementSnapshot>('management_status');
        if (live) setState(data);
      } catch (caught) {
        if (live) setError(describe(caught));
      }
    };
    void invoke<ManagementSnapshot>('management_start')
      .then((data) => {
        if (live) setState(data);
      })
      .catch((caught) => {
        if (live) setError(describe(caught));
      })
      .finally(() => {
        void refresh();
      });
    const timer = setInterval(() => void refresh(), 2000);
    return () => {
      live = false;
      unlisten?.();
      clearInterval(timer);
    };
  }, [native]);

  async function action(command: string) {
    setWorking(true);
    setError('');
    try {
      if (command === 'management_doctor') setDoctor(await invoke<DoctorReport>(command));
      else await invoke(command);
      setState(await invoke<ManagementSnapshot>('management_status'));
    } catch (caught) {
      setError(describe(caught));
    } finally {
      setWorking(false);
    }
  }

  async function changeAgents(agentIds: string[], enabled: boolean) {
    if (!native || working || agentIds.length === 0) return;
    setWorking(true);
    setError('');
    setAgentProblems([]);
    try {
      const result = await invoke<AgentsMutation>('management_agents_change', {
        change: { agentIds, enabled },
      });
      setAgents(result.snapshot);
      setAgentProblems(result.problems);
      if (result.problems.length > 0) {
        setError(result.problems.map((problem) => problem.message).join('；'));
      }
    } catch (caught) {
      setError(describe(caught));
    } finally {
      setWorking(false);
    }
  }

  function toggleAgent(agent: AgentIntegration) {
    if (agent.state === 'conflict' || agent.state === 'unavailable') {
      setSelected((current) => (current === agent.id ? null : agent.id));
      return;
    }
    if (agent.state === 'update-required') {
      void changeAgents([agent.id], true);
      return;
    }
    void changeAgents([agent.id], !(agent.installed && agent.enabled));
  }

  const list = agents?.agents ?? [];
  const eligible = list.filter(
    (agent) => agent.state !== 'conflict' && agent.state !== 'unavailable',
  );
  const everyOn =
    eligible.length > 0 && eligible.every((agent) => agent.installed && agent.enabled);
  const busy = working || !!agents?.syncing;

  return (
    <main className="admin">
      <header>
        <h1>ossia 管理</h1>
        <span role="status">{state?.runtime ? '服务运行中' : '服务未连接'}</span>
      </header>
      {!native && (
        <p className="error" role="alert">
          请在 ossia 桌面应用中打开管理窗口。浏览器谱面无需桌面桥接。
        </p>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <section>
        <h2>本地服务</h2>
        <dl>
          <dt>数据目录</dt>
          <dd>{state?.home ?? '等待读取'}</dd>
          <dt>工具链目录</dt>
          <dd>{state?.engineRoot ?? '等待读取'}</dd>
          <dt>运行时版本</dt>
          <dd>{state?.runtime?.runtimeVersion ?? '未连接'}</dd>
          <dt>访问地址</dt>
          <dd>{state?.runtime?.webBaseUrl ?? '未连接'}</dd>
          <dt>协议 / 数据版本</dt>
          <dd>
            {state?.runtime
              ? `${state.runtime.protocolVersion} / ${state.runtime.schemaVersion}`
              : '未连接'}
          </dd>
        </dl>
        {state?.error && <p className="warn">{state.error.message}</p>}
        <div className="row">
          <button
            disabled={!native || working || !!state?.runtime}
            type="button"
            onClick={() => void action('management_start')}
          >
            启动服务
          </button>
          <button
            disabled={!native || working || !state?.runtime}
            type="button"
            onClick={() => setConfirm(!confirm)}
          >
            停止服务
          </button>
          <button
            disabled={!native || working}
            type="button"
            onClick={() => void action('management_status')}
          >
            刷新状态
          </button>
        </div>
        {confirm && (
          <div>
            <p>停止会取消当前任务；未提交的源码会保留。确认停止？</p>
            <div className="row">
              <button type="button" onClick={() => setConfirm(false)}>
                取消
              </button>
              <button
                disabled={working}
                type="button"
                onClick={() => {
                  setConfirm(false);
                  void action('management_stop');
                }}
              >
                确认停止服务
              </button>
            </div>
          </div>
        )}
        <p className="hint">
          关闭此窗口后，本地服务和 Agent 任务继续运行。数据目录由启动参数选择。
        </p>
      </section>
      <section>
        <h2>诊断</h2>
        <button
          disabled={!native || working}
          type="button"
          onClick={() => void action('management_doctor')}
        >
          {working ? '处理中…' : '运行 doctor'}
        </button>
        {doctor && (
          <div>
            <p>工具链：{doctor.engine.ok ? doctor.engine.buildId : doctor.engine.error?.message}</p>
            <details>
              <summary>诊断详情</summary>
              <pre>{JSON.stringify(doctor, null, 2)}</pre>
            </details>
          </div>
        )}
      </section>
      <section className="agent-card">
        <div className="agent-card-heading">
          <div className="agent-card-copy">
            <h2>Agent 接入</h2>
            <p className="hint">
              通过本地插件把 Ossia 接入支持的 Agent；插件包含 Skill 与 MCP，安装后由宿主加载。
            </p>
          </div>
          <div className="agent-strip" aria-label="支持的 Agent">
            <button
              className={`agent-chip agent-chip-all${everyOn ? ' is-on' : ''}`}
              disabled={!native || busy || eligible.length === 0}
              type="button"
              aria-pressed={everyOn}
              onClick={() =>
                void changeAgents(
                  eligible.map((agent) => agent.id),
                  !everyOn,
                )
              }
            >
              全部
            </button>
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
                  aria-label={`${on ? '移除' : '安装'} ${agent.name} 插件`}
                  title={
                    agent.conflicts[0]?.message ?? `${agent.name} · ${stateLabels[agent.state]}`
                  }
                  onClick={() => toggleAgent(agent)}
                >
                  <span className="agent-chip-icon" aria-hidden="true">
                    {icon ? <img src={icon} alt="" draggable={false} /> : agent.name.slice(0, 1)}
                  </span>
                  <span className="agent-chip-name">{agent.name}</span>
                  <span className="agent-chip-state">{stateLabels[agent.state]}</span>
                </button>
              );
            })}
          </div>
        </div>
        {agents?.syncing && <p className="hint">正在后台同步已接入的 Agent…</p>}
        {agents?.error && <p className="error">{agents.error.message}</p>}
        {list.length === 0 && (
          <p className="hint">
            未检测到可接入的 Agent。如果已经安装 Codex 或 DSH，可设置 <code>OSSIA_CODEX_BIN</code> /{' '}
            <code>OSSIA_DSH_BIN</code> 后重新打开管理窗口。
          </p>
        )}
        {agentProblems.length > 0 && (
          <div className="agent-problems" role="alert">
            {agentProblems.map((problem) => (
              <p key={`${problem.agentId}:${problem.message}`}>
                {problem.agentId}: {problem.message}
              </p>
            ))}
          </div>
        )}
        {list.length > 0 && (
          <details className="agent-details" open>
            <summary>接入详情</summary>
            {list.map((agent) => (
              <div
                className={`agent-detail${selected === agent.id ? ' is-selected' : ''}`}
                key={agent.id}
              >
                <div className="agent-detail-head">
                  <strong>{agent.name}</strong>
                  <span>{stateLabels[agent.state]}</span>
                </div>
                <dl>
                  <dt>接入方式</dt>
                  <dd>
                    {agent.method === 'plugin'
                      ? '本地插件'
                      : agent.method === 'skill'
                        ? 'Skill'
                        : 'MCP'}
                  </dd>
                  <dt>宿主版本</dt>
                  <dd>{agent.version ?? '不可用'}</dd>
                  <dt>配置根</dt>
                  <dd>{agent.configRoot}</dd>
                  {agent.profile && (
                    <>
                      <dt>Profile</dt>
                      <dd>{agent.profile}</dd>
                    </>
                  )}
                  {agent.path && (
                    <>
                      <dt>插件位置</dt>
                      <dd>
                        <code>{agent.path}</code>
                      </dd>
                    </>
                  )}
                </dl>
                {agent.message && <p className="warn">{agent.message}</p>}
                {agent.conflicts.map((item) => (
                  <p className="error" key={`${item.target}:${item.kind}`}>
                    {item.target}: {item.message}
                  </p>
                ))}
                {agent.reloadHint && <p className="hint">{agent.reloadHint}</p>}
              </div>
            ))}
          </details>
        )}
      </section>
    </main>
  );
}

const root = document.getElementById('root');
if (!root) throw new Error('missing #root');
createRoot(root).render(<Admin />);
