<script setup lang="ts">
defineProps<{
  track: number | null
  position: number
  trackCount: number
  trackShare: number
  discShare: number
}>()

const emit = defineEmits<{ cancel: [] }>()

const { t, locale } = useI18n()

const percent = computed(() => new Intl.NumberFormat(locale.value, { style: 'percent' }))
</script>

<template>
  <section
    class="rounded-xs border border-brass-500/40 bg-chassis-900 px-5 py-4"
    role="status"
    aria-live="polite"
  >
    <div class="flex items-center gap-4">
      <p class="text-[0.6875rem] uppercase tracking-[0.18em] text-brass-400">
        {{ t('rip.running') }}
      </p>

      <p v-if="track !== null" class="text-sm text-etch-400">
        {{ t('rip.track', { number: position, total: trackCount }) }}
      </p>

      <p class="ml-auto font-mono text-sm tabular-nums text-etch-100">
        {{ percent.format(discShare) }}
      </p>

      <button
        type="button"
        class="rounded-xs border border-chassis-700 px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] text-etch-400 transition-colors hover:border-etch-600 hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500"
        @click="emit('cancel')"
      >
        {{ t('rip.cancel') }}
      </button>
    </div>

    <div class="mt-3 h-0.5 w-full bg-chassis-700">
      <div
        class="h-full bg-brass-500 transition-[width]"
        :style="{ width: `${discShare * 100}%` }"
      />
    </div>

    <div class="mt-1 h-px w-full bg-chassis-800">
      <div
        class="h-full bg-etch-600 transition-[width]"
        :style="{ width: `${trackShare * 100}%` }"
      />
    </div>
  </section>
</template>
