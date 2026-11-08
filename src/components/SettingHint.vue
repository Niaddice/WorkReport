<script setup lang="ts">
import { PhQuestion } from '@phosphor-icons/vue'

defineProps<{
  text: string
}>()
</script>

<template>
  <button type="button" class="setting-hint-trigger" :data-tooltip="text" :title="text" :aria-label="`提示：${text}`">
    <PhQuestion :size="12" aria-hidden="true" />
    <span class="sr-only">{{ text }}</span>
  </button>
</template>

<style scoped>
.setting-hint-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  margin-left: 5px;
  border: 1px solid var(--skin-border, rgba(0, 0, 0, 0.14));
  border-radius: 50%;
  background: var(--skin-subtle, rgba(127, 127, 127, 0.08));
  color: var(--skin-text-muted, #71717a);
  font-size: 11px;
  line-height: 1;
  cursor: help;
  vertical-align: middle;
  transition: all 120ms ease;
  flex-shrink: 0;
  outline: none;
}

.setting-hint-trigger:hover,
.setting-hint-trigger:focus-visible {
  border-color: var(--skin-accent, #2563eb);
  background: var(--skin-accent-soft, rgba(59, 130, 246, 0.14));
  color: var(--skin-accent, #2563eb);
}
</style>
