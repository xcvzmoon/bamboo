import { invoke } from '@tauri-apps/api/core';
import * as v from 'valibot';

export type HistoryTarget = { kind: 'run'; id: string } | { kind: 'job'; id: string };
const deleteRunResultSchema = v.object({
  deleted: v.array(v.string()),
  failed: v.array(
    v.object({
      run_id: v.string(),
      code: v.string(),
      message: v.string(),
    }),
  ),
});
export type DeleteRunResult = v.InferOutput<typeof deleteRunResultSchema>;

export async function deleteHistory(target: HistoryTarget): Promise<DeleteRunResult> {
  const result: unknown =
    target.kind === 'run'
      ? await invoke('delete_run', { runId: target.id })
      : await invoke('delete_job_history', { jobId: target.id });
  return v.parse(deleteRunResultSchema, result);
}
