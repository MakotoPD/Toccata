<script setup lang="ts">
import type { ReleaseCandidate } from '~/types/disc'

const props = defineProps<{
  /** Number of the row the table has selected, or nothing for the release. */
  selected: number | null
}>()

const release = defineModel<ReleaseCandidate>({ required: true })

const { t } = useI18n()

/** Editing a row edits the release it belongs to, so the same object is used. */
const track = computed(() =>
  props.selected === null
    ? null
    : (release.value.tracks.find((entry) => entry.number === props.selected) ?? null),
)

const tab = ref<'tags' | 'encoder'>('tags')

const tabs = [
  { id: 'tags', label: 'panel.tags' },
  { id: 'encoder', label: 'panel.encoder' },
] as const
</script>

<template>
  <section class="flex min-w-0 flex-1">
    <!-- Vertical tabs, so the panel keeps its width for the fields themselves. -->
    <nav class="flex shrink-0 flex-col border-r border-chassis-800">
      <button
        v-for="entry in tabs"
        :key="entry.id"
        type="button"
        :aria-pressed="tab === entry.id"
        class="border-l-2 border-transparent px-2 py-3 text-[0.625rem] uppercase tracking-[0.14em] text-etch-600 transition-colors [writing-mode:vertical-rl] hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:-outline-offset-2 focus-visible:outline-brass-500 aria-pressed:border-l-brass-500 aria-pressed:text-brass-400"
        @click="tab = entry.id"
      >
        {{ t(entry.label) }}
      </button>
    </nav>

    <div class="min-w-0 flex-1 overflow-y-auto px-6 py-4">
      <template v-if="tab === 'tags'">
        <p class="mb-3 text-[0.625rem] uppercase tracking-[0.16em] text-etch-600">
          {{ track ? t('panel.forTrack', { number: track.number }) : t('panel.forRelease') }}
        </p>

        <div v-if="track" class="grid grid-cols-[6rem_minmax(0,1fr)] items-center gap-x-3 gap-y-2">
          <MetaField v-model="track.title" :label="t('track.title')" />
          <MetaField v-model="track.artist" :label="t('editor.artist')" />
        </div>

        <div v-else class="grid grid-cols-[6rem_minmax(0,1fr)] items-center gap-x-3 gap-y-2">
          <MetaField
            v-model="release.disambiguation"
            :label="t('editor.disambiguation')"
            width="full"
          />
          <MetaField v-model="release.country" :label="t('editor.country')" />
          <p class="col-span-2 mt-2 text-[0.6875rem] leading-relaxed text-etch-600">
            {{ t('panel.hint') }}
          </p>
        </div>
      </template>

      <template v-else>
        <p class="mb-3 text-[0.625rem] uppercase tracking-[0.16em] text-etch-600">WAV</p>
        <p class="text-[0.6875rem] leading-relaxed text-etch-400">
          {{ t('panel.encoderNone') }}
        </p>
      </template>
    </div>
  </section>
</template>
