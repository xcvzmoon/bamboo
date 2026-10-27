import { invoke } from '@tauri-apps/api/core';
import * as v from 'valibot';

export const profileSchema = v.object({
  id: v.string(),
  name: v.string(),
  login_url: v.string(),
  browser: v.string(),
  captured_at: v.number(),
  needs_refresh: v.boolean(),
});
export type SessionProfile = v.InferOutput<typeof profileSchema>;
const captureSchema = v.object({
  id: v.string(),
  name: v.string(),
  refreshing: v.boolean(),
});
export type CaptureStatus = v.InferOutput<typeof captureSchema>;
export const profileFormSchema = v.object({
  name: v.pipe(v.string(), v.trim(), v.nonEmpty('Enter a profile name'), v.maxLength(120)),
  loginUrl: v.pipe(
    v.string(),
    v.url('Enter a login URL'),
    v.regex(/^https?:\/\//, 'Use an HTTP or HTTPS URL'),
  ),
  browser: v.pipe(v.string(), v.startsWith('/', 'Enter an absolute browser executable path')),
});
export type ProfileForm = v.InferOutput<typeof profileFormSchema>;

export async function listProfiles(): Promise<SessionProfile[]> {
  return v.parse(v.array(profileSchema), await invoke('list_profiles'));
}
export async function captureStatus(): Promise<CaptureStatus | null> {
  return v.parse(v.nullable(captureSchema), await invoke('profile_capture_status'));
}
export async function beginCapture(
  profileId: string | null,
  form: ProfileForm,
): Promise<CaptureStatus> {
  return v.parse(
    captureSchema,
    await invoke('begin_profile_capture', {
      profileId,
      ...v.parse(profileFormSchema, form),
    }),
  );
}
export async function finishCapture(captureId: string): Promise<SessionProfile> {
  return v.parse(profileSchema, await invoke('finish_profile_capture', { captureId }));
}
export async function cancelCapture(captureId: string): Promise<void> {
  await invoke('cancel_profile_capture', { captureId });
}
