<script setup lang="ts">
import type { ReleaseCandidate } from '~/types/disc'

const props = defineProps<{ discTrackCount: number }>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const metadata = useMetadata()

const artist = ref('')
const album = ref('')
const barcode = ref('')
const reference = ref('')
const chosen = ref<string | null>(null)

// Opening the dialog starts a fresh search rather than showing the last one.
onMounted(() => {
  metadata.results.value = []
  metadata.ranSearch.value = false
})
const opened = ref<string[]>([])

const selected = computed(
  () => metadata.results.value.find((result) => result.id === chosen.value) ?? null,
)

function matchesDisc(candidate: ReleaseCandidate) {
  return candidate.mediumTrackCounts.includes(props.discTrackCount)
}

function details(candidate: ReleaseCandidate) {
  return [
    candidate.date,
    candidate.country,
    candidate.label,
    candidate.barcode,
    candidate.disambiguation,
  ].filter(Boolean)
}

function toggleOpen(id: string) {
  opened.value = opened.value.includes(id)
    ? opened.value.filter((entry) => entry !== id)
    : [...opened.value, id]
}

async function use() {
  // A pasted address names its own service, so it is looked up without one.
  const pasted = reference.value.trim()
  const reference_ = pasted || chosen.value
  const source = pasted ? null : (selected.value?.sourceId ?? null)

  if (reference_ && (await metadata.adopt(reference_, source))) {
    emit('close')
  }
}

const field =
  'rounded-xs border border-chassis-700 bg-chassis-950 px-2 py-1.5 text-[0.8125rem] text-etch-100 placeholder:text-etch-600 transition-colors hover:border-chassis-600 focus:border-brass-500 focus-visible:outline-none'
const action =
  'rounded-xs border px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] transition-colors focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40'
const label = 'w-24 shrink-0 text-[0.625rem] uppercase tracking-[0.14em] text-etch-600'
</script>

<template>
  <!-- Kept inside the window rather than opened as a second one: the search
       needs the disc that is already loaded here. -->
  <div
    class="absolute inset-0 z-10 grid place-items-center bg-chassis-950/80 px-8 py-8"
    role="dialog"
    aria-modal="true"
    :aria-label="t('menu.search')"
    @keydown.esc="emit('close')"
  >
    <section
      class="flex max-h-full w-full max-w-3xl flex-col rounded-xs border border-chassis-700 bg-chassis-900 shadow-2xl shadow-black/60"
    >
      <header class="flex items-center border-b border-chassis-800 px-5 py-3">
        <h2 class="text-[0.6875rem] uppercase tracking-[0.18em] text-etch-400">
          {{ t('menu.search') }}
        </h2>
        <button
          type="button"
          class="ml-auto text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500"
          :aria-label="t('editor.cancel')"
          @click="emit('close')"
        >
          ✕
        </button>
      </header>

      <form
        class="flex flex-col gap-2 px-5 py-4"
        @submit.prevent="metadata.search(artist, album, '')"
      >
        <div class="flex items-center gap-3">
          <span :class="label">{{ t('search.artist') }}</span>
          <input v-model="artist" type="search" :class="[field, 'flex-1']" />
          <button
            type="submit"
            :class="[
              action,
              'border-chassis-700 text-etch-400 hover:border-etch-600 hover:text-etch-100',
            ]"
            :disabled="metadata.searching.value"
          >
            {{ metadata.searching.value ? t('metadata.identifying') : t('search.submit') }}
          </button>
        </div>

        <div class="flex items-center gap-3">
          <span :class="label">{{ t('search.title') }}</span>
          <input v-model="album" type="search" :class="[field, 'flex-1']" />
        </div>
      </form>

      <div class="flex items-center gap-3 border-t border-chassis-800 px-5 py-3">
        <span :class="label">{{ t('search.barcode') }}</span>
        <input v-model="barcode" type="search" :class="[field, 'flex-1 font-mono']" />
        <button
          type="button"
          :class="[
            action,
            'border-chassis-700 text-etch-400 hover:border-etch-600 hover:text-etch-100',
          ]"
          :disabled="metadata.searching.value || barcode.trim() === ''"
          @click="metadata.search('', '', barcode)"
        >
          {{ t('search.byBarcode') }}
        </button>
      </div>

      <div class="flex items-center gap-3 border-t border-chassis-800 px-5 py-3">
        <span :class="label">{{ t('search.reference') }}</span>
        <input
          v-model="reference"
          type="text"
          :class="[field, 'flex-1 font-mono text-[0.6875rem]']"
          :placeholder="t('search.referenceHint')"
        />
      </div>

      <div class="min-h-40 flex-1 overflow-y-auto border-t border-chassis-800 px-5 py-3">
        <p v-if="metadata.ranSearch.value" class="mb-2 text-[0.6875rem] text-etch-600">
          {{
            metadata.results.value.length
              ? t('search.found', metadata.results.value.length)
              : t('search.empty')
          }}
        </p>

        <ul class="flex flex-col">
          <li v-for="result in metadata.results.value" :key="result.id">
            <div
              class="flex items-baseline gap-2 border-b border-chassis-800 py-1.5"
              :class="chosen === result.id && 'text-brass-400'"
            >
              <button
                type="button"
                class="w-4 shrink-0 text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline-none"
                :aria-expanded="opened.includes(result.id)"
                :aria-label="t('search.tracksOf', { title: result.title })"
                @click="toggleOpen(result.id)"
              >
                {{ opened.includes(result.id) ? '⌄' : '›' }}
              </button>

              <button
                type="button"
                class="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2 text-left focus-visible:outline focus-visible:outline-1 focus-visible:-outline-offset-2 focus-visible:outline-brass-500"
                @click="chosen = result.id"
              >
                <span
                  class="shrink-0 font-mono text-[0.625rem] uppercase tracking-[0.12em] text-etch-600"
                >
                  {{ t('source.' + result.sourceId) }}
                </span>
                <span class="text-[0.8125rem] text-etch-100">{{ result.artist }}</span>
                <span class="text-[0.8125rem] text-etch-400">{{ result.title }}</span>
                <!-- Discogs does not count tracks until the release is
                     fetched, so an unknown count is left unsaid. -->
                <span
                  v-if="result.mediumTrackCounts.length"
                  class="font-mono text-[0.6875rem] tabular-nums text-etch-600"
                >
                  {{
                    t(
                      'metadata.trackCount',
                      result.mediumTrackCounts.reduce((a, b) => a + b, 0),
                    )
                  }}
                </span>
                <span
                  v-for="detail in details(result)"
                  :key="detail!"
                  class="text-[0.6875rem] text-etch-600"
                >
                  · {{ detail }}
                </span>
                <span
                  v-if="matchesDisc(result)"
                  class="ml-auto shrink-0 rounded-xs border border-brass-500/40 px-1.5 text-[0.625rem] uppercase tracking-[0.12em] text-brass-400"
                >
                  {{ t('metadata.trackMatch') }}
                </span>
              </button>
            </div>

            <ol
              v-if="opened.includes(result.id)"
              class="border-b border-chassis-800 py-1 pl-10 text-[0.75rem] text-etch-400"
            >
              <li v-if="!result.tracks.length" class="py-0.5 text-etch-600">
                {{ t('search.tracksHidden') }}
              </li>
              <li v-for="entry in result.tracks" :key="entry.number" class="py-0.5">
                <span class="mr-2 font-mono tabular-nums text-etch-600">
                  {{ String(entry.number).padStart(2, '0') }}
                </span>
                {{ entry.title }}
              </li>
            </ol>
          </li>
        </ul>
      </div>

      <footer class="flex items-center gap-3 border-t border-chassis-800 px-5 py-3">
        <button
          type="button"
          :class="[action, 'border-brass-500 bg-brass-500/10 text-brass-400 hover:bg-brass-500/20']"
          :disabled="metadata.searching.value || (!chosen && reference.trim() === '')"
          @click="use"
        >
          {{ t('search.use') }}
        </button>

        <button
          type="button"
          :class="[
            action,
            'border-chassis-700 text-etch-400 hover:border-etch-600 hover:text-etch-100',
          ]"
          @click="emit('close')"
        >
          {{ t('editor.cancel') }}
        </button>

        <p v-if="selected" class="ml-auto truncate text-[0.6875rem] text-etch-600">
          {{ selected.artist }} — {{ selected.title }}
        </p>
      </footer>
    </section>
  </div>
</template>
