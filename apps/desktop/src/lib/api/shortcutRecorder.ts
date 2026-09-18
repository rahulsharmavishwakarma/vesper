import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Serialize edits across both fields; late cleanup only ends its own session.
let queue: Promise<unknown> = Promise.resolve();
function nativeEdit(command: string, session: string) {
  const request = queue.then(() => invoke<void>(command, { session }));
  queue = request.catch(() => {});
  return request;
}

export function shortcutRecording(onShortcut: (shortcut: string) => void) {
  const session = crypto.randomUUID();
  let stopped = false;
  let unlisten: (() => void) | undefined;
  const ready = (async () => {
    if (!isTauri()) return;
    unlisten = await listen<{ session: string; shortcut: string }>(
      'shortcut-recorded',
      ({ payload }) => {
        if (!stopped && payload.session === session) onShortcut(payload.shortcut);
      },
    );
    if (stopped) {
      unlisten();
      return;
    }
    try {
      await nativeEdit('begin_shortcut_recording', session);
    } catch (error) {
      unlisten();
      throw error;
    }
  })();
  return {
    ready,
    stop() {
      if (stopped) return;
      stopped = true;
      unlisten?.();
      // End after pending setup; errors are surfaced by ready or cleared by
      // native focus-loss handling. Never leak an unhandled rejection on unmount.
      void ready
        .then(() => {
          if (isTauri()) return nativeEdit('end_shortcut_recording', session);
        })
        .catch(() => {});
    },
  };
}
