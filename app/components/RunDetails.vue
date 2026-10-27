<script setup lang="ts">
  import type { SavedRunDetails } from '~/utils/jobs';
  import * as v from 'valibot';
  import { cancelRun, exportRun, jobErrorSchema, loadRunDetails } from '~/utils/jobs';

  const props = defineProps<{ runId: string }>();
  const emit = defineEmits<{ deleted: [ids: string[]] }>();
  const details = shallowRef<SavedRunDetails | null>(null);
  const loading = shallowRef<boolean>(false);
  const cancelling = shallowRef<boolean>(false);
  const exporting = shallowRef<boolean>(false);
  const error = shallowRef<string>('');
  const destination = shallowRef<string>('');
  const preview = computed<string>(() => JSON.stringify(details.value?.records ?? [], null, 2));
  let request = 0;

  async function refresh(): Promise<void> {
    const current = ++request;
    loading.value = true;
    error.value = '';
    destination.value = '';
    details.value = null;
    try {
      const loaded = await loadRunDetails(props.runId);
      if (current === request) {
        details.value = loaded;
      }
    } catch (error_) {
      if (current === request) {
        error.value = v.parse(jobErrorSchema, error_);
      }
    } finally {
      if (current === request) {
        loading.value = false;
      }
    }
  }

  async function cancel(): Promise<void> {
    cancelling.value = true;
    error.value = '';
    try {
      const accepted = await cancelRun(props.runId);
      if (!accepted) {
        await refresh();
      }
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
      cancelling.value = false;
    }
  }

  useIntervalFn(async () => {
    if (details.value?.run.status === 'running' || details.value?.run.status === 'queued') {
      await refresh();
    }
  }, 1000);

  async function exportDataset(format: 'json' | 'csv'): Promise<void> {
    const id = props.runId;
    exporting.value = true;
    error.value = '';
    destination.value = '';
    try {
      const path = await exportRun(id, format);
      if (id === props.runId && path !== null) {
        destination.value = path;
      }
    } catch (error_) {
      if (id === props.runId) {
        error.value = v.parse(jobErrorSchema, error_);
      }
    } finally {
      exporting.value = false;
    }
  }

  watch(() => props.runId, refresh, { immediate: true });
</script>

<template>
  <section
    class="min-w-0 rounded-lg border p-5"
    aria-label="Run details"
  >
    <div class="mb-4 flex items-center justify-between gap-3">
      <h2 class="text-base font-semibold">Run details</h2>
      <Button
        :disabled="loading || exporting"
        type="button"
        variant="ghost"
        size="sm"
        @click="refresh"
        >Refresh</Button
      >
    </div>
    <p
      v-if="loading"
      class="text-sm"
      role="status"
    >
      Loading run…
    </p>
    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>
    <JobsDeleteHistory
      :target="{ kind: 'run', id: runId }"
      :disabled="
        loading ||
        exporting ||
        details?.run.status === 'queued' ||
        details?.run.status === 'running'
      "
      class="my-3"
      @deleted="emit('deleted', $event)"
    />
    <template v-if="details">
      <Button
        v-if="details.run.status === 'queued' || details.run.status === 'running'"
        :disabled="cancelling"
        type="button"
        variant="outline"
        class="mb-3"
        @click="cancel"
        >{{ cancelling ? 'Cancelling…' : 'Cancel run' }}</Button
      >
      <p class="text-sm font-medium">{{ details.run.job.name }} · {{ details.run.status }}</p>
      <p class="text-muted-foreground mt-1 font-mono text-xs break-all">{{ details.run.id }}</p>
      <p class="text-muted-foreground mt-2 text-xs">
        Queued {{ new Date(details.run.queued_at).toLocaleString()
        }}<span v-if="details.run.finished_at !== null">
          · Finished {{ new Date(details.run.finished_at).toLocaleString() }}</span
        >
      </p>
      <p
        v-if="details.run.failure"
        class="text-destructive mt-3 text-sm"
      >
        {{ details.run.failure.code }}: {{ details.run.failure.message }}
      </p>
      <div
        v-if="details.records !== null"
        class="mt-4 grid gap-3"
      >
        <p class="text-sm">{{ details.run.record_count }} records · Preview shows up to 20</p>
        <p
          v-if="details.run.record_count === 0"
          class="text-muted-foreground text-sm"
        >
          This run returned no records. JSON exports as []; CSV exports as an empty file.
        </p>
        <pre class="max-h-96 overflow-auto font-mono text-xs break-words whitespace-pre-wrap">{{
          preview
        }}</pre>
        <div class="flex gap-2">
          <Button
            :disabled="exporting"
            type="button"
            variant="outline"
            @click="exportDataset('json')"
            >Export JSON</Button
          >
          <Button
            :disabled="exporting"
            type="button"
            variant="outline"
            @click="exportDataset('csv')"
            >Export CSV</Button
          >
        </div>
        <p class="text-muted-foreground text-xs">
          Choose a new filename. Existing files are preserved. CSV supports flat records only.
        </p>
      </div>
      <p
        v-if="destination"
        class="mt-3 text-sm break-all"
        role="status"
      >
        Exported to {{ destination }}
      </p>
      <details class="mt-4">
        <summary class="cursor-pointer text-sm font-medium">
          Standard output<span v-if="details.stdout_truncated"> · first 64 KiB</span>
        </summary>
        <pre
          class="mt-2 max-h-64 overflow-auto font-mono text-xs break-words whitespace-pre-wrap"
          >{{ details.stdout || 'No output.' }}</pre>
      </details>
      <details class="mt-3">
        <summary class="cursor-pointer text-sm font-medium">
          Standard error<span v-if="details.stderr_truncated"> · first 64 KiB</span>
        </summary>
        <pre
          class="mt-2 max-h-64 overflow-auto font-mono text-xs break-words whitespace-pre-wrap"
          >{{ details.stderr || 'No errors logged.' }}</pre>
      </details>
    </template>
  </section>
</template>
