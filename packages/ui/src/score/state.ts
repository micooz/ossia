import type {
  AgentStatus,
  DocumentContext,
  DocumentSummary,
  Version,
} from '../generated/protocol.ts';
export interface AppState {
  connection: 'connecting' | 'connected' | 'reconnecting';
  context: DocumentContext | null;
  documents: DocumentSummary[];
  versions: Version[];
  status: AgentStatus | null;
  error: string | null;
}
export const initialState: AppState = {
  connection: 'connecting',
  context: null,
  documents: [],
  versions: [],
  status: null,
  error: null,
};
export type Action =
  | {
      type: 'sync';
      context: DocumentContext | null;
      documents: DocumentSummary[];
      versions: Version[];
      status: AgentStatus;
    }
  | { type: 'context'; context: DocumentContext }
  | { type: 'connection'; connection: AppState['connection'] }
  | { type: 'error'; message: string | null };
export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case 'sync':
      return {
        ...state,
        context: action.context,
        documents: action.documents,
        versions: action.versions,
        status: action.status,
        connection: 'connected',
      };
    case 'context':
      return { ...state, context: action.context };
    case 'connection':
      return { ...state, connection: action.connection };
    case 'error':
      return { ...state, error: action.message };
  }
}
