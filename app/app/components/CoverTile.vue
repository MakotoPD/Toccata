<script setup lang="ts">
defineProps<{
  /** Data URI of the artwork in use, if any was found. */
  cover: string | null
  title: string
}>()

const emit = defineEmits<{ choose: [] }>()

const { t } = useI18n()
</script>

<template>
  <section
    class="flex w-52 shrink-0 flex-col items-center gap-2 border-l border-chassis-800 px-4 py-4"
  >
    <button
      type="button"
      class="group relative aspect-square w-full overflow-hidden rounded-xs border border-chassis-700 bg-chassis-950 transition-colors hover:border-brass-500 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500"
      @click="emit('choose')"
    >
      <img
        v-if="cover"
        :src="cover"
        :alt="t('metadata.coverOf', { title })"
        class="size-full object-cover"
      />

      <span
        v-else
        class="grid size-full place-items-center px-4 text-center text-[0.625rem] uppercase tracking-[0.16em] text-etch-600"
      >
        {{ t('cover.none') }}
      </span>

      <span
        class="absolute inset-x-0 bottom-0 bg-chassis-950/85 py-1 text-[0.625rem] uppercase tracking-[0.14em] text-etch-400 opacity-0 transition-opacity group-hover:opacity-100"
      >
        {{ t('cover.choose') }}
      </span>
    </button>
  </section>
</template>
