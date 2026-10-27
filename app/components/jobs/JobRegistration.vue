<script setup lang="ts">
  import type { SavedJob } from '~/utils/jobs';
  import * as v from 'valibot';
  import {
    jobErrorSchema,
    jobSettingsSchema,
    loadExampleDefaults,
    registerJob,
    selectJobPath,
  } from '~/utils/jobs';

  const emit = defineEmits<{ registered: [job: SavedJob] }>();
  const form = reactive<{
    name: string;
    projectPath: string;
    entryPath: string;
    input: string;
    timeoutMs: number;
  }>({
    name: 'Wikipedia · Bamboo',
    projectPath: '',
    entryPath: '',
    input: '{"topic": "Bamboo"}',
    timeoutMs: 60000,
  });
  const busy = shallowRef<boolean>(false);
  const error = shallowRef<string>('');

  async function browse(project: boolean): Promise<void> {
    busy.value = true;
    error.value = '';
    try {
      const path = await selectJobPath(project);
      if (path !== null) {
        if (project) {
          form.projectPath = path;
        } else {
          form.entryPath = path;
        }
      }
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  }

  async function register(): Promise<void> {
    error.value = '';
    busy.value = true;
    try {
      const settings = v.parse(jobSettingsSchema, form);
      const job = await registerJob(
        settings.name,
        form.projectPath,
        form.entryPath,
        settings.input,
        settings.timeoutMs,
      );
      emit('registered', job);
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  }
  onMounted(async () => {
    busy.value = true;
    try {
      const defaults = await loadExampleDefaults();
      form.projectPath = defaults.project_path;
      form.entryPath = defaults.entry_path;
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  });
</script>

<template>
  <form
    class="grid gap-4"
    @submit.prevent="register"
  >
    <h2 class="text-base font-semibold">Register a local job</h2>
    <p class="text-muted-foreground text-sm">
      The Wikipedia example is filled in. Register it, then choose Run once. Jobs have access to
      your files and network.
    </p>
    <label class="grid gap-1.5 text-sm"
      >Name<input
        v-model="form.name"
        class="bg-background rounded-md border px-3 py-2"
        required
    /></label>
    <label class="grid gap-1.5 text-sm"
      >Project directory
      <span class="flex gap-2"
        ><input
          v-model="form.projectPath"
          class="bg-background min-w-0 flex-1 rounded-md border px-3 py-2 font-mono text-xs"
          placeholder="/absolute/path/to/project"
          required
        /><Button
          :disabled="busy"
          type="button"
          variant="outline"
          @click="browse(true)"
          >Choose</Button
        ></span
      >
    </label>
    <label class="grid gap-1.5 text-sm"
      >Entry file
      <span class="flex gap-2"
        ><input
          v-model="form.entryPath"
          class="bg-background min-w-0 flex-1 rounded-md border px-3 py-2 font-mono text-xs"
          placeholder="/absolute/path/to/project/job.ts"
          required
        /><Button
          :disabled="busy"
          type="button"
          variant="outline"
          @click="browse(false)"
          >Choose</Button
        ></span
      >
    </label>
    <label class="grid gap-1.5 text-sm"
      >JSON input<textarea
        v-model="form.input"
        class="bg-background min-h-24 rounded-md border px-3 py-2 font-mono text-xs"
        spellcheck="false"
        required
      />
    </label>
    <label class="grid gap-1.5 text-sm"
      >Timeout (milliseconds)<input
        v-model.number="form.timeoutMs"
        class="bg-background rounded-md border px-3 py-2"
        type="number"
        min="1"
        step="1"
        required
    /></label>
    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>
    <Button
      :disabled="busy"
      type="submit"
      >{{ busy ? 'Working…' : 'Register job' }}</Button
    >
  </form>
</template>
