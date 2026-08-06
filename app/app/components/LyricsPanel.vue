<script setup lang="ts">
const props = defineProps<{
  /** The track the list has selected, or nothing for the disc as a whole. */
  selected: number | null
  busy: boolean
}>()

const { t } = useI18n()
const lyrics = useLyrics()

const entry = computed(() => (props.selected === null ? null : lyrics.of(props.selected)))

/** Edited locally and handed over on blur, so every keystroke is not a save. */
const plain = ref('')
const synced = ref('')

watch(
  entry,
  (value) => {
    plain.value = value?.plain ?? ''
    synced.value = value?.synced ?? ''
  },
  { immediate: true },
)

function keep() {
  if (props.selected !== null) {
    void lyrics.set(props.selected, plain.value, synced.value)
  }
}

const found = computed(() => lyrics.found.value.length)

const field =
  'w-full rounded-xs border border-chassis-700 bg-chassis-950 px-2 py-1.5 font-mono text-[0.6875rem] leading-relaxed text-etch-100 transition-colors hover:border-chassis-600 focus:border-brass-500 focus-visible:outline-none'
const heading = 'mb-1 block text-[0.625rem] uppercase tracking-[0.14em] text-etch-600'
</script>

<template>
  <div class="space-y-3">
    <div class="flex items-center gap-3">
      <p class="text-[0.625rem] uppercase tracking-[0.16em] text-etch-600">
        {{ selected === null ? t('lyrics.heading') : t('panel.forTrack', { number: selected }) }}
      </p>

      <span v-if="lyrics.running.value" class="text-[0.625rem] text-etch-600">
        {{ t('lyrics.searching') }}
      </span>
      <!-- Zero is its own sentence rather than a plural form: Polish has three
           of those and none of them reads well with none. -->
      <span v-else-if="lyrics.searched.value" class="text-[0.625rem] text-etch-600">
        {{ found ? t('lyrics.found', found) : t('lyrics.nothing') }}
      </span>
    </div>

    <!-- Nothing to edit until a track is picked: the words belong to one
         track, not to the disc. -->
    <p v-if="selected === null" class="text-[0.6875rem] leading-relaxed text-etch-600">
      {{ t('lyrics.hint') }}
    </p>

    <template v-else>
      <div>
        <label :class="heading" :for="`lyrics-plain-${selected}`">{{ t('lyrics.plain') }}</label>
        <textarea
          :id="`lyrics-plain-${selected}`"
          v-model="plain"
          rows="6"
          :disabled="busy"
          :class="field"
          :placeholder="t('lyrics.none')"
          @change="keep"
        />
      </div>

      <div>
        <label :class="heading" :for="`lyrics-synced-${selected}`">{{ t('lyrics.synced') }}</label>
        <textarea
          :id="`lyrics-synced-${selected}`"
          v-model="synced"
          rows="5"
          :disabled="busy"
          :class="field"
          :placeholder="t('lyrics.noneSynced')"
          @change="keep"
        />
        <p class="mt-1 text-[0.625rem] leading-relaxed text-etch-600">
          {{ t('lyrics.syncedHint') }}
        </p>
      </div>
    </template>
  </div>
</template>
