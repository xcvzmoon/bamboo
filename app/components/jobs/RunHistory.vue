<script setup lang="ts">
  import type { RunSummary } from '~/utils/jobs';
  import * as v from 'valibot';
  import { jobErrorSchema, listRuns } from '~/utils/jobs';

  const props = defineProps<{ jobId: string; revision: number }>();
  const emit = defineEmits<{ select: [id: string]; deleted: [ids: string[]] }>();
  const runs = shallowRef<RunSummary[]>([]);
  const offset = shallowRef<number>(0);
  const loading = shallowRef<boolean>(false);
  const error = shallowRef<string>('');
  let request = 0;

  async function refresh(): Promise<void> {
    const current = ++request;
    loading.value = true;
    error.value = '';
    try {
      const entries = await listRuns(props.jobId, offset.value);
      if (current === request) {
        runs.value = entries;
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

  watch(
    () => [props.jobId, props.revision],
    () => {
      offset.value = 0;
      void refresh();
    },
    { immediate: true },
  );
  watch(offset, refresh);
</script>

<template>
  <details class="rounded-lg border p-4">
    <summary class="cursor-pointer text-sm font-medium">Run history</summary>
    <div class="mt-3 grid gap-2">
      <JobsDeleteHistory
        :target="{ kind: 'job', id: jobId }"
        :disabled="loading"
        @deleted="emit('deleted', $event)"
      />
      <p
        v-if="error"
        class="text-destructive text-sm"
        role="alert"
      >
        {{ error }}
      </p>
      <p
        v-if="loading"
        class="text-muted-foreground text-sm"
        role="status"
      >
        Loading runs…
      </p>
      <p
        v-else-if="runs.length === 0"
        class="text-muted-foreground text-sm"
      >
        No runs on this page.
      </p>
      <Button
        v-for="run in runs"
        :key="run.id"
        type="button"
        variant="outline"
        class="h-auto justify-start py-2 text-left whitespace-normal"
        @click="emit('select', run.id)"
      >
        {{ new Date(run.queued_at).toLocaleString() }} · {{ run.status
        }}<span v-if="run.record_count !== null"> · {{ run.record_count }} records</span>
      </Button>
      <div class="flex gap-2">
        <Button
          :disabled="loading || offset === 0"
          type="button"
          variant="ghost"
          size="sm"
          @click="offset = Math.max(0, offset - 20)"
          >Newer</Button
        >
        <Button
          :disabled="loading || runs.length < 20"
          type="button"
          variant="ghost"
          size="sm"
          @click="offset += 20"
          >Older</Button
        >
        <Button
          :disabled="loading"
          type="button"
          variant="ghost"
          size="sm"
          @click="refresh"
          >Refresh</Button
        >
      </div>
    </div>
  </details>
</template>
