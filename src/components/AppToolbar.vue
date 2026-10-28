<script setup lang="ts">
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
</script>

<template>
  <header
    class="home-heading app-toolbar"
    :class="{ 'is-native-titlebar': nativeTitlebar }"
    :data-tauri-drag-region="nativeTitlebar ? 'deep' : undefined"
  >
    <span v-if="nativeTitlebar" class="titlebar-app-name" aria-hidden="true">WorkReport</span>
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
