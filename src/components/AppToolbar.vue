<script setup lang="ts">
import { PhCaretDown, PhHouse, PhKeyboard, PhMagnifyingGlass } from '@phosphor-icons/vue'

defineProps<{
  page: 'home' | 'settings'
  nativeTitlebar?: boolean
  isMac?: boolean
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
const emit = defineEmits<{
  navigate: [page: 'home' | 'settings']
  refresh: []
  workspace: []
  search: []
  shortcuts: []
}>()
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
            v-if="hasWorkspace && page === 'home'"
            type="button"
            class="toolbar-search-button"
            aria-label="全局检索"
            :title="`全局全文检索 (${isMac ? '⌘K' : 'Ctrl+K'})`"
            :disabled="
              busy || excelBusy || picking || settingsSaving || settingsReading || settingsLeaveBusy || databaseBusy
            "
            @click="emit('search')"
          >
            <PhMagnifyingGlass :size="15" aria-hidden="true" />
            <span>搜索…</span>
            <kbd aria-hidden="true">{{ isMac ? '⌘K' : 'Ctrl+K' }}</kbd>
          </button>
          <button
            v-if="hasWorkspace && page === 'home'"
            type="button"
            class="toolbar-shortcuts-button"
            aria-label="快捷键速查"
            :title="`快捷键速查 (${isMac ? '⌘/' : 'Ctrl+/'})`"
            :disabled="
              busy || excelBusy || picking || settingsSaving || settingsReading || settingsLeaveBusy || databaseBusy
            "
            @click="emit('shortcuts')"
          >
            <PhKeyboard :size="16" aria-hidden="true" />
          </button>
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

<style scoped>
.toolbar-search-button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 9px;
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  background: color-mix(in srgb, var(--skin-surface) 80%, transparent);
  color: var(--skin-text-secondary);
  font: inherit;
  font-size: var(--text-small);
  cursor: pointer;
  transition:
    border-color 140ms ease,
    background-color 140ms ease,
    color 140ms ease;
}

.toolbar-search-button:hover:not(:disabled) {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
  color: var(--skin-text);
}

.toolbar-search-button:disabled {
  opacity: 0.45;
  cursor: default;
}

.toolbar-shortcuts-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  background: var(--skin-surface);
  color: var(--skin-text-muted);
  cursor: pointer;
  transition:
    border-color 140ms ease,
    background-color 140ms ease,
    color 140ms ease;
}

.toolbar-shortcuts-button:hover:not(:disabled) {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
  color: var(--skin-text);
}

.toolbar-shortcuts-button:disabled {
  opacity: 0.45;
  cursor: default;
}

.toolbar-search-button kbd {
  padding: 1px 5px;
  border: 1px solid var(--skin-border);
  border-radius: 4px;
  background: var(--skin-canvas);
  color: var(--skin-text-muted);
  font-family: inherit;
  font-size: var(--text-micro);
  line-height: 1.25;
}
</style>
