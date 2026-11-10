const viewId = new URL(location.href).searchParams.get('viewId');
const credential = new URLSearchParams(location.hash.slice(1)).get('start');
history.replaceState(null, '', location.pathname + location.search);
const status = document.querySelector('#status');
const state = document.querySelector('#state');
const streamStatus = document.querySelector('#stream');
const buttons = [...document.querySelectorAll('[data-measure]')];
let token;
let stream;
let connections = 0;

async function request(route, body, signal) {
  const response = await fetch(route, {
    method: body ? 'POST' : 'GET',
    headers: {
      ...(body ? { 'Content-Type': 'application/json' } : {}),
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
    credentials: 'omit',
    signal,
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response;
}
function show(value) {
  state.textContent = JSON.stringify(value, null, 2);
  for (const button of buttons)
    button.setAttribute(
      'aria-pressed',
      String(value.selection.includes(Number(button.dataset.measure))),
    );
}
function report(error) {
  status.textContent = `探针失败：${error.message}`;
}
async function connect() {
  stream?.abort();
  stream = new AbortController();
  const current = stream;
  const snapshot = await request(`/state?viewId=${viewId}`);
  show(await snapshot.json());
  try {
    const response = await request(`/events?viewId=${viewId}`, undefined, current.signal);
    connections++;
    streamStatus.textContent = `事件流已连接 ${connections} 次`;
    const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
    let buffer = '';
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += value;
      let boundary;
      while ((boundary = buffer.indexOf('\n\n')) !== -1) {
        const event = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        if (event.startsWith('data: ')) show(JSON.parse(event.slice(6)));
      }
    }
  } catch (error) {
    if (!current.signal.aborted) report(error);
  }
}
try {
  const response = await request('/bootstrap', { viewId, credential });
  ({ token } = await response.json());
  status.textContent = '同源凭据交换成功 · 不使用 Cookie';
  for (const button of buttons)
    button.addEventListener('click', () => {
      request(`/select?viewId=${viewId}`, { measure: Number(button.dataset.measure) })
        .then((r) => r.json())
        .then(show)
        .catch(report);
    });
  document.querySelector('#reconnect').addEventListener('click', () => {
    void connect().catch(report);
  });
  void connect().catch(report);
} catch (error) {
  report(error);
}
