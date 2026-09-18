import type { RuntimeStatus, StreamingTranscript } from './api/runtime';

/** An idle status or a new recording cannot inherit an earlier live draft. */
export function liveTranscriptFromStatus(
  current: StreamingTranscript | null,
  status: Pick<RuntimeStatus, 'busy' | 'operationId' | 'partial' | 'partialInvalidatedFor'>,
): StreamingTranscript | null {
  if (!status.busy || !status.operationId) return null;
  if (current?.operationId !== status.operationId) current = null;
  return mergeLiveTranscript(
    current,
    status.partial,
    status.operationId,
    status.partialInvalidatedFor,
  );
}

/** The fallback marker is operation-scoped and also rejects delayed events. */
export function mergeLiveTranscript(
  current: StreamingTranscript | null,
  incoming: StreamingTranscript | null,
  activeOperationId: string | null,
  invalidatedFor: string | null,
): StreamingTranscript | null {
  if (current?.operationId === invalidatedFor) current = null;
  if (!incoming || incoming.operationId === invalidatedFor) return current;
  if (incoming.operationId !== activeOperationId) return current;
  if (current?.operationId === incoming.operationId && incoming.sequence <= current.sequence)
    return current;
  return incoming;
}
