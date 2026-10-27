import { invoke } from '@tauri-apps/api/core';
import * as v from 'valibot';

export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

const jsonValueSchema: v.GenericSchema<JsonValue> = v.lazy(() =>
  v.union([
    v.string(),
    v.pipe(v.number(), v.finite()),
    v.boolean(),
    v.null(),
    v.array(jsonValueSchema),
    v.record(v.string(), jsonValueSchema),
  ]),
);
export const jobSchema = v.object({
  id: v.string(),
  name: v.string(),
  profile_id: v.nullable(v.string()),
  project_path: v.string(),
  entry_path: v.string(),
  input: jsonValueSchema,
  timeout_ms: v.pipe(v.number(), v.integer(), v.minValue(1)),
  created_at: v.number(),
});
export type SavedJob = v.InferOutput<typeof jobSchema>;
export const jobSettingsSchema = v.object({
  name: v.pipe(v.string(), v.trim(), v.nonEmpty('Enter a job name')),
  input: v.pipe(v.string(), v.parseJson(), jsonValueSchema),
  timeoutMs: v.pipe(v.number(), v.integer(), v.minValue(1), v.maxValue(Number.MAX_SAFE_INTEGER)),
});
const commandErrorSchema = v.object({
  code: v.string(),
  message: v.string(),
});
const runtimePathsSchema = v.object({
  node: v.string(),
  lightpanda: v.string(),
});
const runSummarySchema = v.object({
  job: jobSchema,
  queued_at: v.number(),
  started_at: v.nullable(v.number()),
  finished_at: v.nullable(v.number()),
  id: v.string(),
  status: v.picklist(['queued', 'running', 'succeeded', 'failed', 'cancelled', 'interrupted']),
  record_count: v.nullable(v.number()),
  failure: v.nullable(commandErrorSchema),
});
export type RunSummary = v.InferOutput<typeof runSummarySchema>;
export type RuntimePaths = v.InferOutput<typeof runtimePathsSchema>;

export const jobErrorSchema = v.pipe(
  v.fallback(
    v.union([
      v.pipe(
        commandErrorSchema,
        v.transform(({ code, message }) => `${code}: ${message}`),
      ),
      v.pipe(
        v.instance(Error),
        v.transform((error) => error.message),
      ),
      v.string(),
    ]),
    'The operation failed. Please try again.',
  ),
  v.string(),
);

export async function listJobs(): Promise<SavedJob[]> {
  return v.parse(v.array(jobSchema), await invoke('list_jobs'));
}

export async function registerJob(
  name: string,
  projectPath: string,
  entryPath: string,
  input: JsonValue,
  timeoutMs: number,
): Promise<SavedJob> {
  return v.parse(
    jobSchema,
    await invoke('register_job', { name, projectPath, entryPath, input, timeoutMs }),
  );
}

export async function saveJobSettings(
  jobId: string,
  name: string,
  input: JsonValue,
  timeoutMs: number,
): Promise<SavedJob> {
  return v.parse(jobSchema, await invoke('save_job_settings', { jobId, name, input, timeoutMs }));
}

export async function startJob(jobId: string, input: JsonValue): Promise<RunSummary> {
  return v.parse(runSummarySchema, await invoke('run_job', { jobId, input }));
}

export async function selectJobPath(project: boolean): Promise<string | null> {
  return v.parse(v.nullable(v.string()), await invoke('select_job_path', { project }));
}

export async function loadRuntimePaths(): Promise<RuntimePaths> {
  return v.parse(runtimePathsSchema, await invoke('runtime_paths'));
}

export async function configureRuntimePaths(
  node: string,
  lightpanda: string,
): Promise<RuntimePaths> {
  return v.parse(runtimePathsSchema, await invoke('configure_runtimes', { node, lightpanda }));
}

const exampleDefaultsSchema = v.object({
  project_path: v.string(),
  entry_path: v.string(),
  node: v.nullable(v.string()),
  lightpanda: v.nullable(v.string()),
});

export async function loadExampleDefaults(): Promise<v.InferOutput<typeof exampleDefaultsSchema>> {
  return v.parse(exampleDefaultsSchema, await invoke('example_defaults'));
}

const runDetailsSchema = v.object({
  run: runSummarySchema,
  records: v.nullable(v.array(v.record(v.string(), jsonValueSchema))),
  stdout: v.string(),
  stderr: v.string(),
  stdout_truncated: v.boolean(),
  stderr_truncated: v.boolean(),
});
export type SavedRunDetails = v.InferOutput<typeof runDetailsSchema>;

export async function listRuns(jobId: string, offset: number): Promise<RunSummary[]> {
  return v.parse(v.array(runSummarySchema), await invoke('list_runs', { jobId, offset }));
}

export async function loadRunDetails(runId: string): Promise<SavedRunDetails> {
  return v.parse(runDetailsSchema, await invoke('run_details', { runId }));
}

export async function exportRun(runId: string, format: 'json' | 'csv'): Promise<string | null> {
  return v.parse(v.nullable(v.string()), await invoke('export_run', { runId, format }));
}

const queueSnapshotSchema = v.object({
  active: v.nullable(runSummarySchema),
  pending: v.array(runSummarySchema),
  closed: v.boolean(),
});
export type QueueSnapshot = v.InferOutput<typeof queueSnapshotSchema>;

export async function loadQueueSnapshot(): Promise<QueueSnapshot> {
  return v.parse(queueSnapshotSchema, await invoke('queue_snapshot'));
}

export async function cancelRun(runId: string): Promise<boolean> {
  return v.parse(v.boolean(), await invoke('cancel_run', { runId }));
}

export async function setJobProfile(jobId: string, profileId: string | null): Promise<SavedJob> {
  return v.parse(
    jobSchema,
    await invoke('set_job_profile', {
      jobId,
      profileId,
    }),
  );
}
