<script setup lang="ts">
  import * as v from 'valibot';
  import {
    configureRuntimePaths,
    jobErrorSchema,
    loadExampleDefaults,
    loadRuntimePaths,
  } from '~/utils/jobs';

  const node = shallowRef<string>('');
  const lightpanda = shallowRef<string>('');
  const busy = shallowRef<boolean>(true);
  const message = shallowRef<string>('');
  const error = shallowRef<string>('');

  async function save(): Promise<void> {
    busy.value = true;
    error.value = '';
    message.value = '';
    try {
      await configureRuntimePaths(node.value, lightpanda.value);
      message.value = 'Runtime paths saved.';
    } catch (error_) {
      error.value = v.parse(jobErrorSchema, error_);
    } finally {
      busy.value = false;
    }
  }

  onMounted(async () => {
    try {
      const defaults = await loadExampleDefaults();
      node.value = defaults.node ?? '';
      lightpanda.value = defaults.lightpanda ?? '';
      const paths = await loadRuntimePaths();
      node.value = paths.node;
      lightpanda.value = paths.lightpanda;
    } catch (error_) {
      if (node.value && lightpanda.value) {
        message.value = 'Installed runtimes detected. Save runtime paths before your first run.';
      } else {
        error.value = v.parse(jobErrorSchema, error_);
      }
    } finally {
      busy.value = false;
    }
  });
</script>

<template>
  <details
    :open="Boolean(message || error)"
    class="rounded-lg border p-4"
  >
    <summary class="cursor-pointer text-sm font-medium">Runtime executables</summary>
    <form
      class="mt-4 grid gap-3"
      @submit.prevent="save"
    >
      <p class="text-muted-foreground text-sm">
        Install Node and Lightpanda, then enter their absolute executable paths.
      </p>
      <label class="grid gap-1.5 text-sm"
        >Node<input
          v-model="node"
          class="bg-background rounded-md border px-3 py-2 font-mono text-xs"
          placeholder="/opt/homebrew/bin/node"
          required
      /></label>
      <label class="grid gap-1.5 text-sm"
        >Lightpanda<input
          v-model="lightpanda"
          class="bg-background rounded-md border px-3 py-2 font-mono text-xs"
          placeholder="/opt/homebrew/bin/lightpanda"
          required
      /></label>
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
      <Button
        :disabled="busy"
        type="submit"
        variant="outline"
        >Save runtime paths</Button
      >
    </form>
  </details>
</template>
