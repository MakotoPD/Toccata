<script setup lang="ts">
import type { Track, TrackMetadata } from '~/types/disc'

const props = defineProps<{
  tracks: Track[]
  /** Titles for the tracks, when a release has been chosen. */
  metadata?: TrackMetadata[]
  /** Used to keep the credit column quiet on albums by a single artist. */
  releaseArtist?: string
}>()

const { t } = useI18n()
const { fromFrames } = useCdTime()

const titled = computed(() => (props.metadata?.length ?? 0) > 0)

function entryFor(number: number) {
  return props.metadata?.find((entry) => entry.number === number) ?? null
}

/** Only worth showing where a track credits someone else, as on compilations. */
function guestArtist(number: number) {
  const artist = entryFor(number)?.artist
  return artist && artist !== props.releaseArtist ? artist : null
}
</script>

<template>
  <table class="w-full border-collapse text-sm">
    <thead>
      <tr
        class="border-b border-chassis-700 text-[0.6875rem] uppercase tracking-[0.18em] text-etch-600"
      >
        <th scope="col" class="py-2 pr-4 text-left font-normal">{{ t('track.number') }}</th>
        <th v-if="titled" scope="col" class="py-2 pr-4 text-left font-normal">
          {{ t('track.title') }}
        </th>
        <th scope="col" class="py-2 pr-4 text-right font-normal">{{ t('track.start') }}</th>
        <th scope="col" class="py-2 pr-4 text-right font-normal">{{ t('track.length') }}</th>
        <th scope="col" class="py-2 text-left font-normal" />
      </tr>
    </thead>
    <tbody>
      <tr
        v-for="track in tracks"
        :key="track.number"
        class="border-b border-chassis-800 last:border-0"
      >
        <td class="py-2 pr-4 font-mono tabular-nums text-brass-400">
          {{ String(track.number).padStart(2, '0') }}
        </td>
        <td v-if="titled" class="py-2 pr-4">
          <span v-if="entryFor(track.number)" class="text-etch-100">
            {{ entryFor(track.number)?.title }}
          </span>
          <span v-else class="text-etch-600">—</span>
          <span v-if="guestArtist(track.number)" class="ml-2 text-xs text-etch-600">
            {{ guestArtist(track.number) }}
          </span>
        </td>
        <td class="py-2 pr-4 text-right font-mono tabular-nums text-etch-600">
          {{ fromFrames(track.start) }}
        </td>
        <td class="py-2 pr-4 text-right font-mono tabular-nums text-etch-100">
          {{ fromFrames(track.length) }}
        </td>
        <td class="py-2">
          <span class="flex gap-2 text-[0.625rem] uppercase tracking-[0.16em]">
            <span v-if="!track.audio" class="rounded-xs bg-chassis-700 px-1.5 py-0.5 text-etch-400">
              {{ t('track.data') }}
            </span>
            <span
              v-if="track.preEmphasis"
              class="rounded-xs bg-chassis-700 px-1.5 py-0.5 text-etch-400"
            >
              {{ t('track.preEmphasis') }}
            </span>
          </span>
        </td>
      </tr>
    </tbody>
  </table>
</template>
