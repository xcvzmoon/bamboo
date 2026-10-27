<script setup lang="ts">
  import type { ProfileForm } from '~/utils/profiles';
  import * as v from 'valibot';
  import { profileFormSchema } from '~/utils/profiles';

  defineProps<{ disabled: boolean }>();
  const emit = defineEmits<{ start: [form: ProfileForm] }>();
  const name = shallowRef<string>('Quotes demo');
  const loginUrl = shallowRef<string>('https://quotes.toscrape.com/login');
  const browser = shallowRef<string>('/Applications/Helium.app/Contents/MacOS/Helium');
  const error = shallowRef<string>('');

  function submit(): void {
    const result = v.safeParse(profileFormSchema, {
      name: name.value,
      loginUrl: loginUrl.value,
      browser: browser.value,
    });
    error.value = result.success ? '' : result.issues[0].message;
    if (result.success) {
      emit('start', result.output);
    }
  }
</script>

<template>
  <form
    class="grid gap-3"
    @submit.prevent="submit"
  >
    <label class="grid gap-1 text-sm">
      Profile name
      <Input
        v-model="name"
        :disabled="disabled"
        required
      />
    </label>

    <label class="grid gap-1 text-sm">
      Login URL
      <Input
        v-model="loginUrl"
        :disabled="disabled"
        type="url"
        required
      />
    </label>

    <label class="grid gap-1 text-sm">
      Browser executable
      <Input
        v-model="browser"
        :disabled="disabled"
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
      >Open login browser</Button
    >
  </form>
</template>
