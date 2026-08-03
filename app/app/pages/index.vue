<script setup lang="ts">
import { invoke, isTauri } from '@tauri-apps/api/core'

const { t, locale, locales, setLocale } = useI18n()

// Runs in a plain browser during `nuxt dev`, where there is no backend to ask.
const coreVersion = ref<string | null>(null)
onMounted(async () => {
  if (isTauri()) {
    coreVersion.value = await invoke<string>('core_version')
  }
})
</script>

<template>
  <main
    class="relative grid min-h-full place-items-center bg-chassis-950 bg-[repeating-radial-gradient(circle_at_50%_45%,transparent_0,transparent_11px,rgba(217,180,99,0.045)_12px,transparent_13px)] font-sans text-etch-100"
  >
    <div class="flex flex-col items-center gap-10 px-8">
      <header class="flex flex-col items-center gap-5">
        <h1 class="font-display text-6xl font-normal tracking-[0.2em] indent-[0.2em]">Toccata</h1>
        <div class="h-px w-24 bg-brass-500" />
        <p class="text-xs uppercase tracking-[0.28em] text-etch-400">{{ t('app.tagline') }}</p>
      </header>

      <nav :aria-label="t('settings.language.label')" class="flex items-center gap-1">
        <button
          v-for="option in locales"
          :key="option.code"
          type="button"
          :aria-pressed="option.code === locale"
          class="rounded-xs px-3 py-1 text-xs uppercase tracking-[0.18em] text-etch-600 transition-colors hover:text-etch-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-4 focus-visible:outline-brass-500 aria-pressed:text-brass-400"
          @click="setLocale(option.code)"
        >
          {{ option.code }}
        </button>
      </nav>

      <p v-if="coreVersion" class="font-mono text-[0.6875rem] tracking-wider text-etch-600">
        {{ t('about.core', { version: coreVersion }) }}
      </p>
    </div>
  </main>
</template>
