import { invoke } from '@tauri-apps/api/core';

export type TermSource =
  | 'vocabulary'
  | 'correction'
  | 'text_replacement'
  | 'spelling_dictionary'
  | 'project_dictionary'
  | 'identifier'
  | 'manifest'
  | 'documentation'
  | 'contact';
export type TermTier = 'asserted' | 'authored';
export type TermStatus = 'unconfirmed' | 'active' | 'suspended' | 'removed';

export interface InventoryRowView {
  id: number;
  surface: string;
  kind: 'name' | 'identifier' | 'acronym' | 'phrase';
  tier: TermTier;
  status: TermStatus;
  sources: TermSource[];
  workspaces: string[];
  count: number;
}
export interface TermMiningRun {
  startedMs: number;
  wallMs: number;
  cpuMs: number;
  outcome: string;
  filesRead: number;
  termsTotal: number;
}
export interface TermInventorySummary {
  total: number;
  usable: number;
  removed: number;
  suspended: number;
  sources: { source: TermSource; tier: TermTier; terms: number; paused: boolean }[];
  workspaces: { scope: string; label: string; terms: number }[];
  rows: InventoryRowView[];
  lastRun: TermMiningRun | null;
}

export const SOURCE_LABELS: Record<TermSource, string> = {
  vocabulary: 'Your vocabulary',
  correction: 'Your corrections',
  text_replacement: 'Text Replacements',
  spelling_dictionary: 'Mac spelling dictionary',
  project_dictionary: 'Project spelling lists',
  identifier: 'Code',
  manifest: 'Package names',
  documentation: 'Documentation',
  contact: 'Contacts',
};

export const getTermInventory = () => invoke<TermInventorySummary>('get_term_inventory');
export const removeInventoryTerm = (id: number) =>
  invoke<TermInventorySummary>('remove_inventory_term', { id });
export const forgetTermSource = (source: TermSource) =>
  invoke<TermInventorySummary>('forget_term_source', { source });
export const resumeTermSource = (source: TermSource) =>
  invoke<TermInventorySummary>('resume_term_source', { source });
