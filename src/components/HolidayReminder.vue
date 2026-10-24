<script setup lang="ts">
import { PhX } from '@phosphor-icons/vue'

defineProps<{ text: string; disabled: boolean }>()
const emit = defineEmits<{ open: []; dismiss: [] }>()
</script>

<template>
  <div class="holiday-reminder" role="status">
    <span>{{ text }}</span>
    <div class="holiday-reminder-actions">
      <button type="button" class="text-action" :disabled="disabled" @click="emit('open')">前往同步</button>
      <button
        type="button"
        class="toolbar-icon-button"
        aria-label="关闭放假同步提醒"
        title="关闭提醒"
        @click="emit('dismiss')"
      >
        <PhX :size="16" aria-hidden="true" />
      </button>
    </div>
  </div>
</template>
