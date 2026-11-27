/** Return an installer target only when the browser reports its platform and architecture. */
export async function detectInstallerTarget(userAgentData) {
  if (
    !['macOS', 'Windows'].includes(userAgentData?.platform) ||
    !userAgentData.getHighEntropyValues
  )
    return null;
  try {
    const { architecture, bitness } = await userAgentData.getHighEntropyValues([
      'architecture',
      'bitness',
    ]);
    if (userAgentData.platform === 'macOS' && architecture === 'arm' && bitness === '64')
      return 'darwin-arm64';
    if (architecture === 'x86' && bitness === '64') {
      return userAgentData.platform === 'macOS' ? 'darwin-amd64' : 'windows-amd64';
    }
  } catch {
    // The browser may deny architecture information.
  }
  return null;
}
