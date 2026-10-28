<script setup lang="ts">
import { ref } from 'vue'
import {
  PhArrowClockwise,
  PhCalendarDots,
  PhChartLine,
  PhClockCounterClockwise,
  PhFileXls,
  PhFolderOpen,
  PhGearSix,
  PhNotePencil,
  PhPalette,
  PhSparkle,
  PhSquaresFour,
} from '@phosphor-icons/vue'
import type { ModuleId } from '../modules/types'

defineProps<{
  disabled: boolean
  active: ModuleId[]
  settingsActive?: boolean
  settingsBusy?: boolean
}>()
const emit = defineEmits<{
  module: [id: ModuleId]
  refresh: []
  settings: []
  skin: []
  layout: []
}>()
const settingsButton = ref<HTMLButtonElement>()
function getSettingsRect(): DOMRect | null {
  const rect = settingsButton.value?.getBoundingClientRect()
  return rect && rect.width > 0 && rect.height > 0 ? rect : null
}
defineExpose({ getSettingsRect })

const moduleButtons = [
  { id: 'calendar', label: '日历', icon: PhCalendarDots },
  { id: 'editor', label: '日报编辑', icon: PhNotePencil },
  { id: 'overview', label: '数据概览', icon: PhChartLine },
  { id: 'history', label: '历史记录', icon: PhClockCounterClockwise },
  { id: 'excel', label: 'Excel 周报', icon: PhFileXls },
  { id: 'summary', label: '总结', icon: PhSparkle },
  { id: 'directory', label: '工作空间目录', icon: PhFolderOpen },
] as const
</script>

<template>
  <aside class="app-rail" aria-label="功能与模块">
    <button
      v-for="item in moduleButtons"
      :key="item.id"
      type="button"
      class="rail-button"
      :class="{ 'is-active': active.includes(item.id) }"
      :aria-label="item.label"
      :aria-pressed="active.includes(item.id)"
      :title="active.includes(item.id) ? `${item.label}（已在首页）` : `添加${item.label}模块`"
      :disabled="disabled"
      @click="emit('module', item.id)"
    >
      <component :is="item.icon" :size="19" aria-hidden="true" />
    </button>
    <span class="rail-divider" aria-hidden="true"></span>
    <button
      type="button"
      class="rail-button"
      aria-label="布局"
      title="布局与分栏"
      :disabled="disabled"
      @click="emit('layout')"
    >
      <PhSquaresFour :size="19" aria-hidden="true" />
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
    <div class="rail-bottom">
      <span class="rail-divider" aria-hidden="true"></span>
      <button
        ref="settingsButton"
        type="button"
        class="rail-button"
        :class="{ 'is-active': settingsActive }"
        aria-label="设置"
        :title="settingsActive ? '保存设置并返回首页' : '设置'"
        :aria-description="settingsActive ? '保存当前设置并返回首页' : '打开设置'"
        :aria-pressed="!!settingsActive"
        :disabled="disabled || settingsBusy"
        @click="emit('settings')"
      >
        <PhGearSix :size="19" aria-hidden="true" />
      </button>
    </div>
  </aside>
</template>
