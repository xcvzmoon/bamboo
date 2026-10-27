<script setup lang="ts">
  import type { SavedJob } from '~/utils/jobs';
  import type { SessionProfile } from '~/utils/profiles';
  import * as v from 'valibot';
  import { jobErrorSchema, setJobProfile } from '~/utils/jobs';
  import { listProfiles } from '~/utils/profiles';

  const props = defineProps<{ jobId: string; profileId: string | null }>();
  const emit = defineEmits<{ saved: [job: SavedJob] }>();
  const profiles = shallowRef<SessionProfile[]>([]);
  const selected = shallowRef<string>(props.profileId ?? '');
  const busy = shallowRef<boolean>(false);
  const error = shallowRef<string>('');

  async function load(): Promise<void> {
    busy.value = true;
    try {
      profiles.value = await listProfiles();
      error.value = '';
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  }

  async function save(): Promise<void> {
    busy.value = true;
    try {
      emit('saved', await setJobProfile(props.jobId, selected.value || null));
      error.value = '';
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  }

  onMounted(load);
</script>

<template>
  <div class="grid gap-2">
    <label class="grid gap-1.5 text-sm">
      Login profile
      <select
        v-model="selected"
        :disabled="busy"
        class="bg-background rounded-md border px-3 py-2"
      >
        <option value="">None · fresh session</option>
        <option
          v-for="profile in profiles"
          :key="profile.id"
          :value="profile.id"
        >
          {{ profile.name }}{{ profile.needs_refresh ? ' · needs refresh' : '' }}
        </option>
      </select>
    </label>

    <div class="flex gap-2">
      <Button
        :disabled="busy"
        type="button"
        size="sm"
        variant="outline"
        @click="save"
        >Save login profile</Button
      >
      <Button
        :disabled="busy"
        type="button"
        size="sm"
        variant="ghost"
        @click="load"
        >Reload profiles</Button
      >
    </div>

    <p class="text-muted-foreground text-xs">
      Runs use the saved profile selection. Refresh expired logins in Login profiles.
    </p>

    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>
  </div>
</template>
