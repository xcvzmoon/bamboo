<script setup lang="ts">
  import type { CaptureStatus, ProfileForm, SessionProfile } from '~/utils/profiles';
  import * as v from 'valibot';
  import { jobErrorSchema } from '~/utils/jobs';
  import {
    beginCapture,
    cancelCapture,
    captureStatus,
    finishCapture,
    listProfiles,
  } from '~/utils/profiles';

  const props = defineProps<{ revision: number }>();
  const profiles = shallowRef<SessionProfile[]>([]);
  const capture = shallowRef<CaptureStatus | null>(null);
  const busy = shallowRef<boolean>(true);
  const error = shallowRef<string>('');
  const message = shallowRef<string>('');

  async function load(): Promise<void> {
    try {
      profiles.value = await listProfiles();
      capture.value = await captureStatus();
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  }

  async function start(form: ProfileForm, profileId: string | null = null): Promise<void> {
    busy.value = true;
    error.value = '';
    message.value = '';
    try {
      capture.value = await beginCapture(profileId, form);
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  }

  async function refresh(profile: SessionProfile): Promise<void> {
    await start(
      {
        name: profile.name,
        loginUrl: profile.login_url,
        browser: profile.browser,
      },
      profile.id,
    );
  }

  async function complete(save: boolean): Promise<void> {
    if (!capture.value) {
      return;
    }
    busy.value = true;
    error.value = '';
    try {
      if (save) {
        await finishCapture(capture.value.id);
        message.value = 'Login profile saved.';
      } else {
        await cancelCapture(capture.value.id);
        message.value = 'Login setup cancelled. Your saved profile is unchanged.';
      }
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      capture.value = null;
      await load();
    }
  }

  async function checkCapture(): Promise<void> {
    if (!capture.value || busy.value) {
      return;
    }
    try {
      capture.value = await captureStatus();
    } catch (error_) {
      capture.value = null;
      error.value = v.parse(jobErrorSchema, error_);
    }
  }

  useIntervalFn(checkCapture, 2000);
  watch(
    () => props.revision,
    async () => {
      if (!busy.value) {
        await load();
      }
    },
  );
  onMounted(load);
</script>

<template>
  <section
    class="mt-8 grid gap-5 rounded-xl border p-5"
    aria-label="Login profiles"
  >
    <div>
      <h2 class="text-base font-semibold">Login profiles</h2>
      <p class="text-muted-foreground mt-1 text-sm">
        Sign in through a separate Helium window, then save the session. Profiles are encrypted with
        a key in macOS Keychain.
      </p>
    </div>

    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>

    <p
      v-if="message"
      class="text-muted-foreground text-sm"
      role="status"
    >
      {{ message }}
    </p>

    <div
      v-if="capture"
      class="grid gap-3 rounded-lg border p-4"
    >
      <p
        class="text-sm"
        role="status"
      >
        Sign in to {{ capture.name }} in the temporary browser window. When login is complete, save
        your session here.
      </p>
      <div class="flex gap-2">
        <Button
          :disabled="busy"
          type="button"
          @click="complete(true)"
          >I've signed in · Save profile</Button
        >
        <Button
          :disabled="busy"
          type="button"
          variant="outline"
          @click="complete(false)"
          >Cancel</Button
        >
      </div>
    </div>

    <ProfilesProfileForm
      :disabled="busy || capture !== null"
      @start="start"
    />

    <ProfilesProfileList
      :profiles="profiles"
      :disabled="busy || capture !== null"
      @refresh="refresh"
    />
  </section>
</template>
