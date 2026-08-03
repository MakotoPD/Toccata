<script setup lang="ts">
import { invoke, isTauri } from '@tauri-apps/api/core'

const { t, locale, locales, setLocale } = useI18n()
const { fromFrames } = useCdTime()
const { drives, selectedId, disc, faultMessage, busy, refresh, read, eject, select } = useDisc()
const metadata = useMetadata()

const coreVersion = ref<string | null>(null)

const totalFrames = computed(() =>
  disc.value ? disc.value.toc.leadOut - (disc.value.toc.tracks[0]?.start ?? 0) : 0,
)

async function readAndIdentify() {
  metadata.reset()
  await read()

  if (disc.value) {
    await metadata.lookup()
  }
}

async function ejectDisc() {
  metadata.reset()
  await eject()
}

onMounted(async () => {
  if (!isTauri()) {
    return
  }

  coreVersion.value = await invoke<string>('core_version')
  await refresh()
})
</script>

<template>
  <div
    class="flex h-full flex-col overflow-hidden bg-chassis-950 bg-[repeating-radial-gradient(circle_at_82%_-10%,transparent_0,transparent_23px,rgba(217,180,99,0.03)_24px,transparent_25px)] font-sans text-etch-100"
  >
    <header class="flex shrink-0 items-center gap-6 border-b border-chassis-800 px-8 py-5">
      <h1 class="font-display text-xl tracking-[0.22em] indent-[0.22em]">Toccata</h1>

      <div class="ml-auto flex items-center gap-3">
        <label class="text-[0.6875rem] uppercase tracking-[0.18em] text-etch-600" for="drive">
          {{ t('drive.label') }}
        </label>
        <select
          id="drive"
          :value="selectedId ?? ''"
          :disabled="drives.length === 0"
          class="rounded-xs border border-chassis-700 bg-chassis-900 px-3 py-1.5 text-sm text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:text-etch-600"
          @change="select(($event.target as HTMLSelectElement).value)"
        >
          <option v-if="drives.length === 0" value="">{{ t('drive.none') }}</option>
          <option v-for="option in drives" :key="option.id" :value="option.id">
            {{ option.name }}
          </option>
        </select>

        <button
          type="button"
          class="rounded-xs border border-chassis-700 px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] text-etch-400 transition-colors hover:border-etch-600 hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40"
          :disabled="busy"
          @click="refresh"
        >
          {{ t('drive.rescan') }}
        </button>

        <button
          type="button"
          class="rounded-xs border border-brass-500 bg-brass-500/10 px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] text-brass-400 transition-colors hover:bg-brass-500/20 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40"
          :disabled="busy || metadata.searching.value || !selectedId"
          @click="readAndIdentify"
        >
          {{
            busy
              ? t('drive.reading')
              : metadata.searching.value
                ? t('metadata.identifying')
                : t('drive.read')
          }}
        </button>

        <button
          type="button"
          class="rounded-xs border border-chassis-700 px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] text-etch-400 transition-colors hover:border-etch-600 hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40"
          :disabled="busy || !selectedId"
          @click="ejectDisc"
        >
          {{ t('drive.eject') }}
        </button>
      </div>
    </header>

    <main class="min-h-0 flex-1 overflow-y-auto px-8 py-8">
      <p
        v-if="faultMessage"
        role="alert"
        class="rounded-xs border border-chassis-700 border-l-2 border-l-brass-500 bg-chassis-900 px-4 py-3 text-sm text-etch-100"
      >
        {{ faultMessage }}
      </p>

      <p
        v-else-if="!disc"
        class="py-16 text-center text-xs uppercase tracking-[0.24em] text-etch-600"
      >
        {{ t('disc.waiting') }}
      </p>

      <template v-else>
        <header v-if="metadata.release.value" class="mb-6 flex items-start gap-5">
          <img
            v-if="metadata.cover.value"
            :src="metadata.cover.value"
            :alt="t('metadata.coverOf', { title: metadata.release.value.title })"
            class="size-24 shrink-0 rounded-xs border border-chassis-700 object-cover"
          />

          <div>
            <h2 class="font-display text-2xl text-etch-100">{{ metadata.release.value.title }}</h2>
            <p class="mt-1 text-sm text-etch-400">{{ metadata.release.value.artist }}</p>
            <p class="mt-2 text-[0.625rem] uppercase tracking-[0.18em] text-etch-600">
              {{ t('metadata.from', { source: t('source.' + metadata.release.value.sourceId) }) }}
              <span v-if="metadata.release.value.relayedFrom">
                {{ t('metadata.relayedFrom', { source: metadata.release.value.relayedFrom }) }}
              </span>
            </p>
          </div>
        </header>

        <dl class="mb-8 grid gap-x-10 gap-y-4 sm:grid-cols-2 lg:grid-cols-4">
          <div>
            <dt class="text-[0.625rem] uppercase tracking-[0.18em] text-etch-600">
              {{ t('disc.totalTime') }}
            </dt>
            <dd class="mt-1 font-mono tabular-nums text-lg text-etch-100">
              {{ fromFrames(totalFrames) }}
            </dd>
          </div>
          <div>
            <dt class="text-[0.625rem] uppercase tracking-[0.18em] text-etch-600">
              {{ t('disc.tracks') }}
            </dt>
            <dd class="mt-1 font-mono tabular-nums text-lg text-etch-100">
              {{ t('disc.trackCount', disc.toc.tracks.length) }}
            </dd>
          </div>
          <div>
            <dt class="text-[0.625rem] uppercase tracking-[0.18em] text-etch-600">
              {{ t('disc.musicbrainzId') }}
            </dt>
            <dd class="mt-1 font-mono text-xs break-all text-brass-400">
              {{ disc.musicbrainzDiscId }}
            </dd>
          </div>
          <div>
            <dt class="text-[0.625rem] uppercase tracking-[0.18em] text-etch-600">
              {{ t('disc.freedbId') }}
            </dt>
            <dd class="mt-1 font-mono text-xs text-etch-400">{{ disc.freedbId }}</dd>
          </div>
        </dl>

        <ReleasePicker
          v-if="metadata.candidates.value.length > 1"
          class="mb-8"
          :candidates="metadata.candidates.value"
          :selected-id="metadata.selectedId.value"
          :disc-track-count="disc.toc.tracks.length"
          @select="metadata.select"
        />

        <p
          v-else-if="metadata.searched.value && metadata.candidates.value.length === 0"
          class="mb-8 text-xs uppercase tracking-[0.2em] text-etch-600"
        >
          {{ t('metadata.none') }}
        </p>

        <ul v-if="metadata.failureMessages.value.length" class="mb-8 flex flex-col gap-2">
          <li
            v-for="message in metadata.failureMessages.value"
            :key="message"
            class="rounded-xs border border-chassis-700 border-l-2 border-l-etch-600 bg-chassis-900 px-4 py-2 text-xs text-etch-400"
          >
            {{ message }}
          </li>
        </ul>

        <TrackList
          :tracks="disc.toc.tracks"
          :metadata="metadata.release.value?.tracks"
          :release-artist="metadata.release.value?.artist"
        />
      </template>
    </main>

    <footer class="flex shrink-0 items-center gap-4 border-t border-chassis-800 px-8 py-4">
      <nav :aria-label="t('settings.language.label')" class="flex items-center gap-1">
        <button
          v-for="option in locales"
          :key="option.code"
          type="button"
          :aria-pressed="option.code === locale"
          class="rounded-xs px-2 py-1 text-[0.625rem] uppercase tracking-[0.18em] text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 aria-pressed:text-brass-400"
          @click="setLocale(option.code)"
        >
          {{ option.code }}
        </button>
      </nav>

      <p v-if="coreVersion" class="ml-auto font-mono text-[0.625rem] tracking-wider text-etch-600">
        {{ t('about.core', { version: coreVersion }) }}
      </p>
    </footer>
  </div>
</template>
