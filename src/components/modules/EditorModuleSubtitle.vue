<script setup lang="ts">
import { computed } from 'vue'
import dayjs from 'dayjs'
import { useModuleContext } from '../../modules/context'

const { journal, rangeDates } = useModuleContext()
const weekday = computed(() => dayjs(journal.selectedDate.value).format('dddd'))
const date = computed(() => dayjs(journal.selectedDate.value).format('M 月 D 日'))
</script>

<template>
  <span v-if="!rangeDates.length" class="module-panel-subtitle" aria-hidden="true">
    <span>{{ weekday }}</span
    ><span>{{ date }}</span>
  </span>
</template>
