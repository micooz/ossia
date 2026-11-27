const installers = {
  '/darwin-arm64/latest/ossia-darwin-arm64.dmg': 'darwin-arm64',
  '/windows-amd64/latest/ossia-windows-amd64.exe': 'windows-amd64',
};

addEventListener('fetch', (event) => {
  event.respondWith(redirectLatest(event.request));
});

async function redirectLatest(request) {
  const url = new URL(request.url);
  const target = installers[url.pathname];
  if (!target) return new Response('Not found', { status: 404 });
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
  }

  try {
    const response = await fetch(`${url.origin}/${target}/index.json`);
    if (!response.ok) throw new Error(`Manifest returned ${response.status}`);
    const manifest = await response.json();
    if (
      manifest.schemaVersion !== 1 ||
      manifest.target !== target ||
      !Array.isArray(manifest.releases)
    ) {
      throw new Error('Invalid manifest');
    }
    const release = manifest.releases.find(
      (item) => item.prerelease === false && /^\d+\.\d+\.\d+$/.test(item.version),
    );
    if (!release) throw new Error('No stable release');
    const filename = url.pathname.slice(url.pathname.lastIndexOf('/') + 1);
    return new Response(null, {
      status: 302,
      headers: {
        Location: `${url.origin}/${target}/${release.version}/${filename}`,
        'Cache-Control': 'no-store',
      },
    });
  } catch (error) {
    console.error('Cannot resolve latest release:', error);
    return new Response('Latest release is temporarily unavailable', {
      status: 503,
      headers: { 'Cache-Control': 'no-store' },
    });
  }
}
