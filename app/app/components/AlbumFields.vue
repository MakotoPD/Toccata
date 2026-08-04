<script setup lang="ts">
import type { ReleaseCandidate } from '~/types/disc'

const release = defineModel<ReleaseCandidate>({ required: true })

const emit = defineEmits<{ artistChanged: [] }>()

const { t } = useI18n()

/** Databases store a full date; the field a ripper wants is the year. */
const year = computed({
  get: () => release.value.date ?? '',
  set: (value: string) => {
    release.value.date = value.trim() === '' ? null : value
  },
})

/** One box for both halves of "disc 3 of 4", the way every ripper writes it. */
const discs = computed({
  get: () =>
    release.value.discTotal
      ? `${release.value.discNumber}/${release.value.discTotal}`
      : String(release.value.discNumber),
  set: (value: string) => {
    const [number, total] = value.split('/')
    release.value.discNumber = Number(number) || 1
    release.value.discTotal = total ? Number(total) || null : null
  },
})
</script>

<template>
  <!-- Four label/value pairs to a row, so every column lines up no matter how
       long the words in a given language turn out to be. -->
  <section
    class="grid shrink-0 grid-cols-[7rem_minmax(0,1fr)_6rem_minmax(0,1fr)_6rem_minmax(0,1fr)_5rem_minmax(0,10rem)] items-center gap-x-3 gap-y-2 border-b border-chassis-800 px-6 py-3"
  >
    <MetaField
      v-model="release.artist"
      :label="t('editor.albumArtist')"
      @change="emit('artistChanged')"
    />
    <MetaField v-model="release.title" :label="t('editor.title')" />
    <MetaField v-model="release.genre" :label="t('editor.genre')" />
    <MetaField v-model="year" :label="t('editor.year')" mono />

    <MetaField v-model="release.composer" :label="t('editor.composer')" />
    <MetaField v-model="release.style" :label="t('editor.style')" />
    <MetaField v-model="release.label" :label="t('editor.label')" />
    <MetaField v-model="discs" :label="t('editor.disc')" mono />

    <MetaField v-model="release.barcode" :label="t('editor.barcode')" mono />
    <MetaField v-model="release.comment" :label="t('editor.comment')" :span="3" />

    <span class="truncate text-right text-[0.625rem] uppercase tracking-[0.14em] text-etch-600">
      {{ t('editor.compilation') }}
    </span>
    <input v-model="release.compilation" type="checkbox" class="size-3.5 accent-brass-500" />
  </section>
</template>
