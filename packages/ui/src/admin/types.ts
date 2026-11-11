import codexIcon from '../assets/agents/codex.svg';
import dshIcon from '../assets/agents/dsh.svg';
import type { AgentIntegration, AgentState } from '../generated/protocol.ts';
import type { MessageKey } from '../i18n.tsx';

export type Page = 'service' | 'agents' | 'general' | 'about';

export const stateKeys: Record<AgentState, MessageKey> = {
  available: 'stateAvailable',
  installed: 'stateInstalled',
  disabled: 'stateDisabled',
  broken: 'stateBroken',
  conflict: 'stateConflict',
  unavailable: 'stateUnavailable',
  'update-required': 'stateUpdateRequired',
};

export function agentIcon(agent: AgentIntegration) {
  if (agent.icon === 'codex') return codexIcon;
  if (agent.icon === 'dsh') return dshIcon;
  return null;
}

export function platformName() {
  if (/Macintosh|Mac OS X/.test(navigator.userAgent)) return 'macos';
  if (/Windows/.test(navigator.userAgent)) return 'windows';
  return 'linux';
}
