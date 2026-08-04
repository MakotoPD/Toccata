<script setup lang="ts">
import type { ReleaseCandidate } from '~/types/disc'

const props = defineProps<{
  result: ReleaseCandidate
  /** Tracks the drive reported, for spotting the right disc of a set. */
  discTrackCount: number
  /** Which disc of which release is currently picked, if any. */
  chosenId: string | null
  chosenMedium: number | null
}>()

const emit = defineEmits<{ choose: [id: string, medium: number | null] }>()

const { t } = useI18n()
const metadata = useMetadata()

const open = ref(false)
const loading = ref(false)
/** The release once fetched: neither service lists discs before that. */
const full = ref<ReleaseCandidate | null>(null)

const media = computed(() => full.value?.media ?? [])

/** A set with one disc is not a set, and does not need choosing between. */
const isSet = computed(() => media.value.length > 1)

function matchesDisc(count: number) {
  return count === props.discTrackCount
}

const summary = computed(() =>
  [
    props.result.date,
    props.result.country,
    props.result.label,
    props.result.barcode,
    props.result.disambiguation,
  ].filter(Boolean),
)

async function toggle() {
  open.value = !open.value

  if (!open.value || full.value || loading.value) {
    return
  }

  loading.value = true
  full.value = await metadata.preview(props.result.id, props.result.sourceId)
  loading.value = false

  // A single disc release has nothing to choose, so opening it picks it.
  if (full.value && full.value.media.length <= 1) {
    emit('choose', props.result.id, null)
  }
}

const disc = 'flex w-full items-baseline gap-2 py-1 pl-6 pr-2 text-left transition-colors'
</script>

<template>
  <li>
    <div
      class="flex items-baseline gap-2 border-b border-chassis-800 py-1.5"
      :class="chosenId === result.id && 'text-brass-400'"
    >
      <button
        type="button"
        class="w-4 shrink-0 text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline-none"
        :aria-expanded="open"
        :aria-label="t('search.tracksOf', { title: result.title })"
        @click="toggle"
      >
        {{ open ? '⌄' : '›' }}
      </button>

      <button
        type="button"
        class="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2 text-left focus-visible:outline focus-visible:outline-1 focus-visible:-outline-offset-2 focus-visible:outline-brass-500"
        @click="toggle"
      >
        <span class="shrink-0 font-mono text-[0.625rem] uppercase tracking-[0.12em] text-etch-600">
          {{ t('source.' + result.sourceId) }}
        </span>
        <span class="text-[0.8125rem] text-etch-100">{{ result.artist }}</span>
        <span class="text-[0.8125rem] text-etch-400">{{ result.title }}</span>

        <span v-for="detail in summary" :key="detail!" class="text-[0.6875rem] text-etch-600">
          · {{ detail }}
        </span>
      </button>
    </div>

    <div v-if="open" class="border-b border-chassis-800 py-1">
      <p v-if="loading" class="py-1 pl-10 text-[0.6875rem] text-etch-600">
        {{ t('metadata.identifying') }}
      </p>

      <p v-else-if="!media.length" class="py-1 pl-10 text-[0.6875rem] text-etch-600">
        {{ t('search.noTracks') }}
      </p>

      <!-- One block per disc, because a boxed set holds several and only one
           of them is in the drive. -->
      <template v-else>
        <div v-for="medium in media" :key="medium.position">
          <button
            v-if="isSet"
            type="button"
            :class="[
              disc,
              chosenId === result.id && chosenMedium === medium.position
                ? 'bg-brass-500/10 text-brass-400'
                : 'text-etch-400 hover:bg-chassis-800',
            ]"
            @click="emit('choose', result.id, medium.position)"
          >
            <span class="text-[0.75rem] uppercase tracking-[0.14em]">
              {{ t('search.discNumber', { number: medium.position }) }}
            </span>
            <span class="font-mono text-[0.6875rem] tabular-nums text-etch-600">
              {{ t('metadata.trackCount', medium.tracks.length) }}
            </span>
            <span
              v-if="matchesDisc(medium.tracks.length)"
              class="ml-auto shrink-0 rounded-xs border border-brass-500/40 px-1.5 text-[0.625rem] uppercase tracking-[0.12em] text-brass-400"
            >
              {{ t('metadata.trackMatch') }}
            </span>
          </button>

          <ol class="py-0.5 pl-12 text-[0.75rem] text-etch-400">
            <li v-for="entry in medium.tracks" :key="entry.number" class="py-0.5">
              <span class="mr-2 font-mono tabular-nums text-etch-600">
                {{ String(entry.number).padStart(2, '0') }}
              </span>
              {{ entry.title }}
              <span
                v-if="entry.artist && entry.artist !== result.artist"
                class="ml-2 text-etch-600"
              >
                {{ entry.artist }}
              </span>
            </li>
          </ol>
        </div>
      </template>
    </div>
  </li>
</template>
