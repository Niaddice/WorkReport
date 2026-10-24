<script setup lang="ts">
import {
  PhArrowClockwise,
  PhClockCounterClockwise,
  PhFileXls,
  PhGearSix,
  PhPalette,
  PhSparkle,
} from '@phosphor-icons/vue'

defineProps<{ disabled: boolean }>()
const emit = defineEmits<{ refresh: []; settings: []; skin: []; summary: []; excel: []; history: [] }>()
</script>

<template>
  <aside class="app-rail" aria-label="功能区">
    <button
      type="button"
      class="rail-button"
      aria-label="设置"
      title="设置"
      :disabled="disabled"
      @click="emit('settings')"
    >
      <PhGearSix :size="19" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="rail-button"
      aria-label="切换皮肤"
      title="切换皮肤"
      :disabled="disabled"
      @click="emit('skin')"
    >
      <PhPalette :size="19" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="rail-button"
      aria-label="刷新"
      title="刷新"
      :disabled="disabled"
      @click="emit('refresh')"
    >
      <PhArrowClockwise :size="19" aria-hidden="true" />
    </button>
    <span class="rail-divider" aria-hidden="true"></span>
    <button
      type="button"
      class="rail-button"
      aria-label="AI 总结"
      title="AI 总结"
      :disabled="disabled"
      @click="emit('summary')"
    >
      <PhSparkle :size="19" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="rail-button"
      aria-label="Excel 周报"
      title="Excel 周报"
      :disabled="disabled"
      @click="emit('excel')"
    >
      <PhFileXls :size="19" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="rail-button"
      aria-label="历史记录"
      title="历史记录"
      :disabled="disabled"
      @click="emit('history')"
    >
      <PhClockCounterClockwise :size="19" aria-hidden="true" />
    </button>
  </aside>
</template>
