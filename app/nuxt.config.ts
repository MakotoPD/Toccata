export default defineNuxtConfig({
  modules: ['@nuxt/eslint'],
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
    server: { strictPort: true },
  },
})
