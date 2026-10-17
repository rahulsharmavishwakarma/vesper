// Minimum macOS of a Mach-O library, from its LC_BUILD_VERSION load command.
// A library built for a newer macOS than the bundle minimum cannot be loaded
// on older systems.
import { execFileSync } from 'node:child_process';

export const DEPLOYMENT_TARGET = '14.0'; // matches the MLX bundle minimum

export function parseMinimumSystem(loadCommands) {
  return loadCommands.match(/cmd LC_BUILD_VERSION[\s\S]*?minos (\S+)/)?.[1] ?? null;
}

export function loadableOn(minimum, target = DEPLOYMENT_TARGET) {
  const [have, want] = [minimum, target].map((value) => value.split('.').map(Number));
  for (let i = 0; i < Math.max(have.length, want.length); i++)
    if ((have[i] ?? 0) !== (want[i] ?? 0)) return (have[i] ?? 0) < (want[i] ?? 0);
  return true;
}

// Null when the file is not a Mach-O object or otool is unavailable; callers
// still verify content hashes separately.
export function minimumSystem(path, run = execFileSync) {
  try {
    return parseMinimumSystem(run('otool', ['-l', path], { encoding: 'utf8', stdio: 'pipe' }));
  } catch {
    return null;
  }
}
