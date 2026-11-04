<script setup lang="ts">
import { PhCaretDown, PhHouse } from '@phosphor-icons/vue'

defineProps<{
  page: 'home' | 'settings'
  nativeTitlebar?: boolean
  greeting: string
  dateLabel: string
  hasWorkspace: boolean
  workspaceName?: string
  workspacePath?: string
  busy: boolean
  picking: boolean
  excelBusy: boolean
  settingsSaving: boolean
  settingsReading: boolean
  settingsLeaveBusy: boolean
  databaseBusy: boolean
  refreshRevision: number
}>()
const emit = defineEmits<{ navigate: [page: 'home' | 'settings']; refresh: []; workspace: [] }>()
</script>

<template>
  <header
    class="home-heading app-toolbar"
    :class="{ 'is-native-titlebar': nativeTitlebar }"
    :data-tauri-drag-region="nativeTitlebar ? 'deep' : undefined"
  >
    <div class="toolbar-workspace">
      <span class="toolbar-app-mark" aria-hidden="true">W</span>
      <span class="titlebar-app-name">WorkReport</span>
      <span v-if="hasWorkspace" class="toolbar-context-divider" aria-hidden="true"></span>
      <button
        v-if="hasWorkspace"
        type="button"
        class="toolbar-workspace-button"
        aria-label="切换工作空间"
        :title="workspacePath || workspaceName || '切换工作空间'"
        :disabled="
          busy || excelBusy || picking || settingsSaving || settingsReading || settingsLeaveBusy || databaseBusy
        "
        @click="emit('workspace')"
      >
        <span>{{ workspaceName || '工作空间' }}</span
        ><PhCaretDown :size="12" aria-hidden="true" />
      </button>
    </div>
    <h1 class="home-greeting" :title="`${greeting}，今天是 ${dateLabel}`">
      <span class="greeting-salutation">{{ greeting }}</span
      ><span class="greeting-date">，今天是 {{ dateLabel }}</span>
    </h1>
    <div class="home-heading-actions">
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
