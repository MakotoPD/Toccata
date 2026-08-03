import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  modules: ['@nuxt/eslint', '@nuxtjs/i18n'],
  css: ['~/assets/css/main.css'],
  // Desktop shell: no server rendering, the whole app is a static bundle
  // loaded by the Tauri webview.
  ssr: false,
  devtools: { enabled: true },
  compatibilityDate: '2026-08-03',
  eslint: {
    // Formatting belongs to Prettier, not to ESLint.
    config: { stylistic: false },
  },
  vite: {
    clearScreen: false,
    plugins: [tailwindcss()],
    server: { strictPort: true },
  },
  i18n: {
    // Desktop app: the locale never belongs in the URL.
    strategy: 'no_prefix',
    defaultLocale: 'en',
    locales: [
      { code: 'en', language: 'en', name: 'English', file: 'en.json' },
      { code: 'pl', language: 'pl-PL', name: 'Polski', file: 'pl.json' },
    ],
    // The webview reports the system locale, so no Tauri plugin is needed
    // to pick a starting language. The cookie keeps a manual override.
    detectBrowserLanguage: {
      useCookie: true,
      cookieKey: 'toccata_locale',
      fallbackLocale: 'en',
    },
  },
})
