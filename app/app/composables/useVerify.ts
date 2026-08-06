import { invoke, isTauri } from '@tauri-apps/api/core'

import type { TrackVerification } from '~/types/disc'

/**
 * Comparing a finished rip against everyone else's, through CTDB.
 *
 * Asked for after the rip rather than during it. The audio is already on disk
 * and correct whatever comes back, so a service being slow must never be
 * something the user waits on to get their files.
 */
export function useVerify() {
  const results = useState<TrackVerification[]>('verification', () => [])
  const running = useState('verifying', () => false)
  const failed = useState('verify-failed', () => false)

  function reset() {
    results.value = []
    failed.value = false
  }

  async function run() {
    if (!isTauri() || running.value) {
      return
    }

    running.value = true
    failed.value = false

    try {
      results.value = await invoke<TrackVerification[]>('verify_rip')
    } catch {
      // A lookup that did not answer says nothing about the rip, so it is
      // reported as its own state rather than as a verdict on the audio.
      failed.value = true
      results.value = []
    } finally {
      running.value = false
    }
  }

  function of(track: number) {
    return results.value.find((entry) => entry.track === track) ?? null
  }

  return { results, running, failed, run, reset, of }
}
