<script setup lang="ts">
import { invoke, isTauri } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

import type { Artwork, ReleaseCandidate } from '~/types/disc'

const props = defineProps<{ release: ReleaseCandidate | null }>()

const emit = defineEmits<{ close: []; pick: [dataUri: string] }>()

const { t } = useI18n()

const results = ref<Artwork[]>([])
const searching = ref(false)
const searched = ref(false)
const busy = ref(false)
const artist = ref(props.release?.artist ?? '')
const album = ref(props.release?.title ?? '')

async function search() {
  if (!isTauri() || searching.value) {
    return
  }

  searching.value = true
  results.value = []

  try {
    results.value = await invoke<Artwork[]>('search_artwork', {
      query: {
        artist: artist.value,
        album: album.value,
        // Only the service a release actually came from can be asked by id.
        musicbrainzId: props.release?.sourceId === 'musicBrainz' ? props.release.id : null,
        discogsId: props.release?.sourceId === 'discogs' ? props.release.id : null,
      },
    })
  } finally {
    searching.value = false
    searched.value = true
  }
}

/** The image is fetched by the backend, which is what keeps the window shut. */
async function choose(art: Artwork) {
  if (busy.value) {
    return
  }

  busy.value = true
  try {
    const data = await invoke<string | null>('fetch_cover', { url: art.full })
    if (data) {
      emit('pick', data)
      emit('close')
    }
  } finally {
    busy.value = false
  }
}

async function fromFile() {
  if (!isTauri() || busy.value) {
    return
  }

  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: t('cover.images'), extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp'] }],
  })

  if (typeof path !== 'string') {
    return
  }

  busy.value = true
  try {
    const data = await invoke<string | null>('cover_from_file', { path })
    if (data) {
      emit('pick', data)
      emit('close')
    }
  } finally {
    busy.value = false
  }
}

onMounted(search)

const field =
  'min-w-0 flex-1 rounded-xs border border-chassis-700 bg-chassis-950 px-2 py-1.5 text-[0.8125rem] text-etch-100 transition-colors hover:border-chassis-600 focus:border-brass-500 focus-visible:outline-none'
const action =
  'rounded-xs border px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] transition-colors focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40'
const quiet = `${action} border-chassis-700 text-etch-400 hover:border-etch-600 hover:text-etch-100`
</script>

<template>
  <div
    class="absolute inset-0 z-20 grid place-items-center bg-chassis-950/85 px-8 py-8"
    role="dialog"
    aria-modal="true"
    :aria-label="t('cover.heading')"
    @keydown.esc="emit('close')"
  >
    <section
      class="flex max-h-full w-full max-w-5xl flex-col rounded-xs border border-chassis-700 bg-chassis-900 shadow-2xl shadow-black/60"
    >
      <header class="flex items-center gap-3 border-b border-chassis-800 px-5 py-3">
        <h2 class="text-[0.6875rem] uppercase tracking-[0.18em] text-etch-400">
          {{ t('cover.heading') }}
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

      <form class="flex items-center gap-3 px-5 py-3" @submit.prevent="search">
        <input v-model="artist" type="search" :class="field" :placeholder="t('search.artist')" />
        <input v-model="album" type="search" :class="field" :placeholder="t('search.title')" />
        <button type="submit" :class="quiet" :disabled="searching">
          {{ searching ? t('metadata.identifying') : t('search.submit') }}
        </button>
        <button type="button" :class="quiet" :disabled="busy" @click="fromFile">
          {{ t('cover.fromFile') }}
        </button>
      </form>

      <div class="min-h-64 flex-1 overflow-y-auto border-t border-chassis-800 px-5 py-4">
        <p v-if="searching" class="text-[0.6875rem] text-etch-600">
          {{ t('metadata.identifying') }}
        </p>

        <p v-else-if="searched && !results.length" class="text-[0.6875rem] text-etch-600">
          {{ t('cover.empty') }}
        </p>

        <ul v-else class="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-4">
          <li v-for="(art, index) in results" :key="`${art.full}-${index}`">
            <button
              type="button"
              class="group w-full text-left focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500"
              :disabled="busy"
              @click="choose(art)"
            >
              <span
                class="block aspect-square overflow-hidden rounded-xs border border-chassis-700 bg-chassis-950 transition-colors group-hover:border-brass-500"
              >
                <img
                  :src="art.thumbnail"
                  alt=""
                  loading="lazy"
                  class="size-full object-cover transition-opacity group-hover:opacity-80"
                />
              </span>

              <span
                class="mt-1 block truncate text-[0.625rem] uppercase tracking-[0.12em] text-etch-600"
              >
                {{ t('source.' + art.sourceId) }}
                <template v-if="art.width && art.height"
                  >· {{ art.width }}×{{ art.height }}</template
                >
                <template v-else-if="art.kind">· {{ art.kind }}</template>
              </span>
            </button>
          </li>
        </ul>
      </div>
    </section>
  </div>
</template>
