import { invoke, isTauri } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

import type { Format, FormatInfo, Quality, Settings } from '~/types/disc'

/**
 * What the user has decided, kept between runs. Saved as soon as it changes,
 * because a settings panel with its own save button is a settings panel people
 * forget to press.
 */
export function useSettings() {
  const settings = useState<Settings | null>('settings', () => null)
  const tokens = useState<string[]>('naming-tokens', () => [])
  const formats = useState<FormatInfo[]>('formats', () => [])

  async function load() {
    if (!isTauri() || settings.value) {
      return
    }

    settings.value = await invoke<Settings>('get_settings')
    tokens.value = await invoke<string[]>('naming_tokens')
    formats.value = await invoke<FormatInfo[]>('formats')
  }

  /**
   * Turns one format on or off. The last one cannot be turned off: a rip with
   * nothing to write to would run the disc through for no reason at all.
   */
  async function toggleFormat(format: Format) {
    const chosen = settings.value?.formats ?? []
    const next = chosen.includes(format)
      ? chosen.filter((entry) => entry !== format)
      : [...chosen, format]

    if (next.length) {
      await save({ formats: next })
    }
  }

  /** Replaces the whole selection, which is what the single picker does. */
  async function chooseFormat(format: Format) {
    await save({ formats: [format] })
  }

  async function setQuality(format: Format, quality: Quality) {
    await save({ qualities: { ...settings.value?.qualities, [format]: quality } })
  }

  /** What a format is set to, falling back to what the backend says it uses. */
  function qualityOf(info: FormatInfo): Quality | null {
    const stored = settings.value?.qualities?.[info.id]
    if (stored) {
      return stored
    }

    switch (info.tuning.kind) {
      case 'compression':
        return { mode: 'compression', level: info.tuning.default }
      case 'lossy':
        return { mode: 'bitrate', kbps: info.tuning.defaultKbps }
      default:
        return null
    }
  }

  async function save(next: Partial<Settings>) {
    if (!isTauri() || !settings.value) {
      return
    }

    const merged = { ...settings.value, ...next }
    settings.value = merged
    await invoke('set_settings', { settings: merged })
  }

  /** Asks the system for a folder, since typing a path is nobody's idea of fun. */
  async function chooseRoot() {
    if (!isTauri()) {
      return
    }

    const picked = await open({ directory: true, multiple: false })
    if (typeof picked === 'string') {
      await save({ outputRoot: picked })
    }
  }

  /** Back to the music folder the system already has. */
  async function clearRoot() {
    await save({ outputRoot: null })
  }

  return {
    settings,
    tokens,
    formats,
    load,
    save,
    toggleFormat,
    chooseFormat,
    setQuality,
    qualityOf,
    chooseRoot,
    clearRoot,
  }
}
