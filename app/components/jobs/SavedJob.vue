<script setup lang="ts">
  import type { SavedJob } from '~/utils/jobs';
  import * as v from 'valibot';
  import { jobErrorSchema, jobSettingsSchema, saveJobSettings } from '~/utils/jobs';

  const props = defineProps<{ job: SavedJob; submitting: boolean }>();
  const emit = defineEmits<{ saved: [job: SavedJob]; run: [jobId: string, input: string] }>();
  const name = shallowRef<string>(props.job.name);
  const input = shallowRef<string>(JSON.stringify(props.job.input, null, 2));
  const timeoutMs = shallowRef<number>(props.job.timeout_ms);
  const saving = shallowRef<boolean>(false);
  const error = shallowRef<string>('');
  const message = shallowRef<string>('');

  async function save(): Promise<void> {
    saving.value = true;
    error.value = '';
    message.value = '';
    try {
      const settings = v.parse(jobSettingsSchema, {
        name: name.value,
        input: input.value,
        timeoutMs: timeoutMs.value,
      });
      emit(
        'saved',
        await saveJobSettings(props.job.id, settings.name, settings.input, settings.timeoutMs),
      );
      message.value = 'Settings saved.';
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      saving.value = false;
    }
  }
</script>

<template>
  <form
    class="grid gap-3 rounded-lg border p-5"
    @submit.prevent="save"
  >
    <label class="text-muted-foreground grid gap-1.5 text-xs"
      >Job name<input
        v-model="name"
        class="bg-background text-foreground rounded-md border px-3 py-2 text-base font-medium"
        required
    /></label>
    <p class="text-muted-foreground font-mono text-xs break-all">
      {{ job.project_path }}<br />{{ job.entry_path }}
    </p>
    <label class="grid gap-1.5 text-sm"
      >JSON input<textarea
        v-model="input"
        class="bg-background min-h-24 rounded-md border px-3 py-2 font-mono text-xs"
        spellcheck="false"
        required
      />
    </label>
    <label class="grid gap-1.5 text-sm"
      >Timeout (milliseconds)<input
        v-model.number="timeoutMs"
        class="bg-background rounded-md border px-3 py-2"
        type="number"
        min="1"
        step="1"
        required
    /></label>
    <JobsJobProfile
      :job-id="job.id"
      :profile-id="job.profile_id"
      @saved="emit('saved', $event)"
    />
    <p class="text-muted-foreground text-xs">
      Save to keep these settings. Run once uses the edited input and the saved timeout.
    </p>
    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>
    <p
      v-if="message"
      class="text-sm"
      role="status"
    >
      {{ message }}
    </p>
    <div class="flex gap-2">
      <Button
        :disabled="saving"
        type="submit"
        variant="outline"
        >Save settings</Button
      >
      <Button
        :disabled="submitting || saving"
        type="button"
        @click="emit('run', job.id, input)"
        >{{ submitting ? 'Adding…' : 'Run once' }}</Button
      >
    </div>
  </form>
</template>
