<script setup lang="ts">
import { useId } from 'vue'
import SettingHint from '../SettingHint.vue'

defineProps<{ title: string; description?: string; headingId?: string }>()
const generatedId = useId()
</script>

<template>
  <section class="module-settings-section" :aria-labelledby="headingId || generatedId">
    <div class="settings-section-heading">
      <div class="settings-section-title-wrap">
        <h3 :id="headingId || generatedId">{{ title }}</h3>
        <SettingHint v-if="description" :text="description" />
      </div>
    </div>
    <div class="module-settings-section-content"><slot /></div>
  </section>
</template>
