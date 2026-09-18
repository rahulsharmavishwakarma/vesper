import { invoke } from '@tauri-apps/api/core';
export type DictationPhase =
  | 'idle'
  | 'starting'
  | 'recording'
  | 'finishing'
  | 'pasting'
  | 'ready'
  | 'sent'
  | 'cancelled'
  | 'error';
export interface DictationStatus {
  id: number;
  revision: number;
  phase: DictationPhase;
  mode: 'hold' | 'toggle';
  operationId: string | null;
  message: string;
  text: string | null;
  startedAtUnixMs: number | null;
  captureReadyMs: number | null;
  stopToFinalMs: number | null;
  finalToDispatchMs: number | null;
}
export interface DictationPermissions {
  supported: boolean;
  accessibility: boolean;
  eventPosting: boolean;
  clipboard: boolean;
}
export const getDictationStatus = () => invoke<DictationStatus>('get_dictation_status');
export const toggleDictation = () => invoke<void>('toggle_dictation');
export const cancelDictation = () => invoke<void>('cancel_dictation');
export const hideDictation = () => invoke<void>('hide_dictation');
export const getDictationPermissions = (request = false) =>
  invoke<DictationPermissions>('dictation_permissions', { request });
