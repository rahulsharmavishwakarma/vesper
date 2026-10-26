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
  miningRoot: string | null;
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
/** Opens the native folder picker; choosing a folder is the opt-in. */
export async function chooseMiningRoot(key: string): Promise<WorkspaceUpdate | null> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({
    title: 'Choose a folder to learn words from',
    multiple: false,
    directory: true,
  });
  if (typeof path !== 'string') return null;
  return invoke<WorkspaceUpdate>('set_workspace_mining_root', { key, path });
}
export const clearMiningRoot = (key: string) =>
  invoke<WorkspaceUpdate>('set_workspace_mining_root', { key, path: null });
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
