<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ title: string; description?: string; headingId?: string }>()
const generatedId = useId()
</script>

<template>
  <section class="module-settings-section" :aria-labelledby="headingId || generatedId">
    <div class="settings-section-heading">
      <div>
        <h3 :id="headingId || generatedId">{{ title }}</h3>
        <p v-if="description">{{ description }}</p>
      </div>
    </div>
    <div class="module-settings-section-content"><slot /></div>
  </section>
</template>
