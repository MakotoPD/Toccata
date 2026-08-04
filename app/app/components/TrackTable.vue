<script setup lang="ts">
import type { ReleaseCandidate, Toc } from '~/types/disc'

const props = defineProps<{
  toc: Toc
  release: ReleaseCandidate | null
  selected: number | null
  /** Per track, whether it will be extracted. */
  included: (number: number) => boolean
  /** Per track, how far the rip has got, when one is running. */
  status: (number: number) => 'waiting' | 'reading' | 'done' | 'failed' | null
}>()

const emit = defineEmits<{ select: [number]; toggle: [number]; toggleAll: [boolean] }>()

const { t, locale } = useI18n()
const { fromFrames } = useCdTime()

const megabytes = computed(() => new Intl.NumberFormat(locale.value, { maximumFractionDigits: 1 }))

const audible = computed(() => props.toc.tracks.filter((track) => track.audio))
const allIncluded = computed(() => audible.value.every((track) => props.included(track.number)))

function titleOf(number: number) {
  return props.release?.tracks.find((track) => track.number === number)?.title ?? ''
}

function artistOf(number: number) {
  return props.release?.tracks.find((track) => track.number === number)?.artist ?? ''
}

/** What the extracted WAV will weigh, which is exactly the sectors it spans. */
function sizeOf(frames: number) {
  return megabytes.value.format((frames * 2352) / 1024 / 1024)
}

const statusTone: Record<string, string> = {
  waiting: 'text-etch-600',
  reading: 'text-brass-400',
  done: 'text-etch-400',
  failed: 'text-brass-400',
}
</script>

<template>
  <table class="w-full border-collapse text-[0.8125rem]">
    <thead class="sticky top-0 bg-chassis-950">
      <tr
        class="border-b border-chassis-700 text-[0.625rem] uppercase tracking-[0.16em] text-etch-600"
      >
        <th scope="col" class="w-9 py-1.5 pl-6">
          <input
            type="checkbox"
            :checked="allIncluded"
            class="size-3.5 accent-brass-500"
            :aria-label="t('track.includeAll')"
            @change="emit('toggleAll', !allIncluded)"
          />
        </th>
        <th scope="col" class="w-10 py-1.5 text-right font-normal">#</th>
        <th scope="col" class="py-1.5 pl-4 text-left font-normal">{{ t('track.title') }}</th>
        <th scope="col" class="w-56 py-1.5 text-left font-normal">{{ t('editor.artist') }}</th>
        <th scope="col" class="w-20 py-1.5 text-right font-normal">{{ t('track.length') }}</th>
        <th scope="col" class="w-28 py-1.5 pl-4 text-left font-normal">{{ t('track.status') }}</th>
        <th scope="col" class="w-24 py-1.5 pr-6 text-right font-normal">{{ t('track.size') }}</th>
      </tr>
    </thead>

    <tbody>
      <tr
        v-for="track in toc.tracks"
        :key="track.number"
        class="border-b border-chassis-900 transition-colors"
        :class="[
          track.audio ? 'cursor-pointer hover:bg-chassis-900' : 'text-etch-600',
          selected === track.number && 'bg-chassis-900',
        ]"
        @click="track.audio && emit('select', track.number)"
      >
        <td class="py-1.5 pl-6">
          <input
            v-if="track.audio"
            type="checkbox"
            :checked="included(track.number)"
            class="size-3.5 accent-brass-500"
            :aria-label="t('track.include', { number: track.number })"
            @click.stop
            @change="emit('toggle', track.number)"
          />
        </td>

        <td
          class="py-1.5 text-right font-mono tabular-nums"
          :class="selected === track.number ? 'text-brass-400' : 'text-etch-600'"
        >
          {{ String(track.number).padStart(2, '0') }}
        </td>

        <td class="py-1.5 pl-4">
          <span v-if="!track.audio" class="text-etch-600">{{ t('track.data') }}</span>
          <span v-else-if="titleOf(track.number)" class="text-etch-100">
            {{ titleOf(track.number) }}
          </span>
          <span v-else class="text-etch-600">—</span>
        </td>

        <td class="truncate py-1.5 pr-4 text-etch-400">{{ artistOf(track.number) }}</td>

        <td class="py-1.5 text-right font-mono tabular-nums text-etch-100">
          {{ fromFrames(track.length) }}
        </td>

        <td class="py-1.5 pl-4 text-[0.6875rem] uppercase tracking-[0.12em]">
          <span v-if="status(track.number)" :class="statusTone[status(track.number)!]">
            {{ t(`rip.status.${status(track.number)}`) }}
          </span>
          <span v-else-if="track.preEmphasis" class="text-etch-600">
            {{ t('track.preEmphasis') }}
          </span>
        </td>

        <td class="py-1.5 pr-6 text-right font-mono tabular-nums text-etch-600">
          {{ sizeOf(track.length) }} MB
        </td>
      </tr>
    </tbody>
  </table>
</template>
