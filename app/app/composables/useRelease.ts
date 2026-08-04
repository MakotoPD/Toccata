import type { ReleaseCandidate, Toc, TrackMetadata } from '~/types/disc'

/**
 * The metadata actually on screen, which is always a working copy rather than
 * whatever a database said. Sources fill it in, the user overrides it, and the
 * rip and the tags both read from here.
 */
export function useRelease() {
  const draft = useState<ReleaseCandidate | null>('release-draft', () => null)
  /** Track the detail panel is editing; null means the release itself. */
  const selected = useState<number | null>('release-selected', () => null)
  /** Track numbers to extract. Everything is included until told otherwise. */
  const excluded = useState<number[]>('release-excluded', () => [])

  const selectedTrack = computed(() =>
    selected.value === null
      ? null
      : (draft.value?.tracks.find((track) => track.number === selected.value) ?? null),
  )

  function isIncluded(number: number) {
    return !excluded.value.includes(number)
  }

  function toggle(number: number) {
    excluded.value = isIncluded(number)
      ? [...excluded.value, number]
      : excluded.value.filter((entry) => entry !== number)
  }

  function includedNumbers(toc: Toc | null) {
    return (toc?.tracks ?? [])
      .filter((track) => track.audio && isIncluded(track.number))
      .map((track) => track.number)
  }

  /** Rows follow the disc, never the track count a database happened to have. */
  function rowsFor(toc: Toc | null, from: TrackMetadata[], artist: string): TrackMetadata[] {
    return (toc?.tracks ?? [])
      .filter((track) => track.audio)
      .map((track) => {
        const existing = from.find((entry) => entry.number === track.number)

        return {
          number: track.number,
          title: existing?.title ?? '',
          artist: existing?.artist ?? artist,
          lengthMs: existing?.lengthMs ?? null,
        }
      })
  }

  function blank(toc: Toc | null, discId: string): ReleaseCandidate {
    return {
      sourceId: 'manual',
      relayedFrom: null,
      id: discId,
      title: '',
      artist: '',
      date: null,
      country: null,
      label: null,
      barcode: null,
      disambiguation: null,
      genre: null,
      style: null,
      composer: null,
      comment: null,
      compilation: false,
      discNumber: 1,
      discTotal: null,
      mediumTrackCounts: [(toc?.tracks ?? []).filter((track) => track.audio).length],
      coverArt: null,
      tracks: rowsFor(toc, [], ''),
    }
  }

  /** Takes a candidate as the new working copy, sized to the disc in the drive. */
  function adopt(candidate: ReleaseCandidate, toc: Toc | null) {
    draft.value = {
      ...candidate,
      tracks: rowsFor(toc, candidate.tracks, candidate.artist),
    }
  }

  function start(toc: Toc | null, discId: string) {
    draft.value = blank(toc, discId)
  }

  function clear() {
    draft.value = null
    selected.value = null
    excluded.value = []
  }

  /** Saves typing the same name on every row of a single artist album. */
  function spreadArtist() {
    if (!draft.value) {
      return
    }

    for (const track of draft.value.tracks) {
      track.artist = draft.value.artist
    }
  }

  return {
    draft,
    selected,
    selectedTrack,
    excluded,
    isIncluded,
    toggle,
    includedNumbers,
    adopt,
    start,
    clear,
    spreadArtist,
  }
}
