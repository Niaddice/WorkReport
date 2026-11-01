<script setup lang="ts">
import type { GitDiffLine } from '../lib/gitDiff'
defineProps<{ lines: GitDiffLine[] }>()
</script>

<template>
  <pre
    class="diff-content"
  ><code v-for="(line, index) in lines" :key="index" :class="`diff-${line.kind}`"><span class="diff-line-number" aria-hidden="true">{{ line.oldLine ?? '' }}</span><span class="diff-line-number" aria-hidden="true">{{ line.newLine ?? '' }}</span><span class="diff-line-text">{{ line.text || ' ' }}</span></code></pre>
</template>

<style scoped>
.diff-content {
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
  font-size: calc(12 * var(--font-unit));
  line-height: 1.75;
  margin: 0;
  padding: 8px 0;
  tab-size: 4;
}
.diff-content code {
  display: grid;
  grid-template-columns: 4ch 4ch auto;
  width: max-content;
  min-width: 100%;
  padding-right: 14px;
  font: inherit;
}
.diff-line-number {
  color: var(--skin-text-muted);
  user-select: none;
  text-align: right;
  padding-right: 1ch;
  border-right: 1px solid var(--skin-border);
}
.diff-line-text {
  padding-left: 12px;
}
.diff-add {
  color: var(--skin-success);
  background: var(--skin-success-soft);
}
.diff-remove {
  color: var(--skin-danger);
  background: var(--skin-danger-soft);
}
.diff-hunk {
  color: var(--skin-accent);
  background: var(--skin-accent-soft);
}
.diff-meta {
  color: var(--skin-text-muted);
}
</style>
