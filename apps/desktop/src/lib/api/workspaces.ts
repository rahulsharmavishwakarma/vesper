import { invoke } from '@tauri-apps/api/core';

export type WorkspaceCategory =
  'code' | 'terminal' | 'email' | 'chat' | 'document' | 'notes' | 'browser' | 'other';
export interface WorkspaceSummary {
  key: string;
  label: string;
  kind: 'folder' | 'web';
  category: WorkspaceCategory;
  dictations: number;
  firstSeenDay: number;
  lastSeenDay: number;
  renamed: boolean;
}
export interface WorkspaceUpdate {
  workspaces: WorkspaceSummary[];
  profileChanged: boolean;
}
export const listWorkspaces = () => invoke<WorkspaceSummary[]>('list_workspaces');
export const renameWorkspace = (key: string, label: string) =>
  invoke<WorkspaceUpdate>('rename_workspace', { key, label });
export const mergeWorkspace = (from: string, into: string) =>
  invoke<WorkspaceUpdate>('merge_workspace', { from, into });
export const deleteWorkspace = (key: string) =>
  invoke<WorkspaceUpdate>('delete_workspace', { key });
export const workspaceKindLabel = (workspace: WorkspaceSummary) =>
  workspace.kind === 'web'
    ? 'Website'
    : ({
        code: 'Code',
        terminal: 'Terminal',
        email: 'Email',
        chat: 'Chat',
        document: 'Documents',
        notes: 'Notes',
        browser: 'Website',
        other: 'Folder',
      }[workspace.category] ?? 'Folder');
