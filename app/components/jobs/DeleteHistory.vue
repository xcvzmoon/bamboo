<script setup lang="ts">
  import type { DeleteRunResult, HistoryTarget } from '~/utils/history';
  import * as v from 'valibot';
  import { deleteHistory } from '~/utils/history';
  import { jobErrorSchema } from '~/utils/jobs';

  const props = defineProps<{ target: HistoryTarget; disabled?: boolean }>();
  const emit = defineEmits<{ deleted: [ids: string[]] }>();
  const confirming = shallowRef<boolean>(false);
  const deleting = shallowRef<boolean>(false);
  const error = shallowRef<string>('');
  const result = shallowRef<DeleteRunResult | null>(null);

  async function remove(): Promise<void> {
    deleting.value = true;
    error.value = '';
    result.value = null;
    try {
      const deleted = await deleteHistory(props.target);
      result.value = deleted;
      confirming.value = deleted.failed.length > 0;
      if (deleted.deleted.length > 0) {
        emit('deleted', deleted.deleted);
      }
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      deleting.value = false;
    }
  }
</script>

<template>
  <div class="grid gap-2">
    <Button
      v-if="!confirming"
      :disabled="disabled || deleting"
      type="button"
      variant="outline"
      size="sm"
      @click="confirming = true"
      >{{ target.kind === 'run' ? 'Delete run' : 'Delete all history' }}</Button
    >

    <div
      v-else
      class="grid gap-2 rounded-md border p-3"
    >
      <p class="text-sm">
        {{
          target.kind === 'run'
            ? 'Delete the logs, results, and metadata for this run?'
            : 'Delete all logs, results, and run metadata for this job?'
        }}
        This cannot be undone. Job settings, schedules, and login profiles are kept.
      </p>
      <div class="flex gap-2">
        <Button
          :disabled="disabled || deleting"
          type="button"
          variant="destructive"
          size="sm"
          @click="remove"
          >{{ deleting ? 'Deleting…' : 'Confirm deletion' }}</Button
        >
        <Button
          :disabled="deleting"
          type="button"
          variant="ghost"
          size="sm"
          @click="confirming = false"
          >Cancel</Button
        >
      </div>
    </div>

    <p
      v-if="error"
      class="text-destructive text-sm"
      role="alert"
    >
      {{ error }}
    </p>

    <template v-if="result">
      <p
        class="text-muted-foreground text-sm"
        role="status"
      >
        {{ result.deleted.length }} runs deleted.
      </p>
      <p
        v-for="failure in result.failed"
        :key="failure.run_id"
        class="text-destructive text-sm break-all"
        role="alert"
      >
        {{ failure.run_id }} · {{ failure.code }}: {{ failure.message }}
      </p>
    </template>
  </div>
</template>
