import { invoke, isTauri } from '@tauri-apps/api/core'

/**
 * Listening to a track before ripping it.
 *
 * The backend hands back the opening of the track as WAV bytes, which become a
 * blob the browser can play. Only one preview exists at a time: the disc can
 * only be read by one reader anyway, so a second one would have to wait.
 */
export function usePreview() {
  const playing = ref<number | null>(null)
  const loading = ref<number | null>(null)

  let element: HTMLAudioElement | null = null
  let url: string | null = null

  function stop() {
    element?.pause()
    playing.value = null

    if (url) {
      URL.revokeObjectURL(url)
      url = null
    }
  }

  async function play(driveId: string | null, number: number) {
    // The same button stops what it started.
    if (playing.value === number) {
      stop()
      return
    }

    if (!driveId || !isTauri() || loading.value !== null) {
      return
    }

    stop()
    loading.value = number

    try {
      const audio = await invoke<ArrayBuffer>('preview_track', { driveId, number })

      url = URL.createObjectURL(new Blob([audio], { type: 'audio/wav' }))
      element ??= new Audio()
      element.onended = stop
      element.src = url
      await element.play()

      playing.value = number
    } catch {
      stop()
    } finally {
      loading.value = null
    }
  }

  onScopeDispose(stop)

  return { playing, loading, play, stop }
}
