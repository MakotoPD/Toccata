<script setup lang="ts">
import { invoke, isTauri } from '@tauri-apps/api/core'
import { revealItemInDir } from '@tauri-apps/plugin-opener'

import type { RipEntry, RipTrackEntry } from '~/types/disc'

const emit = defineEmits<{ close: [] }>()

const { t, locale } = useI18n()
const { settings, save } = useSettings()

const rips = ref<RipEntry[]>([])
const opened = ref<number | null>(null)
const tracks = ref<RipTrackEntry[]>([])
const tab = ref<'history' | 'tokens'>('history')

const when = computed(
  () => new Intl.DateTimeFormat(locale.value, { dateStyle: 'medium', timeStyle: 'short' }),
)

async function load() {
  if (isTauri()) {
    rips.value = await invoke<RipEntry[]>('rip_history', { limit: 100 })
  }
}

async function toggle(rip: RipEntry) {
  if (opened.value === rip.id) {
    opened.value = null
    return
  }

  opened.value = rip.id
  tracks.value = isTauri() ? await invoke<RipTrackEntry[]>('rip_tracks', { rip: rip.id }) : []
}

/** Drops the record. The files are left where they are. */
async function forget(rip: RipEntry) {
  if (isTauri()) {
    await invoke('forget_rip', { rip: rip.id })
    await load()
  }
}

const token = (name: 'discogs' | 'lastfm') =>
  computed({
    get: () => settings.value?.tokens?.[name] ?? '',
    set: (value: string) => {
      // Both keys always travel together, so an absent one is written as
      // nothing rather than left out and silently kept.
      const tokens = { discogs: null, lastfm: null, ...settings.value?.tokens }
      void save({ tokens: { ...tokens, [name]: value.trim() || null } })
    },
  })

const discogs = token('discogs')
const lastfm = token('lastfm')

onMounted(load)

const action =
  'rounded-xs border px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] transition-colors focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40'
const quiet = `${action} border-chassis-700 text-etch-400 hover:border-etch-600 hover:text-etch-100`
const field =
  'min-w-0 flex-1 rounded-xs border border-chassis-700 bg-chassis-950 px-2 py-1.5 font-mono text-[0.75rem] text-etch-100 transition-colors hover:border-chassis-600 focus:border-brass-500 focus-visible:outline-none'
</script>

<template>
  <div
    class="absolute inset-0 z-20 grid place-items-center bg-chassis-950/85 px-8 py-8"
    role="dialog"
    aria-modal="true"
    :aria-label="t('history.heading')"
    @keydown.esc="emit('close')"
  >
    <section
      class="flex max-h-full w-full max-w-5xl flex-col rounded-xs border border-chassis-700 bg-chassis-900 shadow-2xl shadow-black/60"
    >
      <header class="flex items-center gap-4 border-b border-chassis-800 px-5 py-3">
        <button
          v-for="entry in (['history', 'tokens'] as const)"
          :key="entry"
          type="button"
          :aria-pressed="tab === entry"
          class="text-[0.6875rem] uppercase tracking-[0.18em] text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 aria-pressed:text-brass-400"
          @click="tab = entry"
        >
          {{ t(`history.${entry}`) }}
        </button>

        <button
          type="button"
          class="ml-auto text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500"
          :aria-label="t('editor.cancel')"
          @click="emit('close')"
        >
          ✕
        </button>
      </header>

      <div class="min-h-64 flex-1 overflow-y-auto px-5 py-4">
        <template v-if="tab === 'history'">
          <p v-if="!rips.length" class="text-[0.6875rem] text-etch-600">{{ t('history.empty') }}</p>

          <ul v-else class="text-[0.8125rem]">
            <li v-for="rip in rips" :key="rip.id" class="border-b border-chassis-800">
              <div class="flex items-baseline gap-3 py-1.5">
                <button
                  type="button"
                  class="w-4 shrink-0 text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline-none"
                  :aria-expanded="opened === rip.id"
                  :aria-label="t('history.tracksOf', { title: rip.title })"
                  @click="toggle(rip)"
                >
                  {{ opened === rip.id ? '⌄' : '›' }}
                </button>

                <span class="min-w-0 flex-1 truncate">
                  <span class="text-etch-100">{{ rip.artist }}</span>
                  <span class="ml-2 text-etch-400">{{ rip.title }}</span>
                </span>

                <span class="shrink-0 text-[0.6875rem] text-etch-600">
                  {{ when.format(new Date(rip.rippedAt * 1000)) }}
                </span>

                <!-- The one thing worth reading at a glance: whether it is
                     bit-perfect. -->
                <span
                  class="w-24 shrink-0 text-right text-[0.625rem] uppercase tracking-[0.12em]"
                  :class="rip.unreadableSectors ? 'text-brass-400' : 'text-emerald-400'"
                >
                  {{
                    rip.unreadableSectors
                      ? t('history.imperfect', rip.unreadableSectors)
                      : t('history.perfect')
                  }}
                </span>

                <button type="button" :class="quiet" @click="revealItemInDir(rip.folder)">
                  {{ t('output.reveal') }}
                </button>
                <button type="button" :class="quiet" @click="forget(rip)">
                  {{ t('history.forget') }}
                </button>
              </div>

              <div v-if="opened === rip.id" class="pb-2 pl-7">
                <p class="mb-1 font-mono text-[0.625rem] text-etch-600">
                  {{ rip.folder }} · {{ rip.drive }} · {{ rip.readOffset >= 0 ? '+' : ''
                  }}{{ rip.readOffset }} {{ t('drive.samples') }}
                </p>

                <table class="w-full font-mono text-[0.6875rem] text-etch-400">
                  <tr v-for="track in tracks" :key="track.number">
                    <td class="w-8 py-0.5 text-right tabular-nums text-etch-600">
                      {{ String(track.number).padStart(2, '0') }}
                    </td>
                    <td class="py-0.5 pl-3 font-sans text-etch-100">{{ track.title }}</td>
                    <td class="w-24 py-0.5 text-right tabular-nums">
                      {{ track.crc32.toString(16).toUpperCase().padStart(8, '0') }}
                    </td>
                    <td class="w-28 py-0.5 text-right tabular-nums">
                      {{ track.accurateripV1.toString(16).toUpperCase().padStart(8, '0') }}
                    </td>
                    <td class="w-28 py-0.5 text-right tabular-nums">
                      {{ track.accurateripV2.toString(16).toUpperCase().padStart(8, '0') }}
                    </td>
                  </tr>
                </table>
              </div>
            </li>
          </ul>
        </template>

        <!-- Every one of these is optional. The application is fully usable
             without any, which is why nothing here is ever demanded. -->
        <template v-else>
          <div class="space-y-4">
            <div>
              <label
                class="mb-1 block text-[0.625rem] uppercase tracking-[0.14em] text-etch-600"
                for="token-discogs"
              >
                Discogs
              </label>
              <input id="token-discogs" v-model="discogs" type="password" :class="field" />
              <p class="mt-1 text-[0.6875rem] leading-relaxed text-etch-600">
                {{ t('history.discogsHint') }}
              </p>
            </div>

            <div>
              <label
                class="mb-1 block text-[0.625rem] uppercase tracking-[0.14em] text-etch-600"
                for="token-lastfm"
              >
                Last.fm
              </label>
              <input id="token-lastfm" v-model="lastfm" type="password" :class="field" />
              <p class="mt-1 text-[0.6875rem] leading-relaxed text-etch-600">
                {{ t('history.lastfmHint') }}
              </p>
            </div>

            <p class="text-[0.6875rem] leading-relaxed text-etch-400">
              {{ t('history.tokensHint') }}
            </p>
          </div>
        </template>
      </div>
    </section>
  </div>
</template>
