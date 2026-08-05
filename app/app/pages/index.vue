<script setup lang="ts">
import { invoke, isTauri } from '@tauri-apps/api/core'
import { revealItemInDir } from '@tauri-apps/plugin-opener'

const { t, locale, locales, setLocale } = useI18n()
const { drives, selectedId, disc, faultMessage, busy, refresh, read, eject, select } = useDisc()
const metadata = useMetadata()
const release = useRelease()
const ripping = useRip()
const preview = usePreview()
const { settings, load: loadSettings } = useSettings()

const coreVersion = ref<string | null>(null)
const folder = ref<string | null>(null)
const searchOpen = ref(false)
const coverOpen = ref(false)

const toc = computed(() => disc.value?.toc ?? null)

/**
 * The path is recomputed whenever the names that build it change, so the panel
 * never shows somewhere the rip would not actually write.
 */
watch(
  () => [
    release.draft.value?.artist,
    release.draft.value?.title,
    disc.value?.musicbrainzDiscId,
    // The pattern decides the path as much as the names do.
    settings.value?.pattern,
    settings.value?.outputRoot,
  ],
  async () => {
    folder.value =
      isTauri() && disc.value
        ? await invoke<string | null>('rip_folder', { release: release.draft.value })
        : null
  },
  { immediate: true },
)

/** A chosen candidate becomes the working copy, sized to the disc in the drive. */
watch(metadata.release, (candidate) => {
  if (candidate) {
    release.adopt(candidate, toc.value)
  }
})

async function readDisc() {
  release.clear()
  ripping.reset()
  metadata.reset()
  preview.stop()

  await read()

  if (!disc.value) {
    return
  }

  await metadata.lookup()

  // Nothing recognised the disc, so the user starts from an empty sheet
  // rather than from nothing at all.
  if (!release.draft.value) {
    release.start(toc.value, disc.value.musicbrainzDiscId)
  }
}

async function ejectDisc() {
  release.clear()
  ripping.reset()
  metadata.reset()
  preview.stop()
  await eject()
}

async function ripDisc() {
  if (selectedId.value) {
    await ripping.start(selectedId.value, release.draft.value, release.includedNumbers(toc.value))
  }
}

async function revealFolder() {
  if (folder.value) {
    await revealItemInDir(folder.value)
  }
}

async function keepEdits() {
  if (release.draft.value) {
    await metadata.keep(release.draft.value)
  }
}

function toggleAll(include: boolean) {
  release.excluded.value = include
    ? []
    : (toc.value?.tracks ?? []).filter((track) => track.audio).map((track) => track.number)
}

onMounted(async () => {
  if (!isTauri()) {
    return
  }

  coreVersion.value = await invoke<string>('core_version')
  await loadSettings()
  await refresh()
})

const action =
  'rounded-xs border px-3 py-1.5 text-[0.6875rem] uppercase tracking-[0.16em] transition-colors focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40'
const quiet = `${action} border-chassis-700 text-etch-400 hover:border-etch-600 hover:text-etch-100`
const loud = `${action} border-brass-500 bg-brass-500/10 text-brass-400 hover:bg-brass-500/20`
</script>

<template>
  <div
    class="flex h-full flex-col overflow-hidden bg-chassis-950 font-sans text-etch-100 selection:bg-brass-500/30"
  >
    <!-- Actions first, then the disc's own fields, then its tracks: the same
         order the work happens in. -->
    <header class="flex shrink-0 items-center gap-3 border-b border-chassis-800 px-6 py-3">
      <h1 class="mr-3 font-display text-lg tracking-[0.22em] indent-[0.22em]">Toccata</h1>

      <button
        :class="loud"
        :disabled="busy || ripping.running.value || !selectedId"
        @click="readDisc"
      >
        {{ busy ? t('drive.reading') : t('drive.read') }}
      </button>

      <button
        :class="loud"
        :disabled="busy || ripping.running.value || !disc || !release.draft.value"
        @click="ripDisc"
      >
        {{ t('rip.start') }}
      </button>

      <button
        :class="quiet"
        :disabled="busy || ripping.running.value || !disc"
        @click="searchOpen = true"
      >
        {{ t('menu.search') }}
      </button>

      <button :class="quiet" :disabled="busy || !release.draft.value" @click="keepEdits">
        {{ t('editor.save') }}
      </button>

      <div class="ml-auto flex items-center gap-3">
        <p
          v-if="metadata.release.value"
          class="text-[0.625rem] uppercase tracking-[0.16em] text-etch-600"
        >
          {{
            metadata.release.value.sourceId === 'manual'
              ? t('source.manual')
              : t('metadata.from', { source: t('source.' + metadata.release.value.sourceId) })
          }}
        </p>

        <button
          :class="quiet"
          :disabled="busy || ripping.running.value || !selectedId"
          @click="ejectDisc"
        >
          {{ t('drive.eject') }}
        </button>
      </div>
    </header>

    <AlbumFields
      v-if="release.draft.value"
      v-model="release.draft.value"
      @artist-changed="release.spreadArtist"
    />

    <RipProgress
      v-if="ripping.running.value"
      class="shrink-0 rounded-none border-x-0 border-t-0"
      :track="ripping.track.value"
      :position="ripping.position.value"
      :track-count="ripping.trackCount.value"
      :track-share="ripping.trackShare.value"
      :disc-share="ripping.discShare.value"
      @cancel="ripping.cancel"
    />

    <main class="min-h-0 flex-1 overflow-y-auto">
      <p
        v-if="faultMessage"
        role="alert"
        class="m-6 rounded-xs border border-chassis-700 border-l-2 border-l-brass-500 bg-chassis-900 px-4 py-3 text-sm"
      >
        {{ faultMessage }}
      </p>

      <p
        v-else-if="!disc"
        class="py-24 text-center text-xs uppercase tracking-[0.24em] text-etch-600"
      >
        {{ t('disc.waiting') }}
      </p>

      <TrackTable
        v-else
        :toc="disc.toc"
        :release="release.draft.value"
        :selected="release.selected.value"
        :included="release.isIncluded"
        :status="ripping.statusOf"
        :playing="preview.playing.value"
        :loading="preview.loading.value"
        :busy="ripping.running.value"
        @select="release.selected.value = release.selected.value === $event ? null : $event"
        @toggle="release.toggle"
        @toggle-all="toggleAll"
        @play="preview.play(selectedId, $event)"
      />
    </main>

    <footer class="flex h-72 shrink-0 border-t border-chassis-800">
      <OutputPanel
        :drives="drives"
        :drive-id="selectedId"
        :folder="folder"
        :busy="ripping.running.value"
        @select-drive="select"
        @reveal="revealFolder"
      />

      <DetailPanel
        v-if="release.draft.value"
        v-model="release.draft.value"
        :selected="release.selected.value"
      />

      <div v-else class="flex flex-1 items-center justify-center px-6">
        <p class="text-[0.6875rem] uppercase tracking-[0.18em] text-etch-600">
          {{ t('disc.waiting') }}
        </p>
      </div>

      <CoverTile
        :cover="metadata.cover.value"
        :title="release.draft.value?.title ?? ''"
        @choose="coverOpen = true"
      />
    </footer>

    <div
      class="flex shrink-0 items-center gap-4 border-t border-chassis-800 px-6 py-2 text-[0.625rem] tracking-wide text-etch-600"
    >
      <nav :aria-label="t('settings.language.label')" class="flex items-center gap-1">
        <button
          v-for="option in locales"
          :key="option.code"
          type="button"
          :aria-pressed="option.code === locale"
          class="rounded-xs px-1.5 py-0.5 uppercase tracking-[0.18em] transition-colors hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 aria-pressed:text-brass-400"
          @click="setLocale(option.code)"
        >
          {{ option.code }}
        </button>
      </nav>

      <p v-if="ripping.folder.value && !ripping.running.value" class="text-etch-400">
        {{ t('rip.done', { folder: ripping.folder.value }) }}
        <span class="ml-2">
          {{
            ripping.unreadable.value
              ? t('rip.imperfect', ripping.unreadable.value)
              : t('rip.perfect')
          }}
        </span>
      </p>

      <p v-else-if="ripping.faultMessage.value" class="text-etch-400">
        {{ ripping.faultMessage.value }}
      </p>

      <p v-if="coreVersion" class="ml-auto font-mono">
        {{ t('about.core', { version: coreVersion }) }}
      </p>
    </div>

    <CoverChooser
      v-if="coverOpen"
      :release="metadata.release.value"
      @close="coverOpen = false"
      @pick="metadata.cover.value = $event"
    />

    <MetadataSearchDialog
      v-if="searchOpen && disc"
      :disc-track-count="disc.toc.tracks.filter((track) => track.audio).length"
      @close="searchOpen = false"
    />
  </div>
</template>
