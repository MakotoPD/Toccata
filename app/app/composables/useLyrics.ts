import { invoke, isTauri } from '@tauri-apps/api/core'

import type { ReleaseCandidate, TrackLyrics } from '~/types/disc'

/**
 * Words for the disc on screen.
 *
 * Fetched before a rip rather than during it, so that what a database offered
 * can be read and corrected before it is written into anything. A track that
 * finds nothing simply has no entry.
 */
export function useLyrics() {
  const found = useState<TrackLyrics[]>('lyrics', () => [])
  const running = useState('lyrics-running', () => false)
  const searched = useState('lyrics-searched', () => false)

  function reset() {
    found.value = []
    searched.value = false
  }

  async function fetchAll(release: ReleaseCandidate | null) {
    if (!isTauri() || !release || running.value) {
      return
    }

    running.value = true
    try {
      found.value = await invoke<TrackLyrics[]>('fetch_lyrics', { release })
    } catch {
      // Nothing found is the normal case for plenty of discs, and a service
      // that did not answer is not worth an error of its own here.
      found.value = []
    } finally {
      running.value = false
      searched.value = true
    }
  }

  function of(track: number) {
    return found.value.find((entry) => entry.track === track) ?? null
  }

  /** Replaces what a database said with what the user decided. */
  async function set(track: number, plain: string, synced: string) {
    const entry: TrackLyrics = {
      track,
      plain: plain.trim() ? plain : null,
      synced: synced.trim() ? synced : null,
      instrumental: false,
    }

    found.value = [...found.value.filter((other) => other.track !== track), entry].sort(
      (left, right) => left.track - right.track,
    )

    if (isTauri()) {
      await invoke('set_lyrics', { entry })
    }
  }

  return { found, running, searched, reset, fetchAll, of, set }
}
