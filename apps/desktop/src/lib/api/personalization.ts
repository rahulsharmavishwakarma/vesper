import { invoke } from '@tauri-apps/api/core';
import type { Language } from '../config';
import type { SessionRecord } from './dataset';

export type OutputIntent = 'verbatim' | 'note' | 'message' | 'technical';
export interface ProjectRule {
  id: string;
  applicationId: string;
  applicationName: string;
  project: string;
  folder: string | null;
  enabled: boolean;
}
export interface ProjectSelection {
  project: string;
  source:
    | 'manual'
    | 'application'
    | 'folder'
    | 'shared'
    | 'context_unavailable'
    | 'ambiguous'
    | 'workspace'
    | 'category';
  ruleId: string | null;
}
export async function chooseProjectApplication(): Promise<{ id: string; name: string } | null> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({
    title: 'Choose an application',
    multiple: false,
    directory: false,
    filters: [{ name: 'Applications', extensions: ['app'] }],
  });
  if (typeof path !== 'string') return null;
  const application = await invoke<{ id: string; name: string } | null>(
    'read_project_application',
    { path },
  );
  if (!application)
    throw new Error('This application could not be identified. Choose a Mac application.');
  return application;
}
export async function chooseProjectFolder(): Promise<string | null> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({ title: 'Choose a project folder', multiple: false, directory: true });
  return typeof path === 'string' ? invoke<string>('resolve_project_folder', { path }) : null;
}
export interface VocabularyEntry {
  id: string;
  phrase: string;
  language: Language;
  context: string;
  enabled: boolean;
  confirmed: boolean;
  aliases: string[];
  project: string;
  pronunciation: boolean;
  learnedAt: string;
  sources: { sessionId: string; reviewRevisionId: string }[];
  automaticSource: string | null;
  usageCount: number;
}
export interface PersonalizationProfile {
  schemaVersion: number;
  revision: number;
  speechBiasEnabled: boolean;
  outputIntent: OutputIntent;
  preserveFillers: boolean;
  applyCorrections: boolean;
  useRememberedIntent: boolean;
  activeProject: string;
  projectMode: 'manual' | 'automatic';
  projectRules: ProjectRule[];
  automaticWorkspaces: boolean;
  automaticLearning: boolean;
  learningEpoch: number;
  vocabulary: VocabularyEntry[];
  examples: {
    id: string;
    sessionId: string;
    reviewRevisionId: string;
    input: string;
    output: string;
    language: Language;
    preset: 'minimal' | 'balanced';
    intent: OutputIntent;
    enabled: boolean;
    project: string;
  }[];
}
export interface MemoryEvidence {
  profileRevision: number;
  applied: { entryId: string; occurrences: number }[];
  ambiguousMatches: number;
  correctionsEnabled: boolean;
  budgetExceeded: boolean;
  suggestedIntent: { intent: OutputIntent; supportingExamples: number } | null;
}
export interface CorrectionSuggestion {
  reviewRevisionId: string;
  pair: { heard: string; intended: string };
}
export interface LearningDashboard {
  profileRevision: number;
  inspectedSessions: number;
  reviewedSessions: number;
  appliedSessions: number;
  appliedOccurrences: number;
  undoneSessions: number;
  replayExcluded: number;
  replay: {
    modelId: string;
    evidenceKey: string;
    language: Language;
    samples: number;
    referenceWords: number;
    baselineEdits: number;
    memoryEdits: number;
    improved: number;
    worsened: number;
  }[];
  reviewQueue: { sessionId: string; reason: string }[];
}
export const getPersonalization = async () => {
  const profile = await invoke<PersonalizationProfile>('get_personalization');
  return {
    ...profile,
    projectMode: profile.projectMode ?? 'manual',
    projectRules: profile.projectRules ?? [],
    automaticWorkspaces: profile.automaticWorkspaces ?? true,
  };
};
export interface LearningOutcome {
  activated: string[];
  provisional: number;
  suspended: number;
  status: string;
  profileRevision: number;
  elapsedMicros: number;
}
export interface AutomaticLearningState {
  schemaVersion: number;
  policyVersion: number;
  expiredCandidates: number;
  candidates: {
    id: string;
    phrase: string;
    alias: string;
    language: Language;
    project: string;
    context: string;
    state: 'observed' | 'provisional' | 'active' | 'suspended';
    support: { sourceKey: string; episodeId: string; eventId: string; teachingTerms: string[] }[];
    observedCount: number;
    lastSeenMs: number;
    reason: string;
  }[];
  events: {
    kind: 'user_edit' | 'raw_observation' | 'memory_undo';
    retry: 'possible_repair' | 'continuation' | 'changed_intent' | 'uncertain' | null;
  }[];
}
export const getAutomaticLearning = () => invoke<AutomaticLearningState>('get_automatic_learning');
export const forgetAutomaticLearning = (revision: number) =>
  invoke<PersonalizationProfile>('forget_automatic_learning', { revision });
export const suspendAutomaticCandidate = (candidateId: string) =>
  invoke<number>('suspend_automatic_candidate', { candidateId });
export const approveAutomaticCandidate = (candidateId: string) =>
  invoke<PersonalizationProfile>('approve_automatic_candidate', { candidateId });
export const editLatestDraft = (resultId: string, revision: number, text: string) =>
  invoke<import('./runtime').PipelineResult>('edit_latest_draft', { resultId, revision, text });
export const undoLatestLearning = (resultId: string) =>
  invoke<import('./runtime').PipelineResult>('undo_latest_learning', { resultId });
export const savePersonalization = (profile: PersonalizationProfile) =>
  invoke<PersonalizationProfile>('save_personalization', { profile });
export const rememberCleanup = (id: string) =>
  invoke<PersonalizationProfile>('remember_cleanup', { id });
export const getCorrectionSuggestions = (id: string) =>
  invoke<CorrectionSuggestion[]>('get_correction_suggestions', { id });
export const rememberCorrection = (request: {
  sessionId: string;
  reviewRevisionId: string;
  pair: CorrectionSuggestion['pair'];
  context: string;
  project: string;
  pronunciation: boolean;
  profileRevision: number;
}) => invoke<PersonalizationProfile>('remember_correction', { request });
export const getLearningDashboard = () => invoke<LearningDashboard>('get_learning_dashboard');
export const undoPersonalization = (id: string) =>
  invoke<SessionRecord>('undo_personalization', { id });
export const undoUnsavedPersonalization = (resultId: string) =>
  invoke<void>('undo_unsaved_personalization', { resultId });
export const intentLabel = (intent: OutputIntent) =>
  ({
    verbatim: 'Dictated wording',
    note: 'Note',
    message: 'Message',
    technical: 'Technical dictation',
  })[intent];
