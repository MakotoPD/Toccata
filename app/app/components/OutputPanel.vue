<script setup lang="ts">
import type { DriveInfo } from '~/types/disc'

defineProps<{
  drives: DriveInfo[]
  driveId: string | null
  /** Where the rip will land, resolved by the backend from the pattern. */
  folder: string | null
  busy: boolean
}>()

const emit = defineEmits<{ selectDrive: [string]; reveal: [] }>()

const { t } = useI18n()
const { settings, tokens, save, chooseRoot, clearRoot } = useSettings()

const pattern = computed({
  get: () => settings.value?.pattern ?? '',
  set: (value: string) => {
    void save({ pattern: value })
  },
})

/** Built here rather than in the template: a brace inside an interpolation
 *  closes it early. */
const placeholders = computed(() => tokens.value.map((token) => `{${token}}`).join(' '))

const row = 'flex items-center gap-2'
const label = 'w-24 shrink-0 text-[0.625rem] uppercase tracking-[0.14em] text-etch-600'
const control =
  'min-w-0 flex-1 rounded-xs border border-chassis-700 bg-chassis-950 px-2 py-1 text-[0.8125rem] text-etch-100 transition-colors hover:border-chassis-600 focus:border-brass-500 focus-visible:outline-none disabled:text-etch-600'
const small =
  'shrink-0 rounded-xs border border-chassis-700 px-2 py-1 text-[0.625rem] uppercase tracking-[0.14em] text-etch-400 transition-colors hover:border-etch-600 hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brass-500 disabled:opacity-40'
</script>

<template>
  <section
    class="flex w-96 shrink-0 flex-col gap-2 overflow-y-auto border-r border-chassis-800 px-6 py-4"
  >
    <div :class="row">
      <span :class="label">{{ t('output.format') }}</span>
      <select :class="control" disabled>
        <option>WAV</option>
      </select>
    </div>

    <div :class="row">
      <span :class="label">{{ t('output.root') }}</span>
      <input
        :value="settings?.outputRoot ?? t('output.rootDefault')"
        type="text"
        readonly
        :title="settings?.outputRoot ?? ''"
        :class="[control, 'cursor-default text-etch-400']"
      />
      <button type="button" :class="small" :disabled="busy" @click="chooseRoot">
        {{ t('output.browse') }}
      </button>
      <button
        type="button"
        :class="small"
        :disabled="busy || !settings?.outputRoot"
        @click="clearRoot"
      >
        {{ t('output.reset') }}
      </button>
    </div>

    <div :class="row">
      <span :class="label">{{ t('output.naming') }}</span>
      <input v-model="pattern" type="text" :class="[control, 'font-mono text-[0.6875rem]']" />
    </div>

    <!-- The placeholders come from the backend, so the list can never drift
         away from the ones the pattern actually understands. -->
    <p class="pl-[6.5rem] font-mono text-[0.625rem] leading-relaxed text-etch-600">
      {{ placeholders }}
    </p>

    <div :class="row">
      <span :class="label">{{ t('output.path') }}</span>
      <input
        :value="folder ?? t('output.pathPending')"
        type="text"
        readonly
        :title="folder ?? ''"
        :class="[control, 'cursor-default text-etch-400']"
      />
      <button type="button" :class="small" :disabled="!folder" @click="emit('reveal')">
        {{ t('output.reveal') }}
      </button>
    </div>

    <div :class="row">
      <span :class="label">{{ t('drive.label') }}</span>
      <select
        :value="driveId ?? ''"
        :disabled="drives.length === 0 || busy"
        :class="control"
        @change="emit('selectDrive', ($event.target as HTMLSelectElement).value)"
      >
        <option v-if="drives.length === 0" value="">{{ t('drive.none') }}</option>
        <option v-for="drive in drives" :key="drive.id" :value="drive.id">
          {{ drive.name }}
        </option>
      </select>
    </div>
  </section>
</template>
