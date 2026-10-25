<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import CalendarPane from '../CalendarPane.vue'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal, actions, disabled, today } = context
const { entries, previews, selectedDate, loadPreviews } = journal
const calendar = ref<InstanceType<typeof CalendarPane>>()

function clearRange() {
  calendar.value?.clearRange()
}

onMounted(() => {
  context.calendar.value = { clearRange }
})
onBeforeUnmount(() => {
  context.calendar.value = null
})
</script>

<template>
  <CalendarPane
    ref="calendar"
    :today="today"
    :selected="selectedDate"
    :entries="entries"
    :previews="previews"
    :disabled="disabled"
    @select="actions.selectDate"
    @previews="loadPreviews"
    @range="actions.loadRange"
  />
</template>
