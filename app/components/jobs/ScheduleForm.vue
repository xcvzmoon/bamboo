<script setup lang="ts">
  import type { Schedule } from '~/utils/schedules';
  import * as v from 'valibot';
  import { jobErrorSchema } from '~/utils/jobs';
  import { parseScheduleForm } from '~/utils/schedules';

  const props = defineProps<{
    schedule: Schedule | null;
    revision: string | null;
    disabled: boolean;
  }>();
  const emit = defineEmits<{ save: [schedule: Schedule] }>();
  const kind = shallowRef<Schedule['kind']>('disabled');
  const every = shallowRef<number>(1);
  const unit = shallowRef<'minutes' | 'hours'>('minutes');
  const time = shallowRef<string>('09:00');
  const error = shallowRef<string>('');

  function reset(): void {
    const schedule = props.schedule;
    kind.value = schedule?.kind ?? 'disabled';
    if (schedule?.kind === 'interval') {
      every.value = schedule.every;
      unit.value = schedule.unit;
    } else if (schedule?.kind === 'daily') {
      time.value = `${String(schedule.hour).padStart(2, '0')}:${String(schedule.minute).padStart(2, '0')}`;
    }
    error.value = '';
  }

  function submit(): void {
    try {
      const schedule = parseScheduleForm(kind.value, every.value, unit.value, time.value);
      error.value = '';
      emit('save', schedule);
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    }
  }

  watch(() => props.revision, reset, { immediate: true });
</script>

<template>
  <form
    class="grid gap-3"
    @submit.prevent="submit"
  >
    <label class="grid gap-1.5 text-sm">
      Schedule
      <select
        v-model="kind"
        :disabled="disabled"
        class="bg-background rounded-md border px-3 py-2"
      >
        <option value="disabled">Disabled</option>
        <option value="interval">Every interval</option>
        <option value="daily">Daily at a local time</option>
      </select>
    </label>

    <div
      v-if="kind === 'interval'"
      class="flex gap-3"
    >
      <label class="grid min-w-0 gap-1.5 text-sm">
        Every
        <Input
          v-model.number="every"
          :disabled="disabled"
          type="number"
          min="1"
          max="4294967295"
          step="1"
          required
        />
      </label>

      <label class="grid gap-1.5 text-sm">
        Unit
        <select
          v-model="unit"
          :disabled="disabled"
          class="bg-background rounded-md border px-3 py-2"
        >
          <option value="minutes">Minutes</option>
          <option value="hours">Hours</option>
        </select>
      </label>
    </div>

    <label
      v-if="kind === 'daily'"
      class="grid gap-1.5 text-sm"
    >
      Local time
      <Input
        v-model="time"
        :disabled="disabled"
        type="time"
        step="60"
        required
      />
    </label>

    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>

    <Button
      :disabled="disabled"
      type="submit"
      variant="outline"
      >Save schedule</Button
    >
  </form>
</template>
