<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import dayjs, { type Dayjs } from 'dayjs'
import { PhCaretLeft, PhCaretRight, PhCircle, PhFileText } from '@phosphor-icons/vue'
import {
  dateKey,
  lunarDay,
  monthDays,
  movePeriod,
  rangeDateKeys,
  weekdays,
  weekDays,
  type ViewMode,
} from '../lib/calendar'
import { getChinaDayMeta, isWeekend } from '../lib/chinaHolidays'
import type { Entry } from '../lib/workspace'

const props = defineProps<{
  today: string
  selected: string
  entries: Entry[]
  previews: Record<string, string>
  disabled: boolean
}>()
const emit = defineEmits<{ select: [date: string]; previews: [dates: string[]]; range: [dates: string[]] }>()
const current = ref(dayjs())
const view = ref<ViewMode>('month')
const modes = [
  { value: 'week', label: '周' },
  { value: 'month', label: '月' },
  { value: 'year', label: '年' },
] as const
const selection = ref<string[]>([])
const dragging = ref(false)
const dragAnchor = ref('')
const dragCurrent = ref('')
let suppressClick = false
let suppressTimer: ReturnType<typeof setTimeout> | undefined
const title = computed(() => current.value.format(view.value === 'year' ? 'YYYY 年' : 'YYYY 年 M 月'))
const days = computed(() => (view.value === 'week' ? weekDays(current.value) : monthDays(current.value)))
const months = computed(() => Array.from({ length: 12 }, (_, index) => current.value.startOf('year').month(index)))
const entryMap = computed(() => new Map([...props.entries].reverse().map((entry) => [entry.date, entry])))
const visibleDates = computed(() => (view.value === 'year' ? [] : days.value.map(dateKey)))
const periodKey = computed(
  () =>
    `${view.value}-${view.value === 'year' ? current.value.year() : view.value === 'week' ? dateKey(days.value[0]) : current.value.format('YYYY-MM')}`,
)
const range = computed(() => {
  const dates =
    dragging.value && dragAnchor.value && dragCurrent.value && dragAnchor.value !== dragCurrent.value
      ? rangeDateKeys(dragAnchor.value, dragCurrent.value)
      : selection.value
  return { dates, start: dates[0], end: dates.at(-1) }
})
const rangeSet = computed(() => new Set(range.value.dates))
const meta = (day: Dayjs) => getChinaDayMeta(day.format('YYYY-MM-DD'))
/** 月视图里补位的相邻月份日期只作参考，不参与点击与框选。 */
function isOutsideDate(date: string) {
  return view.value === 'month' && !dayjs(date).isSame(current.value, 'month')
}
function isOutsideDay(day: Dayjs) {
  return isOutsideDate(dateKey(day))
}
function move(step: number) {
  if (props.disabled) return
  clearSelection()
  current.value = movePeriod(current.value, view.value, step)
}
function setView(mode: ViewMode) {
  if (props.disabled || view.value === mode) return
  view.value = mode
}
function goToday() {
  if (props.disabled) return
  clearSelection()
  current.value = dayjs()
  emit('select', dateKey(current.value))
}
function openMonth(month: Dayjs) {
  if (props.disabled) return
  current.value = month
  view.value = 'month'
}
function dayLabel(day: Dayjs) {
  const summary = `${day.format('YYYY年M月D日 dddd')}${entryMap.value.has(dateKey(day)) ? '，已有日报' : '，暂无日报'}`
  return isOutsideDay(day) ? `${summary}，非本月，不可选择` : `${summary}，点击打开或开始记录`
}
function isRangeDate(date: string) {
  return rangeSet.value.has(date)
}
function isSingleSelected(date: string) {
  return range.value.dates.length === 0 && date === props.selected
}
function publishRange() {
  const dates = range.value.dates
  emit('range', dates.length > 1 ? [...dates] : [])
}
function startDrag(date: string, event: PointerEvent) {
  if (props.disabled || event.button !== 0 || isOutsideDate(date)) return
  selection.value = []
  publishRange()
  dragging.value = true
  dragAnchor.value = date
  dragCurrent.value = date
  window.addEventListener('pointerup', finishDrag)
}
function moveDrag(date: string) {
  if (dragging.value && !isOutsideDate(date)) dragCurrent.value = date
}
function finishDrag() {
  window.removeEventListener('pointerup', finishDrag)
  if (!dragging.value) return
  const start = dragAnchor.value
  const end = dragCurrent.value
  dragging.value = false
  if (start && end && start !== end) {
    selection.value = rangeDateKeys(start, end)
    suppressClick = true
    clearTimeout(suppressTimer)
    suppressTimer = setTimeout(() => {
      suppressClick = false
    }, 0)
  }
  dragAnchor.value = ''
  dragCurrent.value = ''
  publishRange()
}
function onDayClick(date: string) {
  if (props.disabled || isOutsideDate(date)) return
  if (suppressClick) {
    suppressClick = false
    return
  }
  selection.value = []
  publishRange()
  emit('select', date)
}
function clearSelection() {
  selection.value = []
  publishRange()
}
function onGlobalKeyDown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || props.disabled) return
  // 取消模块拖放由模块板处理，这里不要顺带清空日历框选。
  if (document.body.classList.contains('is-module-dragging')) return
  if (dragging.value) finishDrag()
  clearSelection()
}
onMounted(() => {
  window.addEventListener('keydown', onGlobalKeyDown)
})
defineExpose({ clearRange: clearSelection })
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKeyDown)
  window.removeEventListener('pointerup', finishDrag)
  clearTimeout(suppressTimer)
})
watch(
  () => props.selected,
  (value) => {
    current.value = dayjs(value)
    if (!dragging.value) clearSelection()
  },
)
watch(
  [visibleDates, () => props.entries],
  () => {
    if (visibleDates.value.length) emit('previews', visibleDates.value)
  },
  { immediate: true },
)
</script>

<template>
  <section class="calendar-panel" aria-label="工作日历">
    <div class="calendar-toolbar">
      <div class="period-heading">
        <Transition name="inline-fade" mode="out-in"
          ><h2 :key="title">{{ title }}</h2></Transition
        >
        <div class="period-navigation">
          <md-icon-button aria-label="上一时段" :disabled="disabled" @click="move(-1)"
            ><PhCaretLeft :size="19"
          /></md-icon-button>
          <md-icon-button aria-label="下一时段" :disabled="disabled" @click="move(1)"
            ><PhCaretRight :size="19"
          /></md-icon-button>
        </div>
      </div>
      <div class="view-controls">
        <md-outlined-button class="today-button" :disabled="disabled" @click="goToday">今天</md-outlined-button>
        <div
          class="view-switch"
          :style="{ '--view-index': modes.findIndex((mode) => mode.value === view) }"
          role="group"
          aria-label="日历视图"
        >
          <button
            v-for="mode in modes"
            :key="mode.value"
            :disabled="disabled"
            :aria-pressed="view === mode.value"
            :class="{ active: view === mode.value }"
            @click="setView(mode.value)"
          >
            {{ mode.label }}
          </button>
        </div>
        <Transition name="inline-fade">
          <span v-if="range.dates.length" class="calendar-selection-status" aria-live="polite"
            >已选择 {{ range.dates.length }} 天</span
          >
        </Transition>
      </div>
    </div>

    <div class="calendar-stage">
      <Transition name="calendar-period" mode="out-in">
        <div
          v-if="view !== 'year'"
          :key="periodKey"
          class="calendar-surface"
          :class="{ 'week-view': view === 'week', 'six-week-month': days.length === 42 }"
        >
          <div class="weekday-row">
            <span v-for="(name, index) in weekdays" :key="name" :class="{ weekend: index > 4 }"
              ><span class="weekday-full">周{{ name }}</span
              ><span class="weekday-short">{{ name }}</span></span
            >
          </div>
          <div class="month-grid" :style="{ '--week-count': days.length / 7 }">
            <button
              v-for="day in days"
              :key="dateKey(day)"
              class="day-cell"
              :class="{
                outside: !day.isSame(current, 'month'),
                'is-today': dateKey(day) === today,
                selected: isSingleSelected(dateKey(day)),
                'range-selected': isRangeDate(dateKey(day)),
                'range-start': range.start === dateKey(day),
                'range-end': range.end === dateKey(day),
                weekend: isWeekend(day.format('YYYY-MM-DD')),
                'has-entry': entryMap.has(dateKey(day)),
              }"
              :disabled="disabled || isOutsideDay(day)"
              :aria-label="dayLabel(day)"
              :aria-pressed="isSingleSelected(dateKey(day)) || isRangeDate(dateKey(day))"
              @pointerdown="startDrag(dateKey(day), $event)"
              @pointerenter="moveDrag(dateKey(day))"
              @click="onDayClick(dateKey(day))"
            >
              <div class="day-top">
                <span class="day-number">{{ day.date() }}</span
                ><span class="lunar-label">{{ lunarDay(day) }}</span
                ><span class="day-indicators"
                  ><span v-if="entryMap.has(dateKey(day))" class="entry-dot" aria-hidden="true"></span
                  ><span v-if="meta(day)" class="holiday-label" :class="meta(day)?.kind">{{
                    meta(day)?.kind === 'workday' ? '班' : '休'
                  }}</span></span
                >
              </div>
              <span v-if="meta(day)" class="lunar-label" :class="{ festival: meta(day)?.kind === 'holiday' }">{{
                meta(day)?.name
              }}</span>
              <span v-if="entryMap.has(dateKey(day))" class="entry-preview"
                ><PhFileText :size="13" weight="fill" /><span>{{ previews[dateKey(day)] ?? '加载中…' }}</span></span
              >
            </button>
          </div>
        </div>

        <div v-else :key="periodKey" class="year-grid">
          <section v-for="month in months" :key="month.month()" class="mini-month">
            <button class="mini-title" :disabled="disabled" @click="openMonth(month)">
              {{ month.format('M 月') }}
            </button>
            <div class="mini-weekdays">
              <span v-for="name in weekdays" :key="name">{{ name }}</span>
            </div>
            <div class="mini-days">
              <button
                v-for="day in monthDays(month)"
                :key="dateKey(day)"
                :disabled="disabled"
                :aria-label="dayLabel(day)"
                :aria-pressed="isSingleSelected(dateKey(day)) || isRangeDate(dateKey(day))"
                :class="{
                  outside: !day.isSame(month, 'month'),
                  selected: isSingleSelected(dateKey(day)),
                  'range-selected': isRangeDate(dateKey(day)),
                  'range-start': range.start === dateKey(day),
                  'range-end': range.end === dateKey(day),
                  'is-today': dateKey(day) === today,
                  'has-entry': entryMap.has(dateKey(day)),
                  weekend: isWeekend(day.format('YYYY-MM-DD')),
                }"
                @pointerdown="startDrag(dateKey(day), $event)"
                @pointerenter="moveDrag(dateKey(day))"
                @click="onDayClick(dateKey(day))"
              >
                {{ day.date() }}
              </button>
            </div>
          </section>
        </div>
      </Transition>
    </div>
    <footer class="calendar-footer">
      <span><PhCircle :size="8" weight="fill" />已记录日报</span
      ><span class="calendar-footer-hint">点击打开 · 拖拽框选</span>
    </footer>
  </section>
</template>
