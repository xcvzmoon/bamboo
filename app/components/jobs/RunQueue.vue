<script setup lang="ts">
  import type { QueueSnapshot } from '~/utils/jobs';
  import * as v from 'valibot';
  import { cancelRun, jobErrorSchema, loadQueueSnapshot } from '~/utils/jobs';

  const emit = defineEmits<{ select: [id: string]; changed: [] }>();
  const queue = shallowRef<QueueSnapshot | null>(null);
  const error = shallowRef<string>('');
  const cancelling = shallowRef<string | null>(null);
  let refreshing = false;
  let previous = '';

  async function refresh(): Promise<void> {
    if (refreshing) {
      return;
    }
    refreshing = true;
    try {
      const snapshot = await loadQueueSnapshot();
      const current = JSON.stringify(snapshot);
      if (current !== previous) {
        queue.value = snapshot;
        previous = current;
        emit('changed');
      }
      error.value = '';
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      refreshing = false;
    }
  }

  async function cancel(id: string): Promise<void> {
    cancelling.value = id;
    try {
      await cancelRun(id);
      await refresh();
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      cancelling.value = null;
    }
  }

  useIntervalFn(refresh, 1000, { immediateCallback: true });
</script>

<template>
  <section
    class="rounded-lg border p-4"
    aria-label="Run queue"
  >
    <h2 class="text-sm font-medium">Run queue</h2>
    <p
      v-if="error"
      class="text-destructive mt-2 text-sm"
      role="alert"
    >
      {{ error }}
    </p>
    <template v-if="queue">
      <p
        v-if="queue.closed"
        class="text-destructive mt-2 text-sm"
      >
        The queue is closed. Restart Bamboo before adding runs.
      </p>
      <p
        v-if="!queue.active && queue.pending.length === 0"
        class="text-muted-foreground mt-2 text-sm"
      >
        No active or waiting runs.
      </p>
      <div
        v-if="queue.active"
        class="mt-3 flex items-center gap-2"
      >
        <Button
          type="button"
          variant="ghost"
          class="min-w-0 flex-1 justify-start text-left whitespace-normal"
          @click="emit('select', queue.active.id)"
          >{{ queue.active.job.name }} ·
          {{ queue.active.status === 'queued' ? 'Starting' : queue.active.status }}</Button
        >
        <Button
          :disabled="cancelling !== null"
          type="button"
          variant="outline"
          size="sm"
          @click="cancel(queue.active.id)"
          >Cancel</Button
        >
      </div>
      <ol
        v-if="queue.pending.length > 0"
        class="mt-3 grid gap-2"
      >
        <li
          v-for="(run, index) in queue.pending"
          :key="run.id"
          class="flex items-center gap-2"
        >
          <Button
            type="button"
            variant="ghost"
            class="min-w-0 flex-1 justify-start text-left whitespace-normal"
            @click="emit('select', run.id)"
            >{{ index + 1 }}. {{ run.job.name }} · Waiting</Button
          >
          <Button
            :disabled="cancelling !== null"
            type="button"
            variant="outline"
            size="sm"
            @click="cancel(run.id)"
            >Cancel</Button
          >
        </li>
      </ol>
      <p class="text-muted-foreground mt-3 text-xs">
        One run at a time. Waiting runs use the settings saved when added. Keep Bamboo open.
      </p>
    </template>
  </section>
</template>
