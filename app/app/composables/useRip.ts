import { Channel, invoke, isTauri } from '@tauri-apps/api/core'

import type { ReleaseCandidate, RipEvent, RipFault, TrackStatus } from '~/types/disc'

/**
 * Drives one rip and follows it. Progress arrives on a channel opened for this
 * rip alone, so nothing has to work out which run an event belongs to.
 */
export function useRip() {
  const { t } = useI18n()

  const running = useState('rip-running', () => false)
  const track = useState<number | null>('rip-track', () => null)
  const position = useState('rip-position', () => 0)
  const trackCount = useState('rip-track-count', () => 0)
  const sectors = useState('rip-sectors', () => 0)
  const sectorCount = useState('rip-sector-count', () => 0)
  const folder = useState<string | null>('rip-folder', () => null)
  const unreadable = useState('rip-unreadable', () => 0)
  const fault = useState<RipFault | null>('rip-fault', () => null)
  /** Per track, so the list can show where the rip has got to. */
  const statuses = useState<Record<number, TrackStatus>>('rip-statuses', () => ({}))

  /** Progress within the track being read, as a fraction. */
  const trackShare = computed(() => (sectorCount.value ? sectors.value / sectorCount.value : 0))

  /** Progress across the whole disc, counting finished tracks as whole. */
  const discShare = computed(() => {
    if (!trackCount.value) {
      return 0
    }

    return (position.value - 1 + trackShare.value) / trackCount.value
  })

  const faultMessage = computed(() => {
    const value = fault.value
    if (!value) {
      return null
    }

    switch (value.code) {
      case 'noSuchTrack':
      case 'notAudio':
        return t(`error.rip.${value.code}`, { ...value })
      case 'drive':
        return t('error.rip.drive')
      case 'encode':
      case 'write':
      case 'cancelled':
        return t(`error.rip.${value.code}`)
      default:
        return t('error.unknown')
    }
  })

  function statusOf(number: number): TrackStatus | null {
    return statuses.value[number] ?? null
  }

  function reset() {
    statuses.value = {}
    track.value = null
    position.value = 0
    trackCount.value = 0
    sectors.value = 0
    sectorCount.value = 0
    folder.value = null
    unreadable.value = 0
    fault.value = null
  }

  async function start(
    driveId: string,
    release: ReleaseCandidate | null,
    tracks: number[],
    cover: string | null = null,
  ) {
    if (!isTauri() || running.value) {
      return
    }

    reset()
    running.value = true
    for (const number of tracks) {
      statuses.value[number] = 'waiting'
    }

    const channel = new Channel<RipEvent>()
    channel.onmessage = (message) => {
      switch (message.event) {
        case 'started':
          statuses.value[message.track] = 'reading'
          track.value = message.track
          position.value = message.position
          trackCount.value = message.of
          sectors.value = 0
          sectorCount.value = 0
          break
        case 'progress':
          sectors.value = message.sectors
          sectorCount.value = message.of
          break
        case 'finished':
          statuses.value[message.track] = 'done'
          unreadable.value += message.unreadableSectors
          break
        case 'failed':
          statuses.value[message.track] = 'failed'
          fault.value = message.reason
          break
        case 'done':
          folder.value = message.folder
          unreadable.value = message.unreadableSectors
          break
      }
    }

    try {
      await invoke('rip_disc', {
        driveId,
        release,
        tracks,
        // What gets embedded is the cover on screen, not a second download.
        cover,
        channel,
      })
    } catch (error) {
      fault.value = error as RipFault
    } finally {
      running.value = false
      track.value = null
    }
  }

  async function cancel() {
    if (isTauri()) {
      await invoke('cancel_rip')
    }
  }

  return {
    running,
    statusOf,
    track,
    position,
    trackCount,
    trackShare,
    discShare,
    folder,
    unreadable,
    fault,
    faultMessage,
    reset,
    start,
    cancel,
  }
}
