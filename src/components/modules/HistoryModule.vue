<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { isTauri } from '@tauri-apps/api/core'
import { PhArrowClockwise, PhCheck, PhCircleNotch, PhCopy } from '@phosphor-icons/vue'
import { historyApi, type HistoryVersion } from '../../lib/history'
import { pushMessage } from '../../lib/messages'
import { api } from '../../lib/workspace'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal, rangeDates } = context
const { selectedDate } = journal

const versions = ref<HistoryVersion[]>([])
const listLoading = ref(false)
const listError = ref('')
const refresher = ref(0)
let listRevision = 0

/** 框选了日期范围时按日期分组；单击单个日期时只显示这一天。 */
const rangeMode = computed(() => rangeDates.value.length > 0)
const waitingForRange = computed(() => rangeMode.value && context.rangeLoading.value)
const selectedDates = computed(() =>
  rangeMode.value ? [...new Set(rangeDates.value)].sort().reverse() : [selectedDate.value],
)
const selectedDateSet = computed(() => new Set(selectedDates.value))
const groups = computed(() => {
  const map = new Map<string, HistoryVersion[]>()
  for (const version of versions.value) {
    if (!selectedDateSet.value.has(version.date)) continue
    const list = map.get(version.date)
    if (list) list.push(version)
    else map.set(version.date, [version])
  }
  return selectedDates.value
    .filter((date) => rangeMode.value || map.has(date))
    .map((date) => ({ date, versions: map.get(date) ?? [] }))
})
const rangeLabel = computed(() => {
  const dates = [...selectedDates.value].sort()
  if (dates.length <= 1) return dayjs(dates[0] ?? selectedDate.value).format('YYYY 年 M 月 D 日')
  const first = dayjs(dates[0])
  const last = dayjs(dates[dates.length - 1])
  return `${first.format('YYYY 年 M 月 D 日')} – ${last.format(last.isSame(first, 'year') ? 'M 月 D 日' : 'YYYY 年 M 月 D 日')} · ${dates.length} 天`
})
const contentCache = new Map<string, string>()
const contentReads = new Map<string, Promise<string>>()
let contentRevision = 0
const copyingKey = ref('')
const copiedKey = ref('')
let copiedTimer: ReturnType<typeof setTimeout> | undefined
let hoverTask = 0

interface TooltipState {
  version: HistoryVersion
  content: string
  loading: boolean
  error: string
}
const tooltip = ref<TooltipState | null>(null)
const tooltipPosition = ref({ left: 0, top: 0 })
const tooltipElement = ref<HTMLElement>()
let latestPointer = { x: 0, y: 0 }
let tooltipFrame = 0

function groupLabel(date: string) {
  return dayjs(date).format('YYYY 年 M 月 D 日 dddd')
}
function versionTime(version: HistoryVersion) {
  return dayjs(version.savedAt).format('HH:mm:ss')
}
function versionDate(version: HistoryVersion) {
  return dayjs(version.date).format('YYYY 年 M 月 D 日')
}
function formatBytes(bytes: number) {
  return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`
}
function badgeOf(version: HistoryVersion) {
  return version.format === 'md' ? 'MD' : 'TXT'
}
function itemLabel(version: HistoryVersion) {
  return `预览 ${versionDate(version)} ${versionTime(version)} 保存的 ${badgeOf(version)} 版本`
}
function versionKey(version: HistoryVersion) {
  return `${version.date}/${version.id}`
}
function copyLabel(version: HistoryVersion) {
  return `复制 ${versionDate(version)} ${versionTime(version)} 保存的 ${badgeOf(version)} 版本`
}

function readContent(version: HistoryVersion): Promise<string> {
  const key = versionKey(version)
  const cached = contentCache.get(key)
  if (cached !== undefined) return Promise.resolve(cached)
  const pending = contentReads.get(key)
  if (pending) return pending
  const revision = contentRevision
  const request = historyApi
    .read(version.date, version.id)
    .then((content) => {
      if (revision === contentRevision) contentCache.set(key, content)
      return content
    })
    .finally(() => {
      if (contentReads.get(key) === request) contentReads.delete(key)
    })
  contentReads.set(key, request)
  return request
}

function resetContent() {
  contentRevision++
  contentCache.clear()
  contentReads.clear()
  copyingKey.value = ''
  copiedKey.value = ''
  clearTimeout(copiedTimer)
}

async function copyVersion(version: HistoryVersion) {
  if (copyingKey.value) return
  hideTooltip()
  const key = versionKey(version)
  const revision = contentRevision
  copyingKey.value = key
  copiedKey.value = ''
  clearTimeout(copiedTimer)
  try {
    const content = await readContent(version)
    if (revision !== contentRevision) return
    if (isTauri()) await api.copy(content)
    else await navigator.clipboard.writeText(content)
    if (revision !== contentRevision) return
    copiedKey.value = key
    pushMessage({ tone: 'success', text: '历史版本已复制到剪贴板' })
    copiedTimer = setTimeout(() => {
      copiedKey.value = ''
    }, 1600)
  } catch (cause) {
    if (revision === contentRevision) {
      pushMessage({ tone: 'error', text: `复制历史版本失败：${String(cause)}` })
    }
  } finally {
    if (revision === contentRevision) copyingKey.value = ''
  }
}

async function load() {
  const revision = ++listRevision
  const range = rangeMode.value
  const date = selectedDate.value
  const datesKey = selectedDates.value.join(',')
  listLoading.value = true
  listError.value = ''
  try {
    const values = range ? await historyApi.list() : await historyApi.list(date)
    if (revision !== listRevision || datesKey !== selectedDates.value.join(',')) return
    versions.value = values
  } catch (cause) {
    if (revision === listRevision) listError.value = `读取历史版本失败：${String(cause)}`
  } finally {
    if (revision === listRevision) listLoading.value = false
  }
}

function hideTooltip() {
  hoverTask++
  tooltip.value = null
  latestPointer = { x: 0, y: 0 }
}

function positionTooltip() {
  const element = tooltipElement.value
  if (!element) return
  const margin = 12
  const offset = 16
  const width = element.offsetWidth
  const height = element.offsetHeight
  let left = latestPointer.x + offset
  let top = latestPointer.y + offset
  if (left + width > window.innerWidth - margin) left = latestPointer.x - width - offset
  if (top + height > window.innerHeight - margin) top = latestPointer.y - height - offset
  left = Math.max(margin, Math.min(left, window.innerWidth - width - margin))
  top = Math.max(margin, Math.min(top, window.innerHeight - height - margin))
  tooltipPosition.value = { left, top }
}

function schedulePosition() {
  if (tooltipFrame) return
  tooltipFrame = requestAnimationFrame(() => {
    tooltipFrame = 0
    positionTooltip()
  })
}

async function showTooltip(version: HistoryVersion, clientX: number, clientY: number) {
  latestPointer = { x: clientX, y: clientY }
  const task = ++hoverTask
  tooltip.value = { version, content: '', loading: true, error: '' }
  await nextTick()
  if (task !== hoverTask) return
  schedulePosition()
  try {
    const content = await readContent(version)
    if (task !== hoverTask) return
    tooltip.value = { version, content, loading: false, error: '' }
    await nextTick()
    schedulePosition()
  } catch (cause) {
    if (task !== hoverTask) return
    tooltip.value = { version, content: '', loading: false, error: `读取版本内容失败：${String(cause)}` }
  }
}

function onVersionEnter(version: HistoryVersion, event: MouseEvent) {
  void showTooltip(version, event.clientX, event.clientY)
}
function onVersionMove(event: MouseEvent) {
  if (!tooltip.value) return
  latestPointer = { x: event.clientX, y: event.clientY }
  schedulePosition()
}
function onVersionFocus(version: HistoryVersion, event: FocusEvent) {
  const element = event.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  void showTooltip(version, rect.left + rect.width / 2, rect.top + rect.height / 2)
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && tooltip.value) {
    event.preventDefault()
    hideTooltip()
  }
}

function resetAndReload() {
  versions.value = []
  listError.value = ''
  hideTooltip()
  resetContent()
  void load()
}

watch(
  [rangeMode, selectedDates, selectedDate, waitingForRange, () => journal.workspace.value?.path],
  () => {
    if (rangeMode.value && selectedDates.value.length === 0) return
    // 框选会先保存当前草稿，等合并范围准备完成再读取最新历史版本。
    if (waitingForRange.value) {
      listRevision++
      versions.value = []
      listError.value = ''
      listLoading.value = true
      hideTooltip()
      resetContent()
      return
    }
    resetAndReload()
  },
  { immediate: true },
)
watch(refresher, resetAndReload)
onBeforeUnmount(() => {
  if (tooltipFrame) cancelAnimationFrame(tooltipFrame)
  hideTooltip()
  resetContent()
})
</script>

<template>
  <div class="history-module" :aria-busy="listLoading">
    <div class="history-module-toolbar">
      <span class="history-module-date" :title="rangeLabel">{{ rangeLabel }}</span>
      <button
        type="button"
        class="text-action history-module-refresh"
        :disabled="listLoading"
        aria-label="刷新历史版本"
        title="刷新历史版本"
        @click="refresher++"
      >
        <PhCircleNotch v-if="listLoading" class="spin" :size="14" aria-hidden="true" /><PhArrowClockwise
          v-else
          :size="14"
          aria-hidden="true"
        /><span class="history-module-refresh-label">刷新</span>
      </button>
    </div>

    <p v-if="listError" class="history-module-empty error-text" role="alert">{{ listError }}</p>
    <p v-else-if="listLoading && !versions.length" class="history-module-empty" role="status">
      <PhCircleNotch class="spin" :size="14" aria-hidden="true" />正在读取历史版本…
    </p>
    <p v-else-if="!groups.length" class="history-module-empty">
      {{ rangeMode ? '所选日期范围内还没有历史版本' : '这一天还没有历史版本' }}
    </p>
    <div v-else class="history-module-scroll" @scroll="hideTooltip">
      <section
        v-for="group in groups"
        :key="group.date"
        class="history-module-group"
        :aria-label="groupLabel(group.date)"
      >
        <h3 v-if="rangeMode" class="history-module-group-title">
          <span>{{ groupLabel(group.date) }}</span>
          <span class="history-module-group-count">{{ group.versions.length }} 个版本</span>
        </h3>
        <p v-if="!group.versions.length" class="history-module-group-empty">暂无历史版本</p>
        <div v-for="version in group.versions" :key="versionKey(version)" class="history-module-row">
          <button
            type="button"
            class="history-module-item"
            :aria-label="itemLabel(version)"
            :aria-describedby="tooltip?.version.id === version.id ? 'history-version-tooltip' : undefined"
            @mouseenter="onVersionEnter(version, $event)"
            @mousemove="onVersionMove"
            @mouseleave="hideTooltip"
            @focus="onVersionFocus(version, $event)"
            @blur="hideTooltip"
            @keydown="onKeydown"
          >
            <span class="history-module-time">{{ versionTime(version) }}</span>
            <span class="history-module-format">{{ badgeOf(version) }}</span>
            <span class="history-module-size">{{ formatBytes(version.bytes) }}</span>
          </button>
          <button
            type="button"
            class="history-copy-button"
            :class="{ 'is-copied': copiedKey === versionKey(version) }"
            :aria-label="copyLabel(version)"
            :title="copiedKey === versionKey(version) ? '已复制' : '复制历史版本'"
            :aria-busy="copyingKey === versionKey(version)"
            :disabled="!!copyingKey"
            @click="copyVersion(version)"
          >
            <PhCircleNotch v-if="copyingKey === versionKey(version)" class="spin" :size="14" aria-hidden="true" />
            <PhCheck v-else-if="copiedKey === versionKey(version)" :size="14" aria-hidden="true" />
            <PhCopy v-else :size="14" aria-hidden="true" />
          </button>
        </div>
      </section>
    </div>

    <Teleport to="body">
      <div
        v-if="tooltip"
        id="history-version-tooltip"
        ref="tooltipElement"
        class="history-tooltip"
        role="tooltip"
        :style="{ left: `${tooltipPosition.left}px`, top: `${tooltipPosition.top}px` }"
      >
        <div class="history-tooltip-head">
          <strong>{{ versionDate(tooltip.version) }} {{ versionTime(tooltip.version) }}</strong>
          <span>{{ badgeOf(tooltip.version) }} · {{ formatBytes(tooltip.version.bytes) }}</span>
        </div>
        <p v-if="tooltip.loading" class="history-tooltip-note">正在读取版本内容…</p>
        <p v-else-if="tooltip.error" class="history-tooltip-note history-tooltip-error">{{ tooltip.error }}</p>
        <pre v-else class="history-tooltip-content">{{ tooltip.content || '（这个版本没有内容）' }}</pre>
      </div>
    </Teleport>
  </div>
</template>
