import { invoke, isTauri } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

import type { Settings } from '~/types/disc'

/**
 * What the user has decided, kept between runs. Saved as soon as it changes,
 * because a settings panel with its own save button is a settings panel people
 * forget to press.
 */
export function useSettings() {
  const settings = useState<Settings | null>('settings', () => null)
  const tokens = useState<string[]>('naming-tokens', () => [])

  async function load() {
    if (!isTauri() || settings.value) {
      return
    }

    settings.value = await invoke<Settings>('get_settings')
    tokens.value = await invoke<string[]>('naming_tokens')
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

  return { settings, tokens, load, save, chooseRoot, clearRoot }
}
