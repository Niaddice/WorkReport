<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import {
  PhArrowElbowDownLeft,
  PhCalendarDots,
  PhCircleNotch,
  PhFileText,
  PhMagnifyingGlass,
  PhX,
} from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import { api, type Entry, type SearchResultItem } from '../lib/workspace'

const props = defineProps<{
  workspacePath?: string
  entries: Entry[]
  isMac?: boolean
}>()

const emit = defineEmits<{
  selectJournal: [date: string]
  selectFile: [path: string]
  close: []
}>()

type FilterKind = 'all' | 'journal' | 'file'

const dialog = ref<HTMLDialogElement>()
const searchInput = ref<HTMLInputElement>()
const listRef = ref<HTMLElement>()
const motion = createDialogMotion(() => dialog.value)

const query = ref('')
const filter = ref<FilterKind>('all')
const results = ref<SearchResultItem[]>([])
const loading = ref(false)
const error = ref('')
const activeIndex = ref(0)

let debounceTimer: ReturnType<typeof setTimeout> | undefined
let searchRevision = 0

function formatDateTitle(date: string): string {
  if (date.length === 8) {
    return `${date.slice(0, 4)}年${date.slice(4, 6)}月${date.slice(6, 8)}日`
  }
  return date
}

const recentItems = computed<SearchResultItem[]>(() =>
  [...props.entries]
    .sort((left, right) => right.date.localeCompare(left.date))
    .slice(0, 12)
    .map((entry) => ({
      kind: 'journal' as const,
      date: entry.date,
      path: entry.filename,
      title: formatDateTitle(entry.date),
      format: entry.format,
      snippet: '最近日报记录',
      matchCount: 0,
    })),
)

const isSearching = computed(() => query.value.trim().length > 0)

const baseItems = computed<SearchResultItem[]>(() => (isSearching.value ? results.value : recentItems.value))

const counts = computed(() => {
  const all = baseItems.value.length
  let journal = 0
  let file = 0
  for (const item of baseItems.value) {
    if (item.kind === 'journal') journal++
    else file++
  }
  return { all, journal, file }
})

const filteredItems = computed<SearchResultItem[]>(() => {
  if (filter.value === 'all') return baseItems.value
  return baseItems.value.filter((item) => item.kind === filter.value)
})

const searchTerms = computed<string[]>(() =>
  query.value
    .trim()
    .split(/\s+/)
    .map((term) => term.trim())
    .filter(Boolean),
)

interface HighlightPart {
  text: string
  match: boolean
}

function highlightText(text: string): HighlightPart[] {
  const terms = searchTerms.value
  if (!terms.length || !text) return [{ text, match: false }]
  const escaped = terms.map((term) => term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')
  const regex = new RegExp(`(${escaped})`, 'gi')
  const parts: HighlightPart[] = []
  let lastIndex = 0
  for (const match of text.matchAll(regex)) {
    const index = match.index ?? 0
    if (index > lastIndex) {
      parts.push({ text: text.slice(lastIndex, index), match: false })
    }
    parts.push({ text: match[0], match: true })
    lastIndex = index + match[0].length
  }
  if (lastIndex < text.length) {
    parts.push({ text: text.slice(lastIndex), match: false })
  }
  return parts.length ? parts : [{ text, match: false }]
}

async function runSearch(rawQuery: string) {
  const trimmed = rawQuery.trim()
  const workspacePath = props.workspacePath
  const revision = ++searchRevision
  if (!trimmed || !workspacePath) {
    results.value = []
    loading.value = false
    error.value = ''
    activeIndex.value = 0
    return
  }
  loading.value = true
  error.value = ''
  try {
    const found = await api.search(workspacePath, trimmed, 50)
    if (revision !== searchRevision) return
    results.value = found
    activeIndex.value = 0
  } catch (cause) {
    if (revision !== searchRevision) return
    error.value = String(cause)
    results.value = []
  } finally {
    if (revision === searchRevision) loading.value = false
  }
}

watch(query, (value) => {
  clearTimeout(debounceTimer)
  if (!value.trim()) {
    searchRevision++
    results.value = []
    loading.value = false
    error.value = ''
    activeIndex.value = 0
    return
  }
  debounceTimer = setTimeout(() => {
    void runSearch(value)
  }, 120)
})

watch(filter, () => {
  activeIndex.value = 0
})

watch(
  () => props.workspacePath,
  () => {
    query.value = ''
    results.value = []
    error.value = ''
    loading.value = false
    activeIndex.value = 0
  },
)

function scrollActiveIntoView() {
  void nextTick(() => {
    const activeEl = listRef.value?.querySelector<HTMLElement>(`[data-search-index="${activeIndex.value}"]`)
    if (activeEl && typeof activeEl.scrollIntoView === 'function') {
      activeEl.scrollIntoView({ block: 'nearest' })
    }
  })
}

function chooseItem(item: SearchResultItem | undefined) {
  if (!item) return
  if (item.kind === 'journal' && item.date) {
    emit('selectJournal', item.date)
  } else {
    emit('selectFile', item.path)
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.isComposing || event.keyCode === 229) return
  if (event.key === 'Escape') {
    event.preventDefault()
    emit('close')
    return
  }
  const total = filteredItems.value.length
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    if (total > 0) {
      activeIndex.value = (activeIndex.value + 1) % total
      scrollActiveIntoView()
    }
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    if (total > 0) {
      activeIndex.value = (activeIndex.value - 1 + total) % total
      scrollActiveIntoView()
    }
  } else if (event.key === 'Enter') {
    if (total > 0) {
      event.preventDefault()
      chooseItem(filteredItems.value[activeIndex.value])
    }
  }
}

function clearQuery() {
  query.value = ''
  searchInput.value?.focus()
}

function open() {
  activeIndex.value = 0
  error.value = ''
  motion.open()
  void nextTick(() => {
    searchInput.value?.focus()
    searchInput.value?.select()
  })
}

function close() {
  clearTimeout(debounceTimer)
  return motion.close()
}

function isOpen() {
  return !!dialog.value?.open
}

defineExpose({ open, close, isOpen })

onBeforeUnmount(() => {
  clearTimeout(debounceTimer)
  motion.dispose()
})
</script>

<template>
  <dialog
    ref="dialog"
    class="global-search-dialog confirmation-dialog"
    aria-labelledby="global-search-dialog-title"
    @cancel.prevent="emit('close')"
    @click.self="emit('close')"
  >
    <div class="search-dialog-shell" @keydown="onKeydown">
      <header class="search-dialog-header">
        <div class="search-input-row">
          <PhMagnifyingGlass class="search-input-icon" :size="18" aria-hidden="true" />
          <input
            id="global-search-dialog-title"
            ref="searchInput"
            v-model="query"
            type="search"
            class="search-input"
            placeholder="搜索日报内容、日期（如 2026-10-30）或文件名…"
            aria-label="全局全文检索"
            autocomplete="off"
            spellcheck="false"
          />
          <PhCircleNotch v-if="loading" class="spin search-status-icon" :size="16" aria-hidden="true" />
          <button
            v-else-if="query"
            type="button"
            class="search-clear-btn"
            aria-label="清空搜索词"
            title="清空搜索词"
            @click="clearQuery"
          >
            <PhX :size="14" aria-hidden="true" />
          </button>
          <kbd class="search-shortcut-badge" aria-hidden="true">{{ isMac ? '⌘K' : 'Ctrl+K' }}</kbd>
        </div>
        <div class="search-filter-bar" role="tablist" aria-label="检索范围筛选">
          <button
            type="button"
            role="tab"
            class="search-filter-pill"
            :class="{ 'is-active': filter === 'all' }"
            :aria-selected="filter === 'all'"
            @click="filter = 'all'"
          >
            <span>全部</span>
            <span class="search-filter-count">{{ counts.all }}</span>
          </button>
          <button
            type="button"
            role="tab"
            class="search-filter-pill"
            :class="{ 'is-active': filter === 'journal' }"
            :aria-selected="filter === 'journal'"
            @click="filter = 'journal'"
          >
            <span>日报</span>
            <span class="search-filter-count">{{ counts.journal }}</span>
          </button>
          <button
            type="button"
            role="tab"
            class="search-filter-pill"
            :class="{ 'is-active': filter === 'file' }"
            :aria-selected="filter === 'file'"
            @click="filter = 'file'"
          >
            <span>文件</span>
            <span class="search-filter-count">{{ counts.file }}</span>
          </button>
          <span class="search-section-hint">{{ isSearching ? '搜索结果' : '最近日报' }}</span>
        </div>
      </header>

      <div ref="listRef" class="search-results-body" role="listbox" aria-label="检索结果列表">
        <p v-if="error" class="search-empty-state is-error" role="alert">{{ error }}</p>
        <p v-else-if="!filteredItems.length && !loading" class="search-empty-state">
          {{ isSearching ? '没有找到匹配的日报或文件' : '当前工作空间还没有可展示的记录' }}
        </p>
        <ul v-else class="search-result-list">
          <li v-for="(item, index) in filteredItems" :key="`${item.kind}:${item.date ?? item.path}`">
            <button
              type="button"
              role="option"
              class="search-result-item"
              :class="{ 'is-active': index === activeIndex }"
              :aria-selected="index === activeIndex"
              :data-search-index="index"
              @mouseenter="activeIndex = index"
              @click="chooseItem(item)"
            >
              <span class="search-item-icon" :class="`is-${item.kind}`" aria-hidden="true">
                <PhCalendarDots v-if="item.kind === 'journal'" :size="18" />
                <PhFileText v-else :size="18" />
              </span>
              <span class="search-item-main">
                <span class="search-item-title-row">
                  <strong class="search-item-title">
                    <template v-for="(part, partIndex) in highlightText(item.title)" :key="partIndex">
                      <mark v-if="part.match" class="search-highlight">{{ part.text }}</mark>
                      <template v-else>{{ part.text }}</template>
                    </template>
                  </strong>
                  <span class="search-item-tag">{{ item.kind === 'journal' ? '日报' : '文件' }}</span>
                  <span class="search-item-path" :title="item.path">{{ item.path }}</span>
                  <span v-if="item.matchCount > 0" class="search-item-matches">{{ item.matchCount }} 处匹配</span>
                </span>
                <span class="search-item-snippet">
                  <template v-for="(part, partIndex) in highlightText(item.snippet)" :key="partIndex">
                    <mark v-if="part.match" class="search-highlight">{{ part.text }}</mark>
                    <template v-else>{{ part.text }}</template>
                  </template>
                </span>
              </span>
              <PhArrowElbowDownLeft class="search-item-enter" :size="15" aria-hidden="true" />
            </button>
          </li>
        </ul>
      </div>

      <footer class="search-dialog-footer">
        <div class="search-footer-hints">
          <span><kbd>↑</kbd><kbd>↓</kbd> 切换</span>
          <span><kbd>Enter</kbd> 跳转</span>
          <span><kbd>Esc</kbd> 关闭</span>
        </div>
        <button type="button" class="text-action" data-window-close="default" @click="emit('close')">关闭</button>
      </footer>
    </div>
  </dialog>
</template>

<style scoped>
.global-search-dialog {
  width: min(640px, calc(100vw - 32px));
  max-width: min(640px, calc(100vw - 32px));
  max-height: min(560px, calc(100vh - 64px));
  padding: 0;
  border: 1px solid var(--skin-border);
  border-radius: 14px;
  background: var(--skin-surface);
  color: var(--skin-text);
  box-shadow: 0 20px 48px rgba(0, 0, 0, 0.28);
  overflow: hidden;
}

.search-dialog-shell {
  display: flex;
  flex-direction: column;
  max-height: min(560px, calc(100vh - 64px));
}

.search-dialog-header {
  padding: 14px 16px 10px;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-surface);
  display: grid;
  gap: 10px;
}

.search-input-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: 1px solid var(--skin-border);
  border-radius: 10px;
  background: var(--skin-canvas);
  transition: border-color 140ms ease;
}

.search-input-row:focus-within {
  border-color: var(--skin-accent);
}

.search-input-icon {
  color: var(--skin-text-muted);
  flex-shrink: 0;
}

.search-input {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  color: var(--skin-text);
  font: inherit;
  font-size: var(--text-body);
  outline: none;
}

.search-input::-webkit-search-cancel-button {
  display: none;
}

.search-status-icon {
  color: var(--skin-accent);
  flex-shrink: 0;
}

.search-clear-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
}

.search-clear-btn:hover {
  background: var(--skin-accent-soft);
  color: var(--skin-text);
}

.search-shortcut-badge {
  padding: 2px 6px;
  border: 1px solid var(--skin-border);
  border-radius: 5px;
  background: var(--skin-surface);
  color: var(--skin-text-muted);
  font-family: inherit;
  font-size: var(--text-micro);
  line-height: 1.3;
  flex-shrink: 0;
}

.search-filter-bar {
  display: flex;
  align-items: center;
  gap: 6px;
}

.search-filter-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px;
  border: 1px solid var(--skin-border);
  border-radius: 999px;
  background: transparent;
  color: var(--skin-text-muted);
  font: inherit;
  font-size: var(--text-small);
  cursor: pointer;
  transition:
    background-color 120ms ease,
    border-color 120ms ease,
    color 120ms ease;
}

.search-filter-pill:hover {
  border-color: var(--skin-accent);
  color: var(--skin-text);
}

.search-filter-pill.is-active {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  font-weight: 600;
}

.search-filter-count {
  font-size: var(--text-caption);
  opacity: 0.85;
}

.search-section-hint {
  margin-left: auto;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
}

.search-results-body {
  flex: 1;
  min-height: 180px;
  max-height: 370px;
  overflow-y: auto;
  padding: 8px;
}

.search-empty-state {
  margin: 0;
  padding: 42px 16px;
  text-align: center;
  color: var(--skin-text-muted);
  font-size: var(--text-body);
}

.search-empty-state.is-error {
  color: var(--skin-danger, #c62828);
}

.search-result-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 4px;
}

.search-result-item {
  width: 100%;
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: transparent;
  color: var(--skin-text);
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition:
    background-color 100ms ease,
    border-color 100ms ease;
}

.search-result-item:hover,
.search-result-item.is-active {
  background: var(--skin-accent-soft);
  border-color: color-mix(in srgb, var(--skin-accent) 32%, transparent);
}

.search-item-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 8px;
  background: var(--skin-canvas);
  color: var(--skin-accent);
  flex-shrink: 0;
  margin-top: 1px;
}

.search-item-main {
  flex: 1;
  min-width: 0;
  display: grid;
  gap: 4px;
}

.search-item-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.search-item-title {
  font-weight: 600;
  font-size: var(--text-body);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.search-item-tag {
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--skin-canvas);
  color: var(--skin-text-muted);
  font-size: var(--text-caption);
  flex-shrink: 0;
}

.search-item-path {
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.search-item-matches {
  margin-left: auto;
  color: var(--skin-accent);
  font-size: var(--text-caption);
  flex-shrink: 0;
}

.search-item-snippet {
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: break-word;
}

.search-highlight {
  padding: 0 2px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--skin-accent) 24%, transparent);
  color: var(--skin-text);
  font-weight: 600;
}

.search-item-enter {
  color: var(--skin-text-muted);
  opacity: 0;
  flex-shrink: 0;
  align-self: center;
  transition: opacity 100ms ease;
}

.search-result-item.is-active .search-item-enter {
  opacity: 0.85;
}

.search-dialog-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  border-top: 1px solid var(--skin-border);
  background: var(--skin-canvas);
}

.search-footer-hints {
  display: flex;
  align-items: center;
  gap: 14px;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
}

.search-footer-hints kbd {
  display: inline-block;
  padding: 1px 5px;
  margin-right: 3px;
  border: 1px solid var(--skin-border);
  border-radius: 4px;
  background: var(--skin-surface);
  font-family: inherit;
  font-size: var(--text-micro);
}
</style>
