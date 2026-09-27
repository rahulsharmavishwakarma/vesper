import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AppConfig } from '../config';
import type { MemoryEvidence } from './personalization';

export type TranscriptFeedback = 'like' | 'dislike' | null;

export interface SpeakerTimeline {
  durationMs: number;
  segments: { startMs: number; endMs: number; speaker: number }[];
  clippedSegments: number;
  modelRevision: string;
  runtimeRevision: string;
  provider: string;
  fallbackReason?: string | null;
}

export interface SessionRecord {
  id: string;
  createdAt: string;
  originalFilename: string;
  audioRelativePath: string;
  audioSha256: string;
  audioBytes: number;
  rawTranscript: string | null;
  speakerTimeline?: SpeakerTimeline | null;
  cleanedTranscript: string | null;
  personalizedTranscript?: string | null;
  draftTranscript?: string | null;
  draftRevision?: number;
  personalizationEvidence?: MemoryEvidence | null;
  personalizationProject?: string;
  projectSelection?: import('./personalization').ProjectSelection | null;
  personalizationRejected?: boolean;
  correctedTranscript: string | null;
  verifiedTranscript: string | null;
  reviewHistory: unknown[];
  reviewStatus: 'pending' | 'approved';
  transcriptFeedback?: TranscriptFeedback;
  configSnapshot: AppConfig;
  modelVersions: Record<string, unknown>;
  pipelineEvents?: { stage: string; message: string; timestamp: string }[];
}

export interface ExportSummary {
  destination: string;
  sessionCount: number;
  approvedOnly: boolean;
  manifestPath: string;
  metadataPath: string;
}

function requireDesktop() {
  if (!isTauri()) throw new Error('Dataset files are available in the desktop app.');
}

export async function listSessions(): Promise<SessionRecord[]> {
  requireDesktop();
  return invoke<SessionRecord[]>('list_sessions');
}

export async function setTranscriptFeedback(
  id: string,
  feedback: TranscriptFeedback,
): Promise<SessionRecord> {
  requireDesktop();
  return invoke<SessionRecord>('set_transcript_feedback', { id, feedback });
}

export async function chooseAndImportAudio(): Promise<SessionRecord | null> {
  requireDesktop();
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({
    title: 'Import an audio example',
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
  if (typeof path !== 'string') return null;
  return invoke<SessionRecord>('import_audio', { path });
}

export async function approveCorrection(id: string, correctedText: string): Promise<SessionRecord> {
  requireDesktop();
  if (!correctedText.trim()) throw new Error('Enter a reviewed label before saving.');
  if ([...correctedText].length > 100000) throw new Error('Keep labels within 100,000 characters.');
  return invoke<SessionRecord>('approve_correction', { id, correctedText });
}

export async function approveTranscription(
  id: string,
  verbatimText: string,
): Promise<SessionRecord> {
  requireDesktop();
  if (!verbatimText.trim()) throw new Error('Enter a reviewed transcription before saving.');
  if ([...verbatimText].length > 100000) throw new Error('Keep labels within 100,000 characters.');
  return invoke<SessionRecord>('approve_transcription', { id, verbatimText });
}

export async function chooseAndExportDataset(approvedOnly: boolean): Promise<ExportSummary | null> {
  requireDesktop();
  const { open } = await import('@tauri-apps/plugin-dialog');
  const destination = await open({
    title: 'Choose a folder for the dataset export',
    multiple: false,
    directory: true,
  });
  if (typeof destination !== 'string') return null;
  return invoke<ExportSummary>('export_dataset', { destination, approvedOnly });
}
