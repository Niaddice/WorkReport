<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  PhArrowClockwise,
  PhCalendarDots,
  PhChartLine,
  PhClockCounterClockwise,
  PhFileXls,
  PhFolderOpen,
  PhGearSix,
  PhGitBranch,
  PhNotePencil,
  PhPalette,
  PhSparkle,
  PhSquaresFour,
} from '@phosphor-icons/vue'
import { moduleType, type ModuleId, type ModuleType } from '../modules/types'
import type { ContextMenuItem } from '../lib/contextMenus'
import ContextMenu from './ContextMenu.vue'
import { DEFAULT_RAIL_HOLD_MS } from '../lib/railHoldSettings'
import { useRailHoldClose } from '../lib/useRailHoldClose'

const props = withDefaults(
  defineProps<{
    disabled: boolean
    active: ModuleId[]
    settingsActive?: boolean
    settingsBusy?: boolean
    holdDelayMs?: number
    holdDurationMs?: number
  }>(),
  { holdDelayMs: DEFAULT_RAIL_HOLD_MS, holdDurationMs: DEFAULT_RAIL_HOLD_MS },
)
const emit = defineEmits<{
  module: [id: ModuleId]
  remove: [id: ModuleId]
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

type RailModule = ModuleType
function activeModuleId(type: RailModule): ModuleId | undefined {
  return props.active.filter((id) => moduleType(id) === type).at(-1)
}
const {
  holding,
  progress,
  position,
  label: holdLabel,
  start: startHold,
  move: moveHold,
  cancel: cancelHold,
  shouldSuppressClick,
} = useRailHoldClose({
  blocked: () => props.disabled || !!props.settingsBusy || !!props.settingsActive,
  resolve: activeModuleId,
  delay: () => props.holdDelayMs,
  duration: () => props.holdDurationMs,
  close: (id) => emit('remove', id),
})
function clickModule(event: MouseEvent, id: RailModule) {
  if (!shouldSuppressClick(event)) emit('module', id)
}
const contextMenu = ref<{ id: RailModule; x: number; y: number; anchor: HTMLButtonElement } | null>(null)
const menuItems = computed<ContextMenuItem[]>(() => {
  const type = contextMenu.value?.id
  if (!type) return []
  const id = activeModuleId(type)
  return [
    {
      id: 'close',
      label: '关闭模块',
      disabled: props.disabled || props.settingsBusy || !id,
      run: () => {
        if (id) emit('remove', id)
      },
    },
  ]
})

function openModuleMenu(event: MouseEvent | KeyboardEvent, id: RailModule) {
  if (event instanceof KeyboardEvent && event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return
  event.preventDefault()
  event.stopPropagation()
  cancelHold()
  closeMenu()
  if (props.disabled || props.settingsBusy) return
  const anchor = event.currentTarget as HTMLButtonElement
  const bounds = anchor.getBoundingClientRect()
  const mouse = event instanceof MouseEvent && (event.clientX !== 0 || event.clientY !== 0)
  contextMenu.value = {
    id,
    x: mouse ? event.clientX : bounds.left,
    y: mouse ? event.clientY : bounds.bottom,
    anchor,
  }
}
function closeMenu(restoreFocus = false) {
  const anchor = contextMenu.value?.anchor
  contextMenu.value = null
  if (restoreFocus && anchor?.isConnected && !anchor.disabled) anchor.focus({ preventScroll: true })
}
function chooseMenu(id: string) {
  const item = menuItems.value.find((item) => item.id === id)
  if (!item || item.disabled) return
  closeMenu(true)
  item.run()
}
watch(
  [
    () => props.disabled,
    () => props.settingsActive,
    () => props.settingsBusy,
    () => props.active.join(','),
    () => props.holdDelayMs,
    () => props.holdDurationMs,
  ],
  () => {
    cancelHold()
    closeMenu()
  },
)

const moduleButtons = [
  { id: 'calendar', label: '日历', icon: PhCalendarDots },
  { id: 'editor', label: '编辑', icon: PhNotePencil },
  { id: 'overview', label: '数据概览', icon: PhChartLine },
  { id: 'history', label: '历史记录', icon: PhClockCounterClockwise },
  { id: 'excel', label: 'Excel报告', icon: PhFileXls },
  { id: 'summary', label: '总结', icon: PhSparkle },
  { id: 'directory', label: '工作空间目录', icon: PhFolderOpen },
  { id: 'git', label: 'Git', icon: PhGitBranch },
] as const
</script>

<template>
  <aside class="app-rail" aria-label="功能与模块">
    <template v-for="item in moduleButtons" :key="item.id">
      <button
        type="button"
        class="rail-button"
        :class="{ 'is-active': !!activeModuleId(item.id) }"
        :aria-label="item.label"
        :aria-pressed="!!activeModuleId(item.id)"
        aria-haspopup="menu"
        :aria-expanded="contextMenu?.id === item.id"
        :title="activeModuleId(item.id) ? `${item.label}（已在首页，长按或右键关闭）` : `添加${item.label}模块`"
        :data-tooltip-disabled="holding === item.id ? '' : undefined"
        :disabled="disabled"
        @click="clickModule($event, item.id)"
        @pointerdown="startHold($event, item.id, item.label)"
        @pointermove="moveHold"
        @pointerleave="cancelHold()"
        @contextmenu="openModuleMenu($event, item.id)"
        @keydown="openModuleMenu($event, item.id)"
      >
        <component :is="item.icon" :size="19" aria-hidden="true" />
      </button>
    </template>
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
  <ContextMenu
    v-if="contextMenu"
    kind="module"
    :x="contextMenu.x"
    :y="contextMenu.y"
    :items="menuItems"
    @close="closeMenu"
    @choose="chooseMenu"
  />
  <Teleport to="body">
    <div
      v-if="progress !== null"
      class="rail-hold-progress"
      :style="{ left: `${position.x}px`, top: `${position.y}px` }"
      role="progressbar"
      :aria-label="`正在关闭${holdLabel}模块，松开鼠标取消`"
      aria-valuemin="0"
      aria-valuemax="100"
      :aria-valuenow="Math.round(progress * 100)"
    >
      <svg viewBox="0 0 36 36" aria-hidden="true">
        <circle class="rail-hold-track" cx="18" cy="18" r="14" />
        <circle
          class="rail-hold-arc"
          cx="18"
          cy="18"
          r="14"
          pathLength="100"
          stroke-dasharray="100"
          :stroke-dashoffset="100 - progress * 100"
        />
      </svg>
    </div>
  </Teleport>
</template>

<style scoped>
.rail-hold-progress {
  position: fixed;
  z-index: 1000;
  width: 36px;
  height: 36px;
  transform: translate(-50%, -50%);
  pointer-events: none;
  border-radius: 50%;
  background: color-mix(in srgb, var(--skin-surface) 85%, transparent);
  box-shadow: 0 2px 8px #0002;
}
.rail-hold-progress svg {
  display: block;
  width: 100%;
  height: 100%;
  overflow: visible;
  transform: rotate(-90deg);
}
.rail-hold-progress circle {
  fill: none;
  stroke-width: 3;
}
.rail-hold-track {
  stroke: var(--skin-accent-container);
}
.rail-hold-arc {
  stroke: var(--skin-accent);
  stroke-linecap: round;
}
</style>
