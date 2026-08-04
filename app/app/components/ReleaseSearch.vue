<script setup lang="ts">
import type { ReleaseCandidate } from '~/types/disc'

defineProps<{
  results: ReleaseCandidate[]
  discTrackCount: number
  busy: boolean
  /** Whether a search has run, so an empty list can be told from no search. */
  searched: boolean
}>()

const emit = defineEmits<{
  search: [artist: string, title: string]
  adopt: [reference: string]
}>()

const { t } = useI18n()

const artist = ref('')
const title = ref('')
const reference = ref('')

const canSearch = computed(() => artist.value.trim() !== '' || title.value.trim() !== '')

const field =
  'rounded-xs border border-chassis-700 bg-chassis-900 px-3 py-1.5 text-sm text-etch-100 placeholder:text-etch-600 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500'
const action =
  'rounded-xs border border-chassis-700 px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] text-etch-400 transition-colors hover:border-etch-600 hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40'
</script>

<template>
  <section class="rounded-xs border border-chassis-800 bg-chassis-900/40 px-5 py-4">
    <h3 class="text-[0.625rem] uppercase tracking-[0.18em] text-etch-600">
      {{ t('search.heading') }}
    </h3>

    <form
      class="mt-3 flex flex-wrap items-end gap-3"
      @submit.prevent="emit('search', artist, title)"
    >
      <label class="flex flex-col gap-1">
        <span class="text-[0.625rem] uppercase tracking-[0.16em] text-etch-600">
          {{ t('search.artist') }}
        </span>
        <input
          v-model="artist"
          type="search"
          :class="field"
          :placeholder="t('search.artistHint')"
        />
      </label>

      <label class="flex flex-col gap-1">
        <span class="text-[0.625rem] uppercase tracking-[0.16em] text-etch-600">
          {{ t('search.title') }}
        </span>
        <input v-model="title" type="search" :class="field" :placeholder="t('search.titleHint')" />
      </label>

      <button type="submit" :class="action" :disabled="busy || !canSearch">
        {{ busy ? t('metadata.identifying') : t('search.submit') }}
      </button>
    </form>

    <form
      class="mt-3 flex flex-wrap items-end gap-3 border-t border-chassis-800 pt-3"
      @submit.prevent="emit('adopt', reference)"
    >
      <label class="flex flex-1 flex-col gap-1">
        <span class="text-[0.625rem] uppercase tracking-[0.16em] text-etch-600">
          {{ t('search.reference') }}
        </span>
        <input
          v-model="reference"
          type="text"
          :class="[field, 'w-full font-mono text-xs']"
          :placeholder="t('search.referenceHint')"
        />
      </label>

      <button type="submit" :class="action" :disabled="busy || reference.trim() === ''">
        {{ t('search.use') }}
      </button>
    </form>

    <template v-if="searched">
      <p class="mt-5 text-sm text-etch-400">
        {{ results.length ? t('search.found', results.length) : t('search.empty') }}
      </p>

      <ReleasePicker
        v-if="results.length"
        class="mt-3"
        :candidates="results"
        :selected-id="null"
        :disc-track-count="discTrackCount"
        @select="emit('adopt', $event)"
      />
    </template>
  </section>
</template>
