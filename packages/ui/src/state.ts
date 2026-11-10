import type { DocumentState, StatusSnapshot, Version, ViewSnapshot } from './generated/protocol.ts';
export interface AppState {
  connection: 'connecting' | 'connected' | 'reconnecting' | 'expired';
  view: ViewSnapshot | null;
  documents: DocumentState[];
  versions: Version[];
  status: StatusSnapshot | null;
  error: string | null;
}
export const initialState: AppState = {
  connection: 'connecting',
  view: null,
  documents: [],
  versions: [],
  status: null,
  error: null,
};
export type Action =
  | {
      type: 'sync';
      view: ViewSnapshot;
      documents: DocumentState[];
      versions: Version[];
      status: StatusSnapshot;
    }
  | { type: 'view'; view: ViewSnapshot }
  | { type: 'connection'; connection: AppState['connection'] }
  | { type: 'error'; message: string | null };
export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case 'sync':
      return {
        ...state,
        view: action.view,
        documents: action.documents,
        versions: action.versions,
        status: action.status,
        connection: 'connected',
      };
    case 'view':
      return { ...state, view: action.view };
    case 'connection':
      return { ...state, connection: action.connection };
    case 'error':
      return { ...state, error: action.message };
  }
}
