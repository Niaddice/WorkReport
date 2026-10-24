<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhCalendarCheck } from '@phosphor-icons/vue'
import type { HolidayState } from '../lib/holidays'
import AppSelect from './AppSelect.vue'

const props = defineProps<{
  state: HolidayState
  currentYear: number
  loading: boolean
  syncing: boolean
  error: string
}>()
const emit = defineEmits<{ sync: [year: number] }>()
const selectedYear = ref(props.currentYear)
watch(
  () => props.currentYear,
  (year) => {
    selectedYear.value = year
  },
)
const yearOptions = computed(() =>
  [props.currentYear - 1, props.currentYear, props.currentYear + 1]
    .filter((year) => year >= 2007 && year <= 2100)
    .map((year) => ({ value: String(year), label: `${year} 年` })),
)
const selectedValue = computed({
  get: () => String(selectedYear.value),
  set: (value) => {
    selectedYear.value = Number(value)
  },
})
const record = computed(() => props.state.years.find((item) => item.year === selectedYear.value))
const failure = computed(() =>
  props.state.failures.find(
    (item) =>
      item.year === selectedYear.value &&
      (!record.value || Date.parse(item.attemptedAt) >= Date.parse(record.value.syncedAt)),
  ),
)
const daysOff = computed(() => record.value?.days.filter((day) => day.isOffDay).length ?? 0)
const workdays = computed(() => record.value?.days.filter((day) => !day.isOffDay).length ?? 0)
const status = computed(() =>
  props.loading ? '正在读取…' : failure.value ? '上次同步失败' : record.value ? '已同步到日历' : '尚未同步',
)
function formatTime(value: string) {
  return dayjs(value).format('YYYY 年 M 月 D 日 HH:mm')
}
</script>

<template>
  <div class="holiday-settings" :aria-busy="loading || syncing">
    <div class="holiday-sync-controls">
      <label for="holiday-sync-year">同步年份</label>
      <AppSelect id="holiday-sync-year" v-model="selectedValue" :options="yearOptions" :disabled="loading || syncing" />
      <button
        type="button"
        class="settings-save-button holiday-sync-button"
        :disabled="loading || syncing"
        @click="emit('sync', selectedYear)"
      >
        <PhArrowClockwise :size="15" :class="{ 'holiday-sync-spinning': syncing }" aria-hidden="true" />
        <Transition name="button-label" mode="out-in"
          ><span :key="String(syncing)">{{ syncing ? '正在同步…' : '同步放假信息' }}</span></Transition
        >
      </button>
    </div>
    <div
      class="holiday-sync-status"
      :class="{ 'is-success': record && !failure, 'is-error': failure }"
      role="status"
      aria-live="polite"
    >
      <PhCalendarCheck :size="22" aria-hidden="true" />
      <div>
        <strong>{{ selectedYear }} 年 · {{ status }}</strong>
        <p v-if="record">
          {{ daysOff }} 天放假 · {{ workdays }} 天调休上班<span v-if="failure">，日历保留上次成功同步的数据</span>
        </p>
        <p v-else>同步后，日历会显示放假与调休上班安排。</p>
      </div>
    </div>
    <Transition name="status-fade" mode="out-in">
      <p v-if="error" key="error" class="settings-error" role="alert">{{ error }}</p>
      <p v-else-if="failure" :key="failure.attemptedAt" class="settings-error" role="alert">{{ failure.message }}</p>
    </Transition>
    <div class="settings-details-list">
      <div v-if="record">
        <strong>最近成功同步</strong><span>{{ formatTime(record.syncedAt) }}</span>
      </div>
      <div><strong>同步方式</strong><span>手动同步，完成后立即应用到日历，无需再次保存。</span></div>
      <div>
        <strong>年度提醒</strong
        ><span>仅在每年 1 月 1 日检查当年的同步记录；未同步或上次同步失败时提醒，其他日期不提示。</span>
      </div>
      <div>
        <strong>数据范围</strong><span>中国大陆法定节假日及调休上班安排，由 holiday-cn 整理国务院公布的信息。</span>
      </div>
    </div>
  </div>
</template>
