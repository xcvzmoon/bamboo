<script setup lang="ts">
  import type { SavedJob } from '~/utils/jobs';
  import * as v from 'valibot';
  import { jobErrorSchema, jobSettingsSchema, listJobs, startJob } from '~/utils/jobs';

  const jobs = shallowRef<SavedJob[]>([]);
  const loading = shallowRef<boolean>(true);
  const running = shallowRef<boolean>(false);
  const error = shallowRef<string>('');
  const selectedRun = shallowRef<string | null>(null);
  const historyRevision = shallowRef<number>(0);

  function queueChanged(): void {
    historyRevision.value += 1;
  }

  function deleted(ids: string[]): void {
    if (selectedRun.value && ids.includes(selectedRun.value)) {
      selectedRun.value = null;
    }
    historyRevision.value += 1;
  }

  function registered(job: SavedJob): void {
    jobs.value = [...jobs.value, job];
  }

  function saved(job: SavedJob): void {
    jobs.value = jobs.value.map((entry) => (entry.id === job.id ? job : entry));
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    error.value = '';
    try {
      jobs.value = await listJobs();
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      loading.value = false;
    }
  }

  async function run(jobId: string, input: string): Promise<void> {
    running.value = true;
    error.value = '';

    try {
      const parsed = v.parse(jobSettingsSchema.entries.input, input);
      const outcome = await startJob(jobId, parsed);
      selectedRun.value = outcome.id;
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      running.value = false;
      historyRevision.value += 1;
    }
  }

  onMounted(refresh);
</script>

<template>
  <main class="mx-auto max-w-6xl p-6 lg:p-10">
    <header class="mb-6">
      <p class="text-muted-foreground mb-2 font-mono text-xs">LOCAL AUTOMATION</p>
      <h1 class="text-2xl font-semibold tracking-tight">Browser jobs</h1>
    </header>
    <JobsRuntimeSettings class="mb-6" />
    <ProfilesPanel
      :revision="historyRevision"
      class="mb-6"
    />
    <div class="grid items-start gap-8 lg:grid-cols-[minmax(0,1fr)_22rem]">
      <section
        class="grid gap-4"
        aria-label="Saved jobs"
      >
        <div class="flex items-center justify-between">
          <h2 class="text-sm font-medium">Saved jobs · {{ jobs.length }}</h2>
          <Button
            :disabled="loading || running"
            type="button"
            variant="ghost"
            size="sm"
            @click="refresh"
            >Refresh</Button
          >
        </div>
        <p
          v-if="loading"
          class="text-muted-foreground text-sm"
          role="status"
        >
          Loading jobs…
        </p>
        <p
          v-else-if="jobs.length === 0"
          class="text-muted-foreground rounded-lg border border-dashed p-8 text-sm"
        >
          Register your first TypeScript job to get started.
        </p>
        <p
          v-if="error"
          class="text-destructive text-sm"
          role="alert"
        >
          {{ error }}
        </p>
        <JobsRunQueue
          @changed="queueChanged"
          @select="selectedRun = $event"
        />
        <RunDetails
          v-if="selectedRun"
          :key="`${selectedRun}-${historyRevision}`"
          :run-id="selectedRun"
          @deleted="deleted"
        />
        <div
          v-for="job in jobs"
          :key="job.id"
          class="grid gap-3"
        >
          <JobsSavedJob
            :job="job"
            :submitting="running"
            @saved="saved"
            @run="run"
          />
          <JobsScheduleEditor :job-id="job.id" />
          <JobsRunHistory
            :job-id="job.id"
            :revision="historyRevision"
            @select="selectedRun = $event"
            @deleted="deleted"
          />
        </div>
      </section>
      <aside class="bg-muted/20 rounded-lg border p-5">
        <JobsJobRegistration @registered="registered" />
      </aside>
    </div>
  </main>
</template>
