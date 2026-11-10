import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';

import { describe } from './api.ts';
import type { DoctorReport, ManagementSnapshot } from './generated/protocol.ts';

import './styles.css';

function Admin() {
  const [state, setState] = useState<ManagementSnapshot | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [error, setError] = useState('');
  const [working, setWorking] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const native = '__TAURI_INTERNALS__' in window;
  useEffect(() => {
    if (!native) return;
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
      .finally(() => void refresh());
    const timer = setInterval(() => void refresh(), 2000);
    return () => {
      live = false;
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
            {doctor.legacyDataPresent && (
              <p className="warn">检测到旧数据，需等待 P07 切换；不会自动清理。</p>
            )}
            <details>
              <summary>诊断详情</summary>
              <pre>{JSON.stringify(doctor, null, 2)}</pre>
            </details>
          </div>
        )}
      </section>
      <section>
        <h2>宿主集成</h2>
        <p>尚未接入。P06 将提供宿主检测、安装、修复与移除。</p>
      </section>
    </main>
  );
}
const root = document.getElementById('root');
if (!root) throw new Error('missing #root');
createRoot(root).render(<Admin />);
