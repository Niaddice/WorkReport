<script setup lang="ts">
import HolidaySettings from '../HolidaySettings.vue'
import ModuleSettingsSection from './ModuleSettingsSection.vue'
import { useModuleSettingsContext } from '../../modules/settingsContext'

/**
 * 日历模块设置：放假与调休同步。
 *
 * 放假信息属于应用级数据而不是工作空间设置草稿，同步后立即写入日历；
 * 因此本模块没有需要校验的内容，弹窗只显示“完成”按钮。
 */
const context = useModuleSettingsContext()
const holidayState = context.holidays.state
const holidayYear = context.holidays.currentYear
const holidayLoading = context.holidays.loading
const holidaySyncing = context.holidays.syncing
const holidayError = context.holidays.error
const syncHolidays = context.syncHolidays

const validate = () => true
const focusInvalid = () => {}
defineExpose({ validate, focusInvalid })
</script>

<template>
  <ModuleSettingsSection
    title="放假与调休"
    heading-id="calendar-sync-title"
    description="同步中国大陆法定节假日与调休安排，每年手动更新一次。"
  >
    <HolidaySettings
      :state="holidayState"
      :current-year="holidayYear"
      :loading="holidayLoading"
      :syncing="holidaySyncing"
      :error="holidayError"
      @sync="syncHolidays"
    />
  </ModuleSettingsSection>
</template>
