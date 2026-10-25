<script setup lang="ts">
  const colorMode = useColorMode();

  const isDark = computed({
    get: () => colorMode.value === 'dark',
    set: (_isDark) => (colorMode.preference = _isDark ? 'dark' : 'light'),
  });
</script>

<template>
  <Button
    :aria-label="`Switch to ${isDark ? 'light' : 'dark'} mode`"
    size="icon"
    @click="isDark = !isDark"
  >
    <Icon :name="isDark ? 'i-hugeicons-moon-02' : 'i-hugeicons-sun-03'" />
  </Button>
</template>
