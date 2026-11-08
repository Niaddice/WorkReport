<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { PhGitBranch, PhKeyboard, PhMagnifyingGlass, PhNotePencil, PhSparkle, PhX } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import { isMacPlatform, SHORTCUT_DEFINITIONS, type ShortcutItem } from '../lib/shortcuts'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const dialog = ref<HTMLDialogElement>()
const searchInput = ref<HTMLInputElement>()
const motion = createDialogMotion(() => dialog.value)
const isMac = isMacPlatform()

const query = ref('')
const activeCategory = ref<'all' | 'global' | 'editor' | 'ai' | 'git'>('all')

const categories = [
  { id: 'all', label: '全部快捷键', icon: PhKeyboard },
  { id: 'global', label: '通用全局', icon: PhKeyboard },
  { id: 'editor', label: '日报编辑', icon: PhNotePencil },
  { id: 'ai', label: 'AI 辅助', icon: PhSparkle },
  { id: 'git', label: 'Git 工作台', icon: PhGitBranch },
] as const

const filteredShortcuts = computed<ShortcutItem[]>(() => {
  const q = query.value.trim().toLowerCase()
  return SHORTCUT_DEFINITIONS.filter((item) => {
    if (activeCategory.value !== 'all' && item.category !== activeCategory.value) {
      return false
    }
    if (!q) return true
    return (
      item.title.toLowerCase().includes(q) ||
      item.description.toLowerCase().includes(q) ||
      item.categoryLabel.toLowerCase().includes(q) ||
      item.keys.some((k) => k.toLowerCase().includes(q)) ||
      (item.macKeys?.some((k) => k.toLowerCase().includes(q)) ?? false)
    )
  })
})

const groupedShortcuts = computed(() => {
  if (activeCategory.value !== 'all') {
    return [
      {
        category: activeCategory.value,
        label: categories.find((c) => c.id === activeCategory.value)?.label ?? '',
        items: filteredShortcuts.value,
      },
    ]
  }

  const map = new Map<string, { category: string; label: string; items: ShortcutItem[] }>()
  for (const item of filteredShortcuts.value) {
    if (!map.has(item.category)) {
      map.set(item.category, {
        category: item.category,
        label: item.categoryLabel,
        items: [],
      })
    }
    map.get(item.category)!.items.push(item)
  }
  return Array.from(map.values())
})

watch(
  () => props.visible,
  (val) => {
    if (val) {
      query.value = ''
      activeCategory.value = 'all'
      motion.open()
      void nextTick(() => {
        searchInput.value?.focus()
      })
    } else {
      void motion.close()
    }
  },
  { immediate: true, flush: 'post' },
)

onBeforeUnmount(() => {
  motion.dispose()
})

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    emit('close')
  }
}
</script>

<template>
  <Teleport to="body">
    <dialog
      id="shortcuts-dialog"
      ref="dialog"
      tabindex="-1"
      role="dialog"
      aria-modal="true"
      class="shortcuts-dialog confirmation-dialog"
      aria-labelledby="shortcuts-dialog-title"
      @cancel.prevent="emit('close')"
      @keydown="onKeydown"
    >
      <div class="shortcuts-window">
        <!-- 弹窗顶栏 -->
        <header class="shortcuts-header">
          <div class="shortcuts-header-title">
            <div class="shortcuts-title-icon" aria-hidden="true">
              <PhKeyboard :size="20" />
            </div>
            <div>
              <h3 id="shortcuts-dialog-title" class="shortcuts-title">快捷键速查与高效指南</h3>
              <p class="shortcuts-subtitle">
                随时按下 <kbd>{{ isMac ? '⌘/' : 'Ctrl+/' }}</kbd> 打开或关闭此速查面板
              </p>
            </div>
          </div>
          <button
            type="button"
            class="shortcuts-close-btn"
            aria-label="关闭快捷键速查 (Esc)"
            title="关闭 (Esc)"
            @click="emit('close')"
          >
            <PhX :size="16" aria-hidden="true" />
          </button>
        </header>

        <!-- 搜索与分类导航 -->
        <div class="shortcuts-filter-bar">
          <div class="shortcuts-search-wrap">
            <PhMagnifyingGlass :size="15" class="search-icon" aria-hidden="true" />
            <input
              ref="searchInput"
              v-model="query"
              type="text"
              class="shortcuts-search-input"
              placeholder="搜索快捷键或功能名称…"
              aria-label="搜索快捷键"
            />
            <button v-if="query" type="button" class="search-clear-btn" aria-label="清除搜索条件" @click="query = ''">
              <PhX :size="12" aria-hidden="true" />
            </button>
          </div>

          <div class="shortcuts-category-tabs" role="tablist" aria-label="快捷键分类">
            <button
              v-for="cat in categories"
              :key="cat.id"
              type="button"
              role="tab"
              :aria-selected="activeCategory === cat.id"
              class="category-tab-btn"
              :class="{ 'is-active': activeCategory === cat.id }"
              @click="activeCategory = cat.id"
            >
              <component :is="cat.icon" :size="14" aria-hidden="true" />
              <span>{{ cat.label }}</span>
            </button>
          </div>
        </div>

        <!-- 快捷键列表内容区 -->
        <main class="shortcuts-body" tabindex="0">
          <div v-if="filteredShortcuts.length === 0" class="shortcuts-empty">
            <p>未找到匹配「{{ query }}」的快捷键</p>
          </div>

          <div v-else class="shortcuts-group-list">
            <section v-for="group in groupedShortcuts" :key="group.category" class="shortcuts-group-section">
              <h4 class="group-title">{{ group.label }}</h4>
              <div class="shortcuts-grid">
                <article v-for="item in group.items" :key="item.id" class="shortcut-card">
                  <div class="shortcut-info">
                    <span class="shortcut-name">{{ item.title }}</span>
                    <span class="shortcut-desc">{{ item.description }}</span>
                  </div>
                  <div class="shortcut-keys" aria-label="按键组合">
                    <template v-for="(keyPart, idx) in isMac && item.macKeys ? item.macKeys : item.keys" :key="idx">
                      <span v-if="idx > 0 && keyPart !== '/'" class="key-plus" aria-hidden="true">+</span>
                      <kbd class="key-cap">{{ keyPart }}</kbd>
                    </template>
                  </div>
                </article>
              </div>
            </section>
          </div>
        </main>

        <!-- 弹窗底栏 -->
        <footer class="shortcuts-footer">
          <div class="shortcuts-footer-hint">
            <span>输入框内支持标准光标导航与快捷键，全局随时响应保存与撤销</span>
          </div>
          <button
            type="button"
            class="dialog-button dialog-button-secondary shortcuts-confirm-close"
            @click="emit('close')"
          >
            知道了 (Esc)
          </button>
        </footer>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.shortcuts-dialog {
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  max-width: min(720px, calc(100vw - 32px));
  width: 720px;
}

.shortcuts-window {
  display: flex;
  flex-direction: column;
  max-height: min(680px, calc(100vh - 48px));
  background: var(--skin-surface);
  border: 1px solid var(--skin-border);
  border-radius: 12px;
  box-shadow: var(--skin-shadow-raised);
  overflow: hidden;
}

.shortcuts-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px 14px;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-surface-raised, var(--skin-surface));
}

.shortcuts-header-title {
  display: flex;
  align-items: center;
  gap: 12px;
}

.shortcuts-title-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 9px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}

.shortcuts-title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--skin-text);
}

.shortcuts-subtitle {
  margin: 2px 0 0;
  font-size: 12px;
  color: var(--skin-text-muted);
}

.shortcuts-subtitle kbd {
  display: inline-block;
  padding: 1px 5px;
  font-size: 11px;
  font-family: inherit;
  border: 1px solid var(--skin-border);
  border-radius: 4px;
  background: var(--skin-canvas);
  color: var(--skin-text);
}

.shortcuts-close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
  transition: all 140ms ease;
}

.shortcuts-close-btn:hover {
  background: var(--skin-canvas);
  color: var(--skin-text);
}

.shortcuts-filter-bar {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 20px 10px;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-canvas);
}

.shortcuts-search-wrap {
  position: relative;
  display: flex;
  align-items: center;
  width: 100%;
}

.shortcuts-search-wrap .search-icon {
  position: absolute;
  left: 10px;
  color: var(--skin-text-muted);
  pointer-events: none;
}

.shortcuts-search-input {
  width: 100%;
  height: 32px;
  padding: 0 30px 0 32px;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  background: var(--skin-surface);
  color: var(--skin-text);
  font-size: 13px;
  outline: none;
  transition:
    border-color 140ms ease,
    box-shadow 140ms ease;
}

.shortcuts-search-input:focus {
  border-color: var(--skin-accent);
  box-shadow: 0 0 0 2px var(--skin-accent-soft);
}

.search-clear-btn {
  position: absolute;
  right: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
}

.search-clear-btn:hover {
  color: var(--skin-text);
}

.shortcuts-category-tabs {
  display: flex;
  gap: 6px;
  overflow-x: auto;
}

.category-tab-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 10px;
  border: 1px solid transparent;
  border-radius: 13px;
  background: transparent;
  color: var(--skin-text-muted);
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
  transition: all 140ms ease;
}

.category-tab-btn:hover {
  background: var(--skin-surface);
  color: var(--skin-text);
}

.category-tab-btn.is-active {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  font-weight: 500;
}

.shortcuts-body {
  flex: 1;
  min-height: 280px;
  overflow-y: auto;
  padding: 16px 20px;
  outline: none;
}

.shortcuts-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 200px;
  color: var(--skin-text-muted);
  font-size: 14px;
}

.shortcuts-group-list {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.group-title {
  margin: 0 0 10px;
  font-size: 13px;
  font-weight: 600;
  color: var(--skin-text-muted);
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.shortcuts-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 10px;
}

.shortcut-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  background: var(--skin-surface);
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  transition:
    border-color 140ms ease,
    background-color 140ms ease;
}

.shortcut-card:hover {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
}

.shortcut-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.shortcut-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--skin-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.shortcut-desc {
  font-size: 11px;
  color: var(--skin-text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.shortcut-keys {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.key-plus {
  font-size: 11px;
  color: var(--skin-text-muted);
}

.key-cap {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  height: 22px;
  padding: 0 6px;
  font-size: 11px;
  font-weight: 600;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
  border: 1px solid var(--skin-border);
  border-bottom-width: 2px;
  border-radius: 5px;
  background: var(--skin-subtle);
  color: var(--skin-text);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
}

.shortcuts-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 20px;
  border-top: 1px solid var(--skin-border);
  background: var(--skin-surface-raised, var(--skin-surface));
}

.shortcuts-footer-hint {
  font-size: 12px;
  color: var(--skin-text-muted);
}

.shortcuts-confirm-close {
  height: 30px;
  padding: 0 14px;
  font-size: 13px;
}
</style>
