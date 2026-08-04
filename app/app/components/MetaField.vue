<script setup lang="ts">
/**
 * One labelled control. The root is `display: contents` so the label and the
 * box become cells of whatever grid the panel lays out, which is what keeps
 * columns lined up across rows of very different field widths.
 */
withDefaults(
  defineProps<{
    label: string
    /** Grid columns the input should stretch across. */
    span?: number
    mono?: boolean
    disabled?: boolean
    placeholder?: string
  }>(),
  { span: 1, mono: false, disabled: false, placeholder: '' },
)

const model = defineModel<string | number | null>()
</script>

<template>
  <label class="contents">
    <span class="truncate text-right text-[0.625rem] uppercase tracking-[0.14em] text-etch-600">
      {{ label }}
    </span>

    <input
      v-model="model"
      type="text"
      :disabled="disabled"
      :placeholder="placeholder"
      :style="span > 1 ? { gridColumn: `span ${span}` } : undefined"
      class="min-w-0 rounded-xs border border-chassis-700 bg-chassis-950 px-2 py-1 text-[0.8125rem] text-etch-100 transition-colors placeholder:text-etch-600 hover:border-chassis-600 focus:border-brass-500 focus-visible:outline-none disabled:text-etch-600"
      :class="mono && 'font-mono tabular-nums'"
    />
  </label>
</template>
