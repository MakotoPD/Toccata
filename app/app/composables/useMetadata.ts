import { invoke, isTauri } from '@tauri-apps/api/core'

import type { LookupReport, MetadataFault, ReleaseCandidate, SourceId } from '~/types/disc'

/**
 * Candidate pressings for the disc on screen. One Disc ID mapping to several
 * releases is ordinary, so nothing is ever picked automatically unless there
 * is genuinely nothing to pick between.
 */
export function useMetadata() {
  const { t } = useI18n()

  const candidates = useState<ReleaseCandidate[]>('metadata-candidates', () => [])
  /** Hits from a search the user ran, kept apart from what the cascade found. */
  const results = useState<ReleaseCandidate[]>('metadata-results', () => [])
  const failures = useState<MetadataFault[]>('metadata-failures', () => [])
  const selectedId = useState<string | null>('metadata-selected', () => null)
  const searching = useState('metadata-searching', () => false)
  /** Whether a manual search has been run, so an empty list means something. */
  const ranSearch = useState('metadata-ran-search', () => false)
  const searched = useState('metadata-searched', () => false)
  /** Data URI for the chosen release, fetched by the backend. */
  const cover = useState<string | null>('metadata-cover', () => null)

  const release = computed(
    () => candidates.value.find((candidate) => candidate.id === selectedId.value) ?? null,
  )

  const failureMessages = computed(() =>
    failures.value.map((failure) =>
      t(`error.metadata.${failure.code}`, {
        ...failure,
        // Service names are proper nouns, but they still go through i18n so a
        // locale can transliterate them if it needs to.
        source: t(`source.${failure.sourceId}`),
      }),
    ),
  )

  function reset() {
    candidates.value = []
    results.value = []
    failures.value = []
    selectedId.value = null
    searched.value = false
    ranSearch.value = false
    cover.value = null
  }

  async function lookup() {
    if (!isTauri() || searching.value) {
      return
    }

    searching.value = true
    candidates.value = []
    results.value = []
    failures.value = []
    selectedId.value = null
    cover.value = null

    try {
      const report = await invoke<LookupReport>('lookup_metadata')
      candidates.value = report.candidates
      failures.value = report.failures

      // With a single candidate there is nothing to choose between; with more
      // than one the decision is the user's and stays unmade.
      if (report.candidates.length === 1) {
        await select(report.candidates[0]!.id)
      }
    } finally {
      searching.value = false
      searched.value = true
    }
  }

  async function select(id: string) {
    selectedId.value = id
    cover.value = null

    const url = release.value?.coverArt
    if (!url || !isTauri()) {
      return
    }

    try {
      cover.value = await invoke<string | null>('fetch_cover', { url })
    } catch (error) {
      // A missing or unreachable cover is not worth interrupting anything for.
      failures.value = [...failures.value, error as MetadataFault]
    }
  }

  /**
   * Searching by hand is a normal way in, not a last resort, so it never waits
   * for the cascade to have failed first.
   */
  async function search(artist: string, title: string, barcode: string) {
    if (!isTauri() || searching.value) {
      return
    }

    searching.value = true
    results.value = []

    try {
      results.value = await invoke<ReleaseCandidate[]>('search_releases', {
        artist,
        title,
        barcode,
      })
      ranSearch.value = true
    } catch (error) {
      failures.value = [...failures.value, error as MetadataFault]
    } finally {
      searching.value = false
    }
  }

  /**
   * Turns a search hit, or a pasted address, into the release in use. Search
   * results carry no tracks, so the full record has to be fetched.
   */
  async function adopt(reference: string, sourceId: SourceId | null = null) {
    if (!isTauri() || searching.value) {
      return false
    }

    searching.value = true

    try {
      const found = await invoke<ReleaseCandidate | null>('fetch_release', {
        reference,
        sourceId,
      })
      if (!found) {
        return false
      }

      candidates.value = [found]
      results.value = []
      searched.value = true
      await select(found.id)
      return true
    } catch (error) {
      failures.value = [...failures.value, error as MetadataFault]
      return false
    } finally {
      searching.value = false
    }
  }

  /**
   * Stores the release under the Disc ID of whatever is in the drive, so the
   * same disc is recognised straight away next time. The backend takes the
   * identifier from the table of contents rather than from here.
   */
  async function keep(release: ReleaseCandidate) {
    if (!isTauri()) {
      return
    }

    try {
      await invoke('save_release', { release })
      candidates.value = [release]
      results.value = []
      searched.value = true
      await select(release.id)
    } catch (error) {
      failures.value = [...failures.value, error as MetadataFault]
    }
  }

  async function discard() {
    if (!isTauri()) {
      return
    }

    try {
      await invoke('forget_release')
    } catch (error) {
      failures.value = [...failures.value, error as MetadataFault]
    }
  }

  return {
    candidates,
    results,
    ranSearch,
    keep,
    discard,
    failures,
    failureMessages,
    selectedId,
    release,
    cover,
    searching,
    searched,
    lookup,
    reset,
    select,
    search,
    adopt,
  }
}
