import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';

import { describe } from './api.ts';
import type {
  DoctorReport,
  HostDetection,
  HostKind,
  IntegrationAction,
  IntegrationDetection,
  IntegrationMode,
  IntegrationPlan,
  IntegrationRequest,
  IntegrationStatus,
  ManagementSnapshot,
} from './generated/protocol.ts';

import './styles.css';

function Admin() {
  const [state, setState] = useState<ManagementSnapshot | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [error, setError] = useState('');
  const [working, setWorking] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [integrations, setIntegrations] = useState<HostDetection[]>([]);
  const [modes, setModes] = useState<Record<HostKind, IntegrationMode>>({
    codex: 'skill-mcp',
    dsh: 'plugin',
  });
  const [profiles, setProfiles] = useState<Record<HostKind, string>>({
    codex: 'default',
    dsh: 'desktop',
  });
  const [plans, setPlans] = useState<Partial<Record<HostKind, IntegrationPlan>>>({});
  const [statuses, setStatuses] = useState<Partial<Record<HostKind, IntegrationStatus>>>({});
  const native = '__TAURI_INTERNALS__' in window;
  useEffect(() => {
    if (!native) return;
    void detectIntegrations().catch((e) => setError(describe(e)));
    let live = true;
    const refresh = async () => {
      try {
        const data = await invoke<ManagementSnapshot>('management_status');
        if (live) setState(data);
      } catch (e) {
        if (live) setError(describe(e));
      }
    };
    void invoke<ManagementSnapshot>('management_start')
      .then((data) => {
        if (live) setState(data);
      })
      .catch((e) => {
        if (live) setError(describe(e));
      })
      .finally(() => {
        void refresh();
      });
    const timer = setInterval(() => void refresh(), 2000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [native]);
  async function detectIntegrations() {
    const detection = await invoke<IntegrationDetection>('management_integration_detect');
    setIntegrations(detection.hosts);
    const nextProfiles = { ...profiles };
    for (const host of detection.hosts) {
      if (!nextProfiles[host.host]) {
        nextProfiles[host.host] = host.profiles[0] ?? (host.host === 'dsh' ? 'desktop' : 'default');
      }
    }
    setProfiles(nextProfiles);
    for (const host of detection.hosts) {
      try {
        const status = await integrationCall<IntegrationStatus>('management_integration_status', {
          action: 'status',
          host: host.host,
          profile: nextProfiles[host.host],
          mode: modes[host.host],
          planHash: null,
        });
        setStatuses((current) => ({ ...current, [host.host]: status }));
      } catch {
        // Detection remains the source of truth when no ownership record exists.
      }
    }
  }
  function integrationRequest(
    host: HostKind,
    action: IntegrationAction,
    planHash: string | null = null,
  ): IntegrationRequest {
    return {
      action,
      host,
      profile: profiles[host],
      mode: modes[host],
      planHash,
    };
  }
  async function integrationCall<T>(command: string, request: IntegrationRequest): Promise<T> {
    return invoke<T>(command, { request });
  }
  async function integrationAction(host: HostKind, action: IntegrationAction) {
    setWorking(true);
    setError('');
    try {
      if (action === 'plan' || action === 'apply') {
        const plan = await integrationCall<IntegrationPlan>(
          'management_integration_plan',
          integrationRequest(host, 'plan'),
        );
        setPlans((current) => ({ ...current, [host]: plan }));
        if (action === 'plan') return;
        const status = await integrationCall<IntegrationStatus>(
          'management_integration_apply',
          integrationRequest(host, 'apply', plan.planHash),
        );
        setStatuses((current) => ({ ...current, [host]: status }));
        setPlans((current) => ({ ...current, [host]: plan }));
      } else {
        const actionCommand =
          action === 'repair' ? 'management_integration_repair' : 'management_integration_remove';
        const status = await integrationCall<IntegrationStatus>(
          actionCommand,
          integrationRequest(host, action),
        );
        setStatuses((current) => ({ ...current, [host]: status }));
        setPlans((current) => ({ ...current, [host]: undefined }));
      }
    } catch (e) {
      setError(describe(e));
    } finally {
      setWorking(false);
    }
  }
  async function action(command: string) {
    setWorking(true);
    setError('');
    try {
      if (command === 'management_doctor') setDoctor(await invoke<DoctorReport>(command));
      else await invoke(command);
      setState(await invoke<ManagementSnapshot>('management_status'));
    } catch (e) {
      setError(describe(e));
    } finally {
      setWorking(false);
    }
  }
  return (
    <main className="admin">
      <header>
        <h1>Ossia 管理</h1>
        <span role="status">{state?.runtime ? '服务运行中' : '服务未连接'}</span>
      </header>
      {!native && (
        <p className="error" role="alert">
          请在 Ossia 桌面应用中打开管理窗口。浏览器谱面无需桌面桥接。
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
      <section>
        <h2>宿主集成</h2>
        {integrations.length === 0 && (
          <p className="hint">尚未检测宿主。请在桌面应用中刷新状态。</p>
        )}
        {integrations.map((host) => {
          const status = statuses[host.host];
          const plan = plans[host.host];
          return (
            <article className="integration-card" key={host.host}>
              <header>
                <div>
                  <h3>{host.host === 'codex' ? 'Codex' : 'DeepSeek Harness'}</h3>
                  <p className="hint">
                    {host.installed
                      ? `${host.version ?? '版本未知'} · ${host.executable}`
                      : (host.problem?.message ?? '未安装')}
                  </p>
                </div>
                <span className={status?.state === 'installed' ? 'ok' : 'warn'}>
                  {status?.state ?? (host.installed ? 'available' : 'unavailable')}
                </span>
              </header>
              <dl>
                <dt>配置根</dt>
                <dd>{host.configRoot}</dd>
                <dt>目标 profile</dt>
                <dd>
                  <select
                    disabled={!native || working}
                    value={profiles[host.host]}
                    onChange={(event) =>
                      setProfiles((current) => ({ ...current, [host.host]: event.target.value }))
                    }
                  >
                    {(host.profiles.length ? host.profiles : [profiles[host.host]]).map(
                      (profile) => (
                        <option key={profile} value={profile}>
                          {profile}
                        </option>
                      ),
                    )}
                  </select>
                </dd>
                <dt>接入模式</dt>
                <dd>
                  <select
                    disabled={!native || working}
                    value={modes[host.host]}
                    onChange={(event) =>
                      setModes((current) => ({
                        ...current,
                        [host.host]: event.target.value as IntegrationMode,
                      }))
                    }
                  >
                    <option value="skill-mcp">Skill + MCP</option>
                    <option value="plugin">本地插件</option>
                  </select>
                </dd>
              </dl>
              {status?.message && <p className="warn">{status.message}</p>}
              {status?.conflicts.map((item) => (
                <p className="error" key={`${item.target}:${item.kind}`}>
                  {item.target}: {item.message}
                </p>
              ))}
              <div className="row">
                <button
                  disabled={!native || working || !host.installed}
                  type="button"
                  onClick={() => void integrationAction(host.host, 'plan')}
                >
                  生成计划
                </button>
                <button
                  disabled={!native || working || !plan || plan.conflicts.length > 0}
                  type="button"
                  onClick={() => void integrationAction(host.host, 'apply')}
                >
                  应用计划
                </button>
                <button
                  disabled={!native || working || !status?.management}
                  type="button"
                  onClick={() => void integrationAction(host.host, 'repair')}
                >
                  修复
                </button>
                <button
                  disabled={!native || working || !status?.management}
                  type="button"
                  onClick={() => void integrationAction(host.host, 'remove')}
                >
                  移除
                </button>
              </div>
              {plan && (
                <details>
                  <summary>
                    计划 {plan.planHash.slice(0, 12)} · {plan.entries.length} 项
                  </summary>
                  {plan.warnings.map((warning) => (
                    <p className="warn" key={warning}>
                      {warning}
                    </p>
                  ))}
                  <pre>{JSON.stringify(plan, null, 2)}</pre>
                </details>
              )}
            </article>
          );
        })}
      </section>
    </main>
  );
}
const root = document.getElementById('root');
if (!root) throw new Error('missing #root');
createRoot(root).render(<Admin />);
