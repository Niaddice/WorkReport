<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { PhHouse } from '@phosphor-icons/vue'

defineProps<{
  page: 'home' | 'settings'
  nativeTitlebar?: boolean
  greeting: string
  dateLabel: string
  hasWorkspace: boolean
  busy: boolean
  picking: boolean
  excelBusy: boolean
  settingsSaving: boolean
  settingsReading: boolean
  settingsLeaveBusy: boolean
  databaseBusy: boolean
  refreshRevision: number
}>()
const emit = defineEmits<{ navigate: [page: 'home' | 'settings']; refresh: [] }>()

/* —— 临时调试：在标题栏显示窗口像素与根字号，验证完可整段删除 —— */
const viewportSize = ref('')
const rootFontSize = ref('')
const editorFontSize = ref('')
const probe = ref<HTMLElement>()
const editorProbe = ref<HTMLElement>()
function measureDebug() {
  viewportSize.value = `${window.innerWidth}×${window.innerHeight}`
  if (probe.value) rootFontSize.value = getComputedStyle(probe.value).fontSize
  if (editorProbe.value) editorFontSize.value = getComputedStyle(editorProbe.value).fontSize
}
let debugTimer: ReturnType<typeof setInterval> | undefined
onMounted(() => {
  measureDebug()
  window.addEventListener('resize', measureDebug)
  debugTimer = setInterval(measureDebug, 1000)
})
onBeforeUnmount(() => {
  window.removeEventListener('resize', measureDebug)
  if (debugTimer) clearInterval(debugTimer)
})
</script>

<template>
  <header
    class="home-heading app-toolbar"
    :class="{ 'is-native-titlebar': nativeTitlebar }"
    :data-tauri-drag-region="nativeTitlebar ? 'deep' : undefined"
  >
    <span v-if="nativeTitlebar" class="titlebar-app-name" aria-hidden="true">WorkReport</span>
    <!-- 临时调试探针：用于读取 --font-unit 与编辑区字号的计算值 -->
    <span ref="probe" class="debug-probe" aria-hidden="true"></span>
    <span ref="editorProbe" class="debug-probe debug-probe-editor" aria-hidden="true"></span>
    <h1 class="home-greeting" :title="`${greeting}，今天是 ${dateLabel}`">
      <span class="greeting-salutation">{{ greeting }}</span
      ><span class="greeting-date">，今天是 {{ dateLabel }}</span>
    </h1>
    <div class="home-heading-actions">
      <span class="debug-metrics" aria-hidden="true"
        >{{ viewportSize }} · 根字号 {{ rootFontSize }} · 编辑区 {{ editorFontSize }}</span
      >
      <Transition name="toolbar-switch" mode="out-in">
        <div :key="page" class="toolbar-action-group">
          <button
            v-if="page === 'settings'"
            type="button"
            class="toolbar-icon-button"
            aria-label="返回首页"
            title="返回首页"
            :disabled="busy || excelBusy || settingsSaving || settingsReading || settingsLeaveBusy || databaseBusy"
            @click="emit('navigate', 'home')"
          >
            <PhHouse :size="20" aria-hidden="true" />
          </button>
        </div>
      </Transition>
    </div>
  </header>
</template>
