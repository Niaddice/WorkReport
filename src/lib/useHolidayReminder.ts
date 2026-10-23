import { computed, ref, type Ref } from 'vue'
import { shouldRemindHolidaySync, type HolidayState } from './holidays'

export function useHolidayReminder(state: Ref<HolidayState>, localDate: Ref<string>, ready: Ref<boolean>) {
  const dismissedDate = ref('')
  const visible = computed(() => ready.value && dismissedDate.value !== localDate.value
    && shouldRemindHolidaySync(state.value, localDate.value))

  function dismiss() { dismissedDate.value = localDate.value }

  return { visible, dismiss }
}
