import { invoke, isTauri } from '@tauri-apps/api/core';
import type { SessionRecord } from './dataset';

export interface RuntimeStatus {
  operationId: string | null;
  busy: boolean;
  stage: string;
  message: string;
  modelDirectory: string;
  modelsReady: boolean;
  maxRecordingSeconds: number;
  audioLevel: number;
  partial: StreamingTranscript | null;
  partialInvalidatedFor: string | null;
  download: DownloadStatus | null;
}

export interface DownloadStatus {
  operationId: string;
  modelId: string | null;
  quantization: 'q4_0' | 'q8_0' | null;
  progress: InstallProgress | null;
}
export interface InstallProgress {
  phase: 'checking' | 'downloading' | 'verifying' | 'installed';
  fileName: string;
  fileIndex: number;
  fileCount: number;
  completedBytes: number;
  totalBytes: number;
}

export interface StreamingTranscript {
  operationId: string;
  sequence: number;
  committedText: string;
  provisionalText: string;
  audioProcessedSeconds: number;
  emittedAtUnixMs: number;
  isFinal: boolean;
}

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface ModelInstallation {
  ready: boolean;
  modelDirectory: string;
  files: { name: string; installed: boolean; sizeBytes: number }[];
  supported: boolean;
  message: string;
}

export interface ModelCatalogInstallation extends ModelInstallation {
  modelId: string;
  name: string;
  role: 'speech_recognition' | 'text_reconstruction' | 'audio_frontend';
  quantization: 'q4_0' | 'q8_0' | null;
  selected: boolean;
  artifactsInstalled: boolean;
  runtimeReady: boolean;
  localFileCount: number;
  downloadedBytes: number;
  removableBytes: number;
  removalError: string | null;
}

export interface PipelineResult {
  resultId: string;
  rawTranscript: string;
  cleanedTranscript: string | null;
  personalizedTranscript?: string | null;
  personalizationEvidence?: import('./personalization').MemoryEvidence | null;
  draftTranscript?: string | null;
  draftRevision?: number;
  learningOutcome?: import('./personalization').LearningOutcome | null;
  session: SessionRecord | null;
  warnings: string[];
  durationSeconds: number;
}

function desktopInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri())
    return Promise.reject(new Error('Recording and model inference require the desktop app.'));
  return invoke<T>(command, args);
}

export function getRuntimeStatus() {
  return desktopInvoke<RuntimeStatus>('get_runtime_status');
}
export interface AudioFeatures {
  level: number;
  pitchHz: number | null;
}
export function getAudioFeatures() {
  return desktopInvoke<AudioFeatures>('get_audio_features');
}
export function getModelInstallation() {
  return desktopInvoke<ModelInstallation>('get_model_installation');
}
export function listModelInstallations() {
  return desktopInvoke<ModelCatalogInstallation[]>('list_model_installations');
}
export function listAudioDevices() {
  return desktopInvoke<AudioDevice[]>('list_audio_devices');
}
export function installModels() {
  return desktopInvoke<ModelInstallation>('install_models');
}
export function installModel(
  modelId: string,
  quantization: ModelCatalogInstallation['quantization'],
) {
  return desktopInvoke<ModelCatalogInstallation>('install_model', { modelId, quantization });
}
export function removeModel(model: ModelCatalogInstallation) {
  return desktopInvoke<{ removedBytes: number; removedFiles: number }>('remove_model', {
    modelId: model.modelId,
    role: model.role,
    quantization: model.quantization,
    modelDirectory: model.modelDirectory,
  });
}
export function startRecording(deviceId: string | null) {
  return desktopInvoke<string>('start_recording', { deviceId });
}
export function stopRecording() {
  return desktopInvoke<PipelineResult>('stop_recording');
}
export function cancelOperation() {
  return desktopInvoke<void>('cancel_operation');
}
export function getLatestResult() {
  return desktopInvoke<PipelineResult | null>('get_latest_result');
}

export async function chooseAudioFile(): Promise<string | null> {
  if (!isTauri()) throw new Error('Audio transcription requires the desktop app.');
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({
    title: 'Choose audio to transcribe',
    multiple: false,
    directory: false,
    filters: [
      {
        name: 'Audio files',
        extensions: [
          'wav',
          'mp3',
          'm4a',
          'flac',
          'ogg',
          'opus',
          'aac',
          'aif',
          'aiff',
          'webm',
          'mp4',
        ],
      },
    ],
  });
  return typeof path === 'string' ? path : null;
}

export function transcribeFile(path: string) {
  return desktopInvoke<PipelineResult>('transcribe_file', { path });
}
