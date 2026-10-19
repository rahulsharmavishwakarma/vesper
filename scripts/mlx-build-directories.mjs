import { lstatSync, mkdirSync, mkdtempSync, realpathSync, renameSync, statSync } from 'node:fs';
import { basename, join } from 'node:path';

// Only generated MLX directories are eligible for recovery. Never follow a
// dangling link to recreate a deleted temporary checkout or replace its data.
export function prepareMlxBuildDirectories(root, warn = console.warn) {
  mkdirSync(join(root, '.tooling'), { recursive: true });
  // Swift Build records absolute output paths. A worktree whose `.tooling` is a
  // link to the shared folder must use the same canonical path, or the next
  // build from the other path reports every output as stale.
  const tooling = realpathSync(join(root, '.tooling'));
  const paths = {
    scratch: join(tooling, 'mlx-build'),
    cache: join(tooling, 'swift-cache'),
    destination: join(tooling, 'mlx-runtime'),
  };
  const missing = Object.values(paths).filter((path) => {
    try {
      if (!statSync(path).isDirectory()) throw new Error(`Expected a directory: ${path}`);
      return false;
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      return true;
    }
  });
  let recovery;
  for (const path of missing) {
    let entry;
    try {
      entry = lstatSync(path);
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
    if (entry?.isSymbolicLink()) {
      recovery ??= mkdtempSync(join(tooling, 'mlx-directory-recovery-'));
      renameSync(path, join(recovery, basename(path)));
    }
    mkdirSync(path, { recursive: true });
  }
  if (recovery) warn(`Recovered MLX build directories; original links preserved in ${recovery}`);
  return paths;
}
