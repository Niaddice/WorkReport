<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, useId, watch, type CSSProperties } from 'vue'
import dayjs from 'dayjs'
import { dateKey, rangeDateKeys } from '../lib/calendar'
import { heatmapGrid, type HeatmapGrid } from '../lib/heatmapLayout'
import { api, type Entry } from '../lib/workspace'
import { buttonTooltipPosition } from '../lib/tooltipPosition'

const props = defineProps<{ today: string; entries: Entry[]; workspacePath: string }>()
const scroller = ref<HTMLElement>()
const viewport = ref({ width: 0, height: 0, boxWidth: 0, boxHeight: 0 })
const counts = shallowRef(new Map<string, number | null>())
const loading = ref(false)
const error = ref(false)
const tooltipId = `heatmap-tooltip-${useId()}`
const hoveredDate = ref('')
const tooltipElement = ref<HTMLElement>()
const tooltipStyle = ref<CSSProperties>({ visibility: 'hidden' })
let tooltipRevision = 0
let resizeObserver: ResizeObserver | undefined
const end = computed(() => dayjs(props.today).startOf('day'))
const start = computed(() => end.value.subtract(364, 'day'))
const dates = computed(() => rangeDateKeys(dateKey(start.value), dateKey(end.value)))
const recorded = computed(() => new Set(props.entries.map((entry) => dateKey(entry.date))))
const layouts = computed(() => [365, 90, 30, 7, 1].map((days) => heatmapGrid(dateKey(end.value), days)))
const cellGap = computed(() => {
  // Use the allocated box so appearing scrollbars cannot toggle density breakpoints.
  const { boxWidth: width, boxHeight: height } = viewport.value
  if (height > 0 && height < 110) return 1
  return (width > 0 && width < 480) || (height > 0 && height < 130) ? 2 : 3
})
const headingHeight = computed(() => (viewport.value.boxHeight > 0 && viewport.value.boxHeight < 110 ? 12 : 18))
function fittingCellSize(grid: HeatmapGrid) {
  const { width, height } = viewport.value
  const gap = cellGap.value
  const columns = grid.columns.length
  const rows = grid.rows.length
  const labels = grid.rowLabels ? 18 : 0
  const header = grid.days === 1 ? 0 : headingHeight.value
  const byWidth = width > 0 ? (width - 2 - labels - (columns + (grid.rowLabels ? 2 : 1)) * gap) / columns : 16
  const byHeight = height > 0 ? (height - 8 - header - (rows + 3) * gap) / rows : 16
  return Math.floor(Math.min(byWidth, byHeight))
}
const grid = computed(() => layouts.value.find((layout) => fittingCellSize(layout) >= 10) ?? layouts.value.at(-1)!)
const cellSize = computed(() => Math.max(8, Math.min(16, fittingCellSize(grid.value))))
const monthHeight = computed(() => (grid.value.days === 1 ? 0 : headingHeight.value))
const visibleDates = computed(() => grid.value.rows.flatMap((row) => row.days.filter((date): date is string => !!date)))
const recordedCount = computed(() => visibleDates.value.filter((date) => recorded.value.has(date)).length)
const periodLabel = computed(() => (grid.value.days === 1 ? '今天' : `近 ${grid.value.days} 天`))
const rangeLabel = computed(() =>
  grid.value.days === 1
    ? end.value.format('YYYY.MM.DD')
    : `${dayjs(grid.value.start).format('YYYY.MM.DD')} – ${end.value.format('YYYY.MM.DD')}`,
)

function level(date: string) {
  if (!recorded.value.has(date)) return 0
  const count = counts.value.get(date) ?? 0
  return count >= 1000 ? 4 : count >= 500 ? 3 : count >= 200 ? 2 : 1
}
function label(date: string) {
  const prefix = `${dayjs(date).format('YYYY年M月D日 dddd')}${date === props.today ? '，今天' : ''}`
  if (!recorded.value.has(date)) return `${prefix}，未记录`
  const count = counts.value.get(date)
  const detail =
    typeof count === 'number' ? `${count.toLocaleString()} 字` : loading.value ? '字数统计中' : '字数暂不可用'
  return `${prefix}，已记录，${detail}`
}
function hideDayTooltip() {
  tooltipRevision++
  hoveredDate.value = ''
}
async function showDayTooltip(date: string, event: MouseEvent) {
  const anchor = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const revision = ++tooltipRevision
  hoveredDate.value = date
  tooltipStyle.value = { visibility: 'hidden' }
  await nextTick()
  const tooltip = tooltipElement.value
  if (!tooltip || revision !== tooltipRevision) return
  const position = buttonTooltipPosition(
    anchor,
    { width: tooltip.offsetWidth, height: tooltip.offsetHeight },
    { width: window.innerWidth, height: window.innerHeight },
  )
  tooltipStyle.value = { left: `${position.left}px`, top: `${position.top}px`, visibility: 'visible' }
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') hideDayTooltip()
}
function trackScroll() {
  hideDayTooltip()
  ensureTodayVisible()
}
function keepVisible(container: HTMLElement, day: HTMLElement) {
  if (!container.clientWidth || !container.clientHeight) return
  const bounds = container.getBoundingClientRect()
  const cell = day.getBoundingClientRect()
  const scaleX = bounds.width / container.offsetWidth || 1
  const scaleY = bounds.height / container.offsetHeight || 1
  const left = bounds.left + (container.clientLeft + 2) * scaleX
  const top = bounds.top + (container.clientTop + 2) * scaleY
  const right = left + (container.clientWidth - 4) * scaleX
  const bottom = top + (container.clientHeight - 4) * scaleY
  if (cell.left < left) container.scrollLeft += (cell.left - left) / scaleX
  else if (cell.right > right) container.scrollLeft += (cell.right - right) / scaleX
  if (cell.top < top) container.scrollTop += (cell.top - top) / scaleY
  else if (cell.bottom > bottom) container.scrollTop += (cell.bottom - bottom) / scaleY
}
function ensureTodayVisible(includeOverview = false) {
  const element = scroller.value
  const day = element?.querySelector<HTMLElement>(`[data-date="${props.today}"]`)
  if (!element || !day) return
  keepVisible(element, day)
  const overview = includeOverview ? element.closest<HTMLElement>('.overview-content') : null
  if (overview) keepVisible(overview, day)
}
function measureViewport() {
  hideDayTooltip()
  const element = scroller.value
  if (!element) return
  const { clientWidth: width, clientHeight: height, offsetWidth: boxWidth, offsetHeight: boxHeight } = element
  const value = viewport.value
  if (width !== value.width || height !== value.height || boxWidth !== value.boxWidth || boxHeight !== value.boxHeight)
    viewport.value = { width, height, boxWidth, boxHeight }
}
onMounted(() => {
  window.addEventListener('scroll', hideDayTooltip, true)
  window.addEventListener('resize', measureViewport)
  window.addEventListener('blur', hideDayTooltip)
  window.addEventListener('keydown', onKeydown)
  measureViewport()
  if (typeof ResizeObserver === 'undefined') return
  resizeObserver = new ResizeObserver(measureViewport)
  if (scroller.value) resizeObserver.observe(scroller.value)
})
onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  hideDayTooltip()
  window.removeEventListener('scroll', hideDayTooltip, true)
  window.removeEventListener('resize', measureViewport)
  window.removeEventListener('blur', hideDayTooltip)
  window.removeEventListener('keydown', onKeydown)
})

watch(
  [() => props.workspacePath, () => props.entries, dates],
  async ([path, , visibleDates], previous, onCleanup) => {
    let active = true
    onCleanup(() => {
      active = false
    })
    if (path !== previous?.[0]) {
      counts.value = new Map()
      hideDayTooltip()
    }
    error.value = false
    const requested = visibleDates.filter((date) => recorded.value.has(date))
    if (!path || !requested.length) {
      counts.value = new Map()
      loading.value = false
      return
    }
    loading.value = true
    try {
      const activity = await api.dailyActivity(path, requested)
      if (active) counts.value = new Map(activity.map((day) => [day.date, day.wordCount]))
    } catch {
      if (active) {
        counts.value = new Map()
        error.value = true
      }
    } finally {
      if (active) loading.value = false
    }
  },
  { immediate: true },
)
// Re-anchor both axes after resizing or midnight, including a short overview's viewport.
watch(
  [end, grid, cellSize, viewport],
  async () => {
    hideDayTooltip()
    await nextTick()
    ensureTodayVisible(true)
  },
  { immediate: true },
)
</script>

<template>
  <section class="daily-heatmap" aria-label="每日热力图" :aria-busy="loading">
    <div class="heatmap-heading">
      <h3>每日热力图</h3>
      <span
        >{{ periodLabel }} · 已记录 <strong>{{ recordedCount }}</strong> 天</span
      >
    </div>
    <p class="heatmap-range">{{ rangeLabel }}</p>
    <div
      ref="scroller"
      class="heatmap-scroller"
      tabindex="0"
      :aria-label="`${periodLabel}每日记录热力图，当前日始终可见`"
      @scroll="trackScroll"
    >
      <table
        class="heatmap-table"
        :style="{
          '--heatmap-columns': grid.columns.length,
          '--heatmap-row-label-width': grid.rowLabels ? '18px' : '0px',
          '--heatmap-edge-gaps': grid.rowLabels ? 2 : 1,
          '--heatmap-cell-size': `${cellSize}px`,
          '--heatmap-gap': `${cellGap}px`,
          '--heatmap-month-height': `${monthHeight}px`,
        }"
      >
        <caption class="sr-only">
          {{
            periodLabel
          }}每日记录情况，颜色越深表示日报字数越多
        </caption>
        <thead v-if="grid.days !== 1">
          <tr>
            <th v-if="grid.rowLabels" class="heatmap-weekday"><span class="sr-only">星期</span></th>
            <th v-for="column in grid.columns" :key="column.key" scope="col" :aria-label="column.description">
              <span>{{ column.label }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in grid.rows" :key="row.key" :aria-label="row.label">
            <th v-if="grid.rowLabels" scope="row" class="heatmap-weekday">{{ row.label }}</th>
            <td v-for="(date, column) in row.days" :key="grid.columns[column].key">
              <span
                v-if="date"
                class="heatmap-day"
                :class="{ 'is-today': date === today }"
                :data-date="date"
                :data-level="level(date)"
                role="img"
                :aria-label="label(date)"
                :aria-describedby="hoveredDate === date ? tooltipId : undefined"
                @mouseenter="showDayTooltip(date, $event)"
                @mouseleave="hideDayTooltip"
              ></span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="heatmap-footer">
      <span v-if="loading" role="status">正在统计字数…</span>
      <span v-else-if="error" role="status">字数暂不可用，仍显示记录日期</span>
      <span v-else>按日报字数</span>
      <div class="heatmap-legend" aria-label="颜色由浅到深：未记录、少于200字、200至499字、500至999字、1000字及以上">
        <span>少</span>
        <span
          v-for="value in [0, 1, 2, 3, 4]"
          :key="value"
          class="heatmap-day"
          :data-level="value"
          aria-hidden="true"
        ></span>
        <span>多</span>
      </div>
    </div>
    <Teleport v-if="hoveredDate" to="body">
      <div
        :id="tooltipId"
        ref="tooltipElement"
        class="button-tooltip heatmap-tooltip"
        role="tooltip"
        :style="tooltipStyle"
      >
        <span class="button-tooltip-text">{{ label(hoveredDate) }}</span>
      </div>
    </Teleport>
  </section>
</template>
