<script setup lang="ts">
  import type { SessionProfile } from '~/utils/profiles';

  defineProps<{ profiles: SessionProfile[]; disabled: boolean }>();
  const emit = defineEmits<{ refresh: [profile: SessionProfile] }>();
</script>

<template>
  <ul
    v-if="profiles.length"
    class="grid gap-3"
    aria-label="Saved login profiles"
  >
    <li
      v-for="profile in profiles"
      :key="profile.id"
      class="flex items-center justify-between gap-3 rounded-lg border p-3"
    >
      <div>
        <p class="text-sm font-medium">{{ profile.name }}</p>
        <p
          v-if="profile.needs_refresh"
          class="text-destructive text-sm"
        >
          Login expired. Refresh this profile.
        </p>
        <p class="text-muted-foreground text-xs">
          Saved {{ new Date(profile.captured_at * 1000).toLocaleString() }}
        </p>
      </div>

      <Button
        :disabled="disabled"
        type="button"
        variant="outline"
        size="sm"
        @click="emit('refresh', profile)"
        >Refresh login</Button
      >
    </li>
  </ul>

  <p
    v-else
    class="text-muted-foreground text-sm"
  >
    No saved login profiles yet.
  </p>
</template>
