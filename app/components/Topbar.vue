<script setup lang="ts">
  import { version } from '~~/package.json';
</script>

<template>
  <header
    class="flex h-9.25 items-center justify-between border-b ps-21 pe-1.5"
    data-tauri-drag-region
  >
    <h1 class="text-xs">{{ `Bamboo v${version}` }}</h1>

    <ColorModeButton
      size="icon-xs"
      variant="ghost"
    />
  </header>
</template>
