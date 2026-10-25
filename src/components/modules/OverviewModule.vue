<script setup lang="ts">
import { computed } from 'vue'
import dayjs from 'dayjs'
import { PhCircleNotch } from '@phosphor-icons/vue'
import { getChinaDayMeta } from '../../lib/chinaHolidays'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal } = context
const { entries, busy: journalBusy } = journal

/** 统一把日记键（YYYYMMDD）与节假日键（YYYY-MM-DD）归一到同一种比较格式。 */
function dayKey(value: string) {
  return dayjs(value).format('YYYY-MM-DD')
}

const today = computed(() => context.now.value.startOf('day'))
const todayKey = computed(() => today.value.format('YYYY-MM-DD'))
const recordDates = computed(() => new Set(entries.value.map((entry) => dayKey(entry.date))))
const todayEntry = computed(() => entries.value.find((entry) => dayKey(entry.date) === todayKey.value) ?? null)

const monthCount = computed(
  () => entries.value.filter((entry) => dayjs(entry.date).isSame(today.value, 'month')).length,
)
const yearCount = computed(() => entries.value.filter((entry) => dayjs(entry.date).isSame(today.value, 'year')).length)
const weekCount = computed(() => {
  const start = today.value.subtract((today.value.day() + 6) % 7, 'day')
  const end = start.add(6, 'day')
  return entries.value.filter((entry) => {
    const day = dayjs(entry.date).startOf('day')
    return !day.isBefore(start) && !day.isAfter(end)
  }).length
})
const streakDays = computed(() => {
  // 今天有记录就从今天倒数，否则从昨天开始；遇到断档立即停止。
  let cursor = recordDates.value.has(todayKey.value) ? today.value : today.value.subtract(1, 'day')
  let streak = 0
  while (recordDates.value.has(cursor.format('YYYY-MM-DD'))) {
    streak++
    cursor = cursor.subtract(1, 'day')
  }
  return streak
})
const metrics = computed(() => [
  { label: '本月日报', value: monthCount.value },
  { label: '本年日报', value: yearCount.value },
  { label: '本周日报', value: weekCount.value },
  { label: '连续记录天数', value: streakDays.value },
])

const todayState = computed(() => {
  const entry = todayEntry.value
  return entry ? `已记录 · ${entry.format === 'md' ? 'MD' : 'TXT'}` : '待记录'
})
const todayHoliday = computed(() => getChinaDayMeta(todayKey.value))
const holidayLabel = computed(() => {
  const meta = todayHoliday.value
  if (!meta) return ''
  return meta.kind === 'holiday' ? `${meta.name} · 放假` : `${meta.name} · 调休上班`
})

/** 工作空间索引尚未载入时先显示加载态，避免把空列表误报成“没有日报”。 */
const loading = computed(() => journalBusy.value && !entries.value.length)
</script>

<template>
  <div class="overview-module" :aria-busy="loading">
    <div class="overview-hero">
      <span class="overview-today">今日日报</span>
      <span class="overview-today-state" :class="{ 'is-recorded': !!todayEntry }">{{ todayState }}</span>
      <span v-if="todayHoliday" class="overview-holiday">{{ holidayLabel }}</span>
    </div>
    <div v-if="loading" class="overview-loading" role="status" aria-live="polite">
      <PhCircleNotch class="spin" :size="16" aria-hidden="true" />正在读取日报…
    </div>
    <div v-else-if="!entries.length" class="overview-empty">从今天的第一篇日报开始</div>
    <div v-else class="overview-metrics">
      <div v-for="metric in metrics" :key="metric.label" class="overview-metric">
        <strong class="overview-metric-value">{{ metric.value }}</strong>
        <span class="overview-metric-label">{{ metric.label }}</span>
      </div>
    </div>
  </div>
</template>
