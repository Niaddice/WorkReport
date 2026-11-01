<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, useId, watch, type CSSProperties } from 'vue'
import dayjs from 'dayjs'
import { dateKey, rangeDateKeys, weekdays } from '../lib/calendar'
import { api, type Entry } from '../lib/workspace'
import { buttonTooltipPosition } from '../lib/tooltipPosition'

const props = defineProps<{ today: string; entries: Entry[]; workspacePath: string }>()
const scroller = ref<HTMLElement>()
const counts = shallowRef(new Map<string, number | null>())
const loading = ref(false)
const error = ref(false)
const tooltipId = `heatmap-tooltip-${useId()}`
const hoveredDate = ref('')
const tooltipElement = ref<HTMLElement>()
const tooltipStyle = ref<CSSProperties>({ visibility: 'hidden' })
let tooltipRevision = 0
let followLatest = true
let resizeObserver: ResizeObserver | undefined
const end = computed(() => dayjs(props.today).startOf('day'))
const start = computed(() => end.value.subtract(364, 'day'))
const dates = computed(() => rangeDateKeys(dateKey(start.value), dateKey(end.value)))
const recorded = computed(() => new Set(props.entries.map((entry) => dateKey(entry.date))))
const recordedCount = computed(() => dates.value.filter((date) => recorded.value.has(date)).length)
const rangeLabel = computed(() => `${start.value.format('YYYY.MM.DD')} – ${end.value.format('YYYY.MM.DD')}`)
const weeks = computed(() => {
  const first = start.value.subtract((start.value.day() + 6) % 7, 'day')
  const length = Math.ceil((end.value.diff(first, 'day') + 1) / 7)
  return Array.from({ length }, (_, column) => {
    const days = Array.from({ length: 7 }, (_, row) => {
      const day = first.add(column * 7 + row, 'day')
      return day.isBefore(start.value) || day.isAfter(end.value) ? null : dateKey(day)
    })
    const monthStart = days.find((date) => date && dayjs(date).date() === 1)
    return {
      key: dateKey(first.add(column * 7, 'day')),
      days,
      label: monthStart ? dayjs(monthStart).format('M月') : '',
    }
  })
})

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
  const element = scroller.value
  if (element) followLatest = element.scrollWidth - element.clientWidth - element.scrollLeft < 2
}
function scrollToLatest() {
  if (scroller.value) scroller.value.scrollLeft = scroller.value.scrollWidth
}
onMounted(() => {
  window.addEventListener('scroll', hideDayTooltip, true)
  window.addEventListener('resize', hideDayTooltip)
  window.addEventListener('blur', hideDayTooltip)
  window.addEventListener('keydown', onKeydown)
  if (typeof ResizeObserver === 'undefined') return
  resizeObserver = new ResizeObserver(() => {
    hideDayTooltip()
    if (followLatest) scrollToLatest()
  })
  if (scroller.value) resizeObserver.observe(scroller.value)
})
onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  hideDayTooltip()
  window.removeEventListener('scroll', hideDayTooltip, true)
  window.removeEventListener('resize', hideDayTooltip)
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
// Start at the most recent dates when the module is too narrow for the whole year.
watch(
  end,
  async () => {
    hideDayTooltip()
    await nextTick()
    followLatest = true
    scrollToLatest()
  },
  { immediate: true },
)
</script>

<template>
  <section class="daily-heatmap" aria-label="每日热力图" :aria-busy="loading">
    <div class="heatmap-heading">
      <h3>每日热力图</h3>
      <span
        >近 365 天 · 已记录 <strong>{{ recordedCount }}</strong> 天</span
      >
    </div>
    <p class="heatmap-range">{{ rangeLabel }}</p>
    <div
      ref="scroller"
      class="heatmap-scroller"
      tabindex="0"
      aria-label="每日记录热力图，可左右滚动查看"
      @scroll="trackScroll"
    >
      <table class="heatmap-table" :style="{ '--heatmap-weeks': weeks.length }">
        <caption class="sr-only">
          近 365 天每日记录情况，颜色越深表示日报字数越多
        </caption>
        <thead>
          <tr>
            <th class="heatmap-weekday"><span class="sr-only">星期</span></th>
            <th
              v-for="week in weeks"
              :key="week.key"
              scope="col"
              :aria-label="`${dayjs(week.key).format('M月D日')}开始的一周`"
            >
              <span>{{ week.label }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(weekday, row) in weekdays" :key="weekday">
            <th scope="row" class="heatmap-weekday">{{ weekday }}</th>
            <td v-for="week in weeks" :key="week.key">
              <span
                v-if="week.days[row]"
                class="heatmap-day"
                :class="{ 'is-today': week.days[row] === today }"
                :data-date="week.days[row]"
                :data-level="level(week.days[row]!)"
                role="img"
                :aria-label="label(week.days[row]!)"
                :aria-describedby="hoveredDate === week.days[row] ? tooltipId : undefined"
                @mouseenter="showDayTooltip(week.days[row]!, $event)"
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
