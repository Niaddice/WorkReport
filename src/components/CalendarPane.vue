<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import dayjs, { type Dayjs } from 'dayjs'
import { PhCaretLeft, PhCaretRight, PhCircle, PhFileText } from '@phosphor-icons/vue'
import { dateKey, lunarDay, monthDays, movePeriod, weekdays, weekDays, type ViewMode } from '../lib/calendar'
import { getChinaDayMeta, isWeekend } from '../lib/chinaHolidays'
import type { Entry } from '../lib/workspace'

const props = defineProps<{ selected: string; entries: Entry[]; disabled: boolean }>()
const emit = defineEmits<{ select: [date: string]; create: [date: string] }>()
const current = ref(dayjs())
const view = ref<ViewMode>('month')
const modes = [{ value: 'week', label: '周' }, { value: 'month', label: '月' }, { value: 'year', label: '年' }] as const
const today = ref(dateKey())
const title = computed(() => current.value.format(view.value === 'year' ? 'YYYY 年' : 'YYYY 年 M 月'))
const days = computed(() => view.value === 'week' ? weekDays(current.value) : monthDays(current.value))
const months = computed(() => Array.from({ length: 12 }, (_, index) => current.value.startOf('year').month(index)))
const entryMap = computed(() => new Map([...props.entries].reverse().map((entry) => [entry.date, entry])))
const count = computed(() => props.entries.filter((entry) => dayjs(entry.date).isSame(current.value, view.value === 'year' ? 'year' : view.value === 'week' ? 'week' : 'month')).length)
const meta = (day: Dayjs) => getChinaDayMeta(day.format('YYYY-MM-DD'))
function move(step: number) { current.value = movePeriod(current.value, view.value, step) }
function goToday() {
  current.value = dayjs()
  today.value = dateKey()
  emit('select', today.value)
}
function dayLabel(day: Dayjs) {
  return `${day.format('YYYY年M月D日 dddd')}${entryMap.value.has(dateKey(day)) ? '，已有日报' : '，暂无日报'}，双击或按回车新增或打开`
}
watch(() => props.selected, (value) => { current.value = dayjs(value) })
</script>

<template>
  <section class="calendar-panel" aria-label="工作日历">
    <div class="calendar-toolbar">
      <div class="period-heading">
        <h2>{{ title }}</h2>
        <div class="period-navigation">
          <md-icon-button aria-label="上一时段" @click="move(-1)"><PhCaretLeft :size="19" /></md-icon-button>
          <md-icon-button aria-label="下一时段" @click="move(1)"><PhCaretRight :size="19" /></md-icon-button>
        </div>
      </div>
      <div class="view-controls">
        <md-outlined-button class="today-button" @click="goToday">今天</md-outlined-button>
        <div class="view-switch" role="group" aria-label="日历视图">
          <button v-for="mode in modes" :key="mode.value" :aria-pressed="view === mode.value" :class="{ active: view === mode.value }" @click="view = mode.value">{{ mode.label }}</button>
        </div>
      </div>
    </div>

    <div v-if="view !== 'year'" class="calendar-surface" :class="{ 'week-view': view === 'week', 'six-week-month': days.length === 42 }">
      <div class="weekday-row"><span v-for="(name, index) in weekdays" :key="name" :class="{ weekend: index > 4 }">周{{ name }}</span></div>
      <div class="month-grid" :style="{ '--week-count': days.length / 7 }">
        <button v-for="day in days" :key="dateKey(day)" class="day-cell" :class="{
          outside: !day.isSame(current, 'month'),
          'is-today': dateKey(day) === today,
          selected: dateKey(day) === selected,
          weekend: isWeekend(day.format('YYYY-MM-DD')),
          'has-entry': entryMap.has(dateKey(day)),
        }" :disabled="disabled" :aria-label="dayLabel(day)" :aria-pressed="dateKey(day) === selected" @click="emit('select', dateKey(day))" @dblclick="emit('create', dateKey(day))" @keydown.enter.prevent="emit('create', dateKey(day))">
          <div class="day-top"><span class="day-number">{{ day.date() }}</span><span class="day-indicators"><span v-if="entryMap.has(dateKey(day))" class="entry-dot" aria-hidden="true"></span><span v-if="meta(day)" class="holiday-label" :class="meta(day)?.kind">{{ meta(day)?.kind === 'workday' ? '班' : '休' }}</span></span></div>
          <span class="lunar-label" :class="{ festival: meta(day)?.kind === 'holiday' }">{{ meta(day)?.name ?? lunarDay(day) }}</span>
          <span v-if="entryMap.has(dateKey(day))" class="entry-preview"><PhFileText :size="13" weight="fill" /><span>{{ entryMap.get(dateKey(day))?.preview }}</span></span>
        </button>
      </div>
    </div>

    <div v-else class="year-grid">
      <section v-for="month in months" :key="month.month()" class="mini-month">
        <button class="mini-title" @click="current = month; view = 'month'">{{ month.format('M 月') }}</button>
        <div class="mini-weekdays"><span v-for="name in weekdays" :key="name">{{ name }}</span></div>
        <div class="mini-days">
          <button v-for="day in monthDays(month)" :key="dateKey(day)" :disabled="disabled" :aria-label="dayLabel(day)" :aria-pressed="dateKey(day) === selected" :class="{ outside: !day.isSame(month, 'month'), selected: dateKey(day) === selected, 'is-today': dateKey(day) === today, 'has-entry': entryMap.has(dateKey(day)), weekend: isWeekend(day.format('YYYY-MM-DD')) }" @click="emit('select', dateKey(day))" @dblclick="emit('create', dateKey(day))" @keydown.enter.prevent="emit('create', dateKey(day))">{{ day.date() }}</button>
        </div>
      </section>
    </div>
    <footer class="calendar-footer"><span><PhCircle :size="8" weight="fill" />已记录日报</span><span>{{ view === 'year' ? '本年' : view === 'week' ? '本周' : '本月' }} {{ count }} 篇<span class="footer-divider">·</span>双击日期，开始记录</span></footer>
  </section>
</template>
