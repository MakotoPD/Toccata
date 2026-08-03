<script setup lang="ts">
import type { ReleaseCandidate } from '~/types/disc'

const props = defineProps<{
  candidates: ReleaseCandidate[]
  selectedId: string | null
  /** Tracks the drive actually reported, for spotting the right pressing. */
  discTrackCount: number
}>()

const emit = defineEmits<{ select: [id: string] }>()

const { t } = useI18n()

/** The strongest hint available before the user knows the answer. */
function matchesDisc(candidate: ReleaseCandidate) {
  return candidate.tracks.length === props.discTrackCount
}

function details(candidate: ReleaseCandidate) {
  return [
    candidate.date,
    candidate.country,
    candidate.label,
    candidate.barcode,
    candidate.disambiguation,
    candidate.discTotal
      ? t('metadata.discOf', { number: candidate.discNumber, total: candidate.discTotal })
      : candidate.discNumber > 1
        ? t('metadata.disc', { number: candidate.discNumber })
        : null,
  ].filter(Boolean)
}
</script>

<template>
  <section>
    <p v-if="candidates.length > 1" class="mb-4 text-sm text-etch-400">
      {{ t('metadata.choose') }}
    </p>

    <ul class="flex flex-col gap-2">
      <li v-for="candidate in candidates" :key="candidate.id">
        <button
          type="button"
          :aria-pressed="candidate.id === selectedId"
          class="w-full rounded-xs border border-chassis-700 bg-chassis-900 px-4 py-3 text-left transition-colors hover:border-etch-600 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 aria-pressed:border-brass-500 aria-pressed:bg-brass-500/10"
          @click="emit('select', candidate.id)"
        >
          <span class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span class="text-sm text-etch-100">{{ candidate.title }}</span>
            <span class="text-sm text-etch-400">{{ candidate.artist }}</span>

            <span
              v-if="matchesDisc(candidate)"
              class="ml-auto rounded-xs border border-brass-500/40 px-2 py-0.5 text-[0.625rem] uppercase tracking-[0.14em] text-brass-400"
            >
              {{ t('metadata.trackMatch') }}
            </span>
          </span>

          <span
            class="mt-1.5 flex flex-wrap items-center gap-x-2 text-[0.6875rem] tracking-wide text-etch-600"
          >
            <span class="font-mono">{{ t('source.' + candidate.sourceId) }}</span>
            <span aria-hidden="true">·</span>
            <span class="font-mono tabular-nums">
              {{ t('metadata.trackCount', candidate.tracks.length) }}
            </span>
            <template v-for="detail in details(candidate)" :key="detail">
              <span aria-hidden="true">·</span>
              <span>{{ detail }}</span>
            </template>
          </span>
        </button>
      </li>
    </ul>
  </section>
</template>
