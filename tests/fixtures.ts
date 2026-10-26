import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test as baseTest } from 'vitest';

export const test = baseTest.extend(
  'temporaryDirectory',
  async ({ task: _task }, { onCleanup }) => {
    const directory = await mkdtemp(join(tmpdir(), 'bamboo test '));
    onCleanup(() =>
      rm(directory, {
        recursive: true,
        force: true,
      }),
    );
    return directory;
  },
);
