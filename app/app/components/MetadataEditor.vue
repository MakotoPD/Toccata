<script setup lang="ts">
import type { ReleaseCandidate, TrackMetadata } from '~/types/disc'

const props = defineProps<{
  /** What to start from, or nothing at all for a disc no source knows. */
  release: ReleaseCandidate | null
  /** Rows always follow the disc, not the release a database offered. */
  trackCount: number
  discId: string
  busy: boolean
}>()

const emit = defineEmits<{ save: [ReleaseCandidate]; cancel: [] }>()

const { t } = useI18n()

function blankTracks(from: TrackMetadata[], artist: string): TrackMetadata[] {
  return Array.from({ length: props.trackCount }, (_, index) => {
    const existing = from.find((track) => track.number === index + 1)

    return {
      number: index + 1,
      title: existing?.title ?? '',
      artist: existing?.artist ?? artist,
      lengthMs: existing?.lengthMs ?? null,
    }
  })
}

function draftFrom(release: ReleaseCandidate | null): ReleaseCandidate {
  return {
    sourceId: 'manual',
    relayedFrom: null,
    // The Disc ID doubles as the local identifier: it is what the release is
    // filed under and it never collides with another disc.
    id: props.discId,
    title: release?.title ?? '',
    artist: release?.artist ?? '',
    date: release?.date ?? null,
    country: release?.country ?? null,
    label: release?.label ?? null,
    barcode: release?.barcode ?? null,
    disambiguation: release?.disambiguation ?? null,
    discNumber: release?.discNumber ?? 1,
    discTotal: release?.discTotal ?? null,
    mediumTrackCounts: [props.trackCount],
    coverArt: release?.coverArt ?? null,
    tracks: blankTracks(release?.tracks ?? [], release?.artist ?? ''),
  }
}

const draft = ref<ReleaseCandidate>(draftFrom(props.release))

/** Filling in the release artist is worth doing once, not once per track. */
function applyArtistToTracks() {
  for (const track of draft.value.tracks) {
    track.artist = draft.value.artist
  }
}

const label = 'text-[0.625rem] uppercase tracking-[0.16em] text-etch-600'
const field =
  'rounded-xs border border-chassis-700 bg-chassis-900 px-3 py-1.5 text-sm text-etch-100 placeholder:text-etch-600 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500'
</script>

<template>
  <form
    class="rounded-xs border border-chassis-700 bg-chassis-900/40 px-5 py-4"
    @submit.prevent="emit('save', draft)"
  >
    <h3 :class="label">{{ t('editor.heading') }}</h3>

    <div class="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.title') }}</span>
        <input v-model="draft.title" type="text" :class="field" />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.artist') }}</span>
        <input v-model="draft.artist" type="text" :class="field" @change="applyArtistToTracks" />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.date') }}</span>
        <input v-model="draft.date" type="text" :class="field" placeholder="1998-07-14" />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.label') }}</span>
        <input v-model="draft.label" type="text" :class="field" />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.barcode') }}</span>
        <input v-model="draft.barcode" type="text" :class="[field, 'font-mono']" />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.country') }}</span>
        <input v-model="draft.country" type="text" :class="field" placeholder="PL" />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.discNumber') }}</span>
        <input
          v-model.number="draft.discNumber"
          type="number"
          min="1"
          :class="[field, 'tabular-nums']"
        />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.discTotal') }}</span>
        <input
          v-model.number="draft.discTotal"
          type="number"
          min="1"
          :class="[field, 'tabular-nums']"
        />
      </label>

      <label class="flex flex-col gap-1">
        <span :class="label">{{ t('editor.disambiguation') }}</span>
        <input v-model="draft.disambiguation" type="text" :class="field" />
      </label>
    </div>

    <h4 :class="[label, 'mt-6']">{{ t('editor.tracks') }}</h4>

    <ul class="mt-2 flex flex-col gap-2">
      <li v-for="track in draft.tracks" :key="track.number" class="flex items-center gap-3">
        <span class="w-8 shrink-0 font-mono tabular-nums text-brass-400">
          {{ String(track.number).padStart(2, '0') }}
        </span>
        <input
          v-model="track.title"
          type="text"
          :class="[field, 'flex-1']"
          :aria-label="t('editor.trackTitle', { number: track.number })"
        />
        <input
          v-model="track.artist"
          type="text"
          :class="[field, 'w-56']"
          :aria-label="t('editor.trackArtist', { number: track.number })"
        />
      </li>
    </ul>

    <div class="mt-5 flex items-center gap-3">
      <button
        type="submit"
        class="rounded-xs border border-brass-500 bg-brass-500/10 px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] text-brass-400 transition-colors hover:bg-brass-500/20 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40"
        :disabled="busy"
      >
        {{ t('editor.save') }}
      </button>

      <button
        type="button"
        class="rounded-xs border border-chassis-700 px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] text-etch-400 transition-colors hover:border-etch-600 hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500"
        @click="emit('cancel')"
      >
        {{ t('editor.cancel') }}
      </button>

      <p class="text-[0.625rem] tracking-wide text-etch-600">{{ t('editor.storedUnder') }}</p>
    </div>
  </form>
</template>
