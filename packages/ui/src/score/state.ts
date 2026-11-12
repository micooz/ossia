import type { AgentStatus, DocumentContext, Version } from '../generated/protocol.ts';
export interface AppState {
  connection: 'connecting' | 'connected' | 'reconnecting';
  context: DocumentContext | null;
  versions: Version[];
  status: AgentStatus | null;
}
export const initialState: AppState = {
  connection: 'connecting',
  context: null,
  versions: [],
  status: null,
};
export type Action =
  | {
      type: 'sync';
      context: DocumentContext | null;
      versions: Version[];
      status: AgentStatus;
    }
  | { type: 'context'; context: DocumentContext }
  | { type: 'connection'; connection: AppState['connection'] };
export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case 'sync':
      return {
        ...state,
        context: action.context,
        versions: action.versions,
        status: action.status,
        connection: 'connected',
      };
    case 'context':
      return { ...state, context: action.context };
    case 'connection':
      return { ...state, connection: action.connection };
  }
}
