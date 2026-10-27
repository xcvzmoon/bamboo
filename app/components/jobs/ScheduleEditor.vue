<script setup lang="ts">
  import type { Schedule, ScheduleDetails } from '~/utils/schedules';
  import * as v from 'valibot';
  import { jobErrorSchema } from '~/utils/jobs';
  import { loadSchedule, saveSchedule } from '~/utils/schedules';

  const props = defineProps<{ jobId: string }>();
  const details = shallowRef<ScheduleDetails | null>(null);
  const loading = shallowRef<boolean>(true);
  const saving = shallowRef<boolean>(false);
  const error = shallowRef<string>('');
  const message = shallowRef<string>('');
  let request = 0;

  async function refresh(): Promise<void> {
    const current = ++request;
    try {
      const loaded = await loadSchedule(props.jobId);
      if (current === request) {
        details.value = loaded;
        error.value = '';
      }
    } catch (error_) {
      if (current === request) {
        error.value = v.parse(jobErrorSchema, error_);
      }
    } finally {
      loading.value = false;
    }
  }

  async function save(schedule: Schedule): Promise<void> {
    request += 1;
    saving.value = true;
    error.value = '';
    message.value = '';
    try {
      details.value = await saveSchedule(props.jobId, schedule);
      message.value = 'Schedule saved.';
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      saving.value = false;
    }
  }

  useIntervalFn(async () => {
    if (!saving.value && !loading.value) {
      await refresh();
    }
  }, 5000);
  onMounted(refresh);
</script>

<template>
  <section
    class="grid gap-3 rounded-lg border p-4"
    aria-label="Job schedule"
  >
    <JobsScheduleForm
      :schedule="details?.saved.schedule ?? null"
      :revision="details?.saved.revision ?? null"
      :disabled="loading || saving"
      @save="save"
    />

    <p
      v-if="loading"
      class="text-muted-foreground text-sm"
      role="status"
    >
      Loading schedule…
    </p>

    <p
      v-else-if="details && details.next_at !== null"
      class="text-sm"
    >
      Next occurrence {{ new Date(details.next_at).toLocaleString() }}
    </p>

    <p
      v-else-if="!error"
      class="text-muted-foreground text-sm"
    >
      Automatic runs are disabled.
    </p>

    <Button
      :disabled="loading || saving"
      type="button"
      variant="ghost"
      size="sm"
      @click="refresh"
      >Reload schedule</Button
    >

    <p class="text-muted-foreground text-xs">
      Schedules run while Bamboo is open and use saved input and login settings. Daily times follow
      the system timezone. Busy and missed occurrences are skipped; manual runs take priority.
    </p>

    <p
      v-if="message"
      class="text-sm"
      role="status"
    >
      {{ message }}
    </p>

    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>
  </section>
</template>
