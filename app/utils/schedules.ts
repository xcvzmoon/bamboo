import { invoke } from '@tauri-apps/api/core';
import * as v from 'valibot';

export const scheduleSchema = v.variant('kind', [
  v.object({ kind: v.literal('disabled') }),
  v.object({
    kind: v.literal('interval'),
    every: v.pipe(
      v.number(),
      v.integer(),
      v.minValue(1, 'Enter a positive interval'),
      v.maxValue(4_294_967_295),
    ),
    unit: v.picklist(['minutes', 'hours']),
  }),
  v.object({
    kind: v.literal('daily'),
    hour: v.pipe(v.number(), v.integer(), v.minValue(0), v.maxValue(23)),
    minute: v.pipe(v.number(), v.integer(), v.minValue(0), v.maxValue(59)),
  }),
]);
export type Schedule = v.InferOutput<typeof scheduleSchema>;
const localTimeSchema = v.pipe(
  v.string(),
  v.regex(/^(?:[01]\d|2[0-3]):[0-5]\d$/, 'Enter a local time in HH:MM format'),
);
const scheduleDetailsSchema = v.object({
  saved: v.object({
    job_id: v.string(),
    revision: v.string(),
    schedule: scheduleSchema,
    anchor_at: v.number(),
    last_occurrence: v.nullable(v.number()),
  }),
  next_at: v.nullable(v.number()),
});
export type ScheduleDetails = v.InferOutput<typeof scheduleDetailsSchema>;

export function parseScheduleForm(
  kind: Schedule['kind'],
  every: number,
  unit: 'minutes' | 'hours',
  time: string,
): Schedule {
  switch (kind) {
    case 'disabled':
      return v.parse(scheduleSchema, { kind });
    case 'interval':
      return v.parse(scheduleSchema, { kind, every, unit });
    case 'daily':
      return parseDailyTime(time);
    default:
      throw new Error('SCHEDULE_KIND: Unsupported schedule kind');
  }
}

function parseDailyTime(time: string): Schedule {
  const parsed = v.parse(localTimeSchema, time);
  return v.parse(scheduleSchema, {
    kind: 'daily',
    hour: Number(parsed.slice(0, 2)),
    minute: Number(parsed.slice(3)),
  });
}

export async function loadSchedule(jobId: string): Promise<ScheduleDetails | null> {
  return v.parse(v.nullable(scheduleDetailsSchema), await invoke('get_schedule', { jobId }));
}

export async function saveSchedule(jobId: string, schedule: Schedule): Promise<ScheduleDetails> {
  return v.parse(
    scheduleDetailsSchema,
    await invoke('save_schedule', {
      jobId,
      schedule: v.parse(scheduleSchema, schedule),
    }),
  );
}
