<script setup lang="ts">
import type { FormatInfo, Quality } from '~/types/disc'

defineProps<{ busy: boolean }>()

const { t } = useI18n()
const { settings, formats, toggleFormat, setQuality, qualityOf } = useSettings()

/** Rates worth offering. Finer steps than these are not audible off a CD. */
const BITRATES = [96, 128, 160, 192, 224, 256, 320]

const chosen = computed(() => settings.value?.formats ?? [])
const multiple = computed(() => chosen.value.length > 1)

/** Only the formats being written have anything to set. */
const shown = computed(() => formats.value.filter((entry) => chosen.value.includes(entry.id)))

function mode(info: FormatInfo): Quality['mode'] {
  return qualityOf(info)?.mode ?? 'bitrate'
}

function kbps(info: FormatInfo) {
  const quality = qualityOf(info)
  return quality?.mode === 'bitrate' ? quality.kbps : null
}

function variable(info: FormatInfo) {
  const quality = qualityOf(info)
  if (quality?.mode === 'variable') {
    return quality.quality
  }

  return info.tuning.kind === 'lossy' ? info.tuning.defaultQuality : 0
}

function level(info: FormatInfo) {
  const quality = qualityOf(info)
  if (quality?.mode === 'compression') {
    return quality.level
  }

  return info.tuning.kind === 'compression' ? info.tuning.default : 0
}

/** Switching between a fixed rate and the codec's own scale. */
function setMode(info: FormatInfo, next: Quality['mode']) {
  if (info.tuning.kind !== 'lossy') {
    return
  }

  void setQuality(
    info.id,
    next === 'bitrate'
      ? { mode: 'bitrate', kbps: kbps(info) ?? info.tuning.defaultKbps }
      : { mode: 'variable', quality: variable(info) },
  )
}

const row = 'flex items-center gap-3'
const label = 'w-32 shrink-0 text-[0.625rem] uppercase tracking-[0.14em] text-etch-600'
const control =
  'min-w-0 flex-1 rounded-xs border border-chassis-700 bg-chassis-950 px-2 py-1 text-[0.8125rem] text-etch-100 transition-colors hover:border-chassis-600 focus:border-brass-500 focus-visible:outline-none disabled:text-etch-600'
</script>

<template>
  <div class="space-y-5">
    <!-- With several formats chosen, this is also where they are chosen: the
         list in the panel only says how many, not which. -->
    <section v-if="multiple">
      <p class="mb-2 text-[0.625rem] uppercase tracking-[0.16em] text-etch-600">
        {{ t('encoder.formats') }}
      </p>

      <ul class="grid grid-cols-2 gap-x-4 gap-y-1">
        <li v-for="entry in formats" :key="entry.id">
          <label class="flex items-center gap-2">
            <input
              type="checkbox"
              class="size-3.5 shrink-0 accent-brass-500"
              :checked="chosen.includes(entry.id)"
              :disabled="busy"
              @change="toggleFormat(entry.id)"
            />
            <span class="truncate text-[0.8125rem] text-etch-100">{{ entry.label }}</span>
          </label>
        </li>
      </ul>
    </section>

    <section v-for="info in shown" :key="info.id" class="space-y-2">
      <p class="text-[0.625rem] uppercase tracking-[0.16em] text-brass-400">
        {{ t('encoder.settingsOf', { format: info.label }) }}
      </p>

      <!-- Lossless and compressed: one knob, and it costs nothing but time. -->
      <template v-if="info.tuning.kind === 'compression'">
        <div :class="row">
          <span :class="label">{{ t('encoder.compression') }}</span>
          <select
            :value="level(info)"
            :disabled="busy"
            :class="control"
            @change="
              setQuality(info.id, {
                mode: 'compression',
                level: Number(($event.target as HTMLSelectElement).value),
              })
            "
          >
            <option v-for="step in info.tuning.max + 1" :key="step" :value="step - 1">
              {{
                t('encoder.level', { level: step - 1 }) +
                (step - 1 === info.tuning.default ? ` — ${t('encoder.recommended')}` : '')
              }}
            </option>
          </select>
        </div>
        <p class="text-[0.6875rem] leading-relaxed text-etch-600">
          {{ t('encoder.compressionHint') }}
        </p>
      </template>

      <!-- Lossy: a fixed rate, or the codec's own scale where it has one. -->
      <template v-else-if="info.tuning.kind === 'lossy'">
        <div v-if="info.tuning.maxQuality > 0" :class="row">
          <span :class="label">{{ t('encoder.mode') }}</span>
          <select
            :value="mode(info)"
            :disabled="busy"
            :class="control"
            @change="setMode(info, ($event.target as HTMLSelectElement).value as Quality['mode'])"
          >
            <option value="variable">{{ t('encoder.variable') }}</option>
            <option value="bitrate">{{ t('encoder.constant') }}</option>
          </select>
        </div>

        <div v-if="mode(info) === 'variable' && info.tuning.maxQuality > 0" :class="row">
          <span :class="label">{{ t('encoder.quality') }}</span>
          <input
            type="range"
            min="0"
            :max="info.tuning.maxQuality"
            :value="variable(info)"
            :disabled="busy"
            class="min-w-0 flex-1 accent-brass-500"
            @change="
              setQuality(info.id, {
                mode: 'variable',
                quality: Number(($event.target as HTMLInputElement).value),
              })
            "
          />
          <span class="w-10 shrink-0 text-right font-mono text-[0.75rem] tabular-nums text-etch-100">
            {{ variable(info) }}
          </span>
        </div>

        <div v-else :class="row">
          <span :class="label">{{ t('encoder.bitrate') }}</span>
          <select
            :value="kbps(info) ?? info.tuning.defaultKbps"
            :disabled="busy"
            :class="control"
            @change="
              setQuality(info.id, {
                mode: 'bitrate',
                kbps: Number(($event.target as HTMLSelectElement).value),
              })
            "
          >
            <option v-for="rate in BITRATES" :key="rate" :value="rate">{{ rate }} kbps</option>
          </select>
        </div>
      </template>

      <!-- Nothing to decide, and saying so is more useful than an empty tab.
           The fields are shown disabled because what they would offer is
           conversion, and a rip that converts is no longer a rip. -->
      <template v-else>
        <div :class="row">
          <span :class="label">{{ t('encoder.bitDepth') }}</span>
          <input :value="t('encoder.asDisc')" readonly disabled :class="control" />
        </div>
        <div :class="row">
          <span :class="label">{{ t('encoder.sampleRate') }}</span>
          <input :value="t('encoder.asDisc')" readonly disabled :class="control" />
        </div>
        <div :class="row">
          <span :class="label">{{ t('encoder.channels') }}</span>
          <input :value="t('encoder.asDisc')" readonly disabled :class="control" />
        </div>
        <p class="text-[0.6875rem] leading-relaxed text-etch-600">
          {{ t('encoder.untouchedHint') }}
        </p>
      </template>
    </section>
  </div>
</template>
