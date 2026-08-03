import { invoke, isTauri } from '@tauri-apps/api/core'

import type { LookupReport, MetadataFault, ReleaseCandidate } from '~/types/disc'

/**
 * Candidate pressings for the disc on screen. One Disc ID mapping to several
 * releases is ordinary, so nothing is ever picked automatically unless there
 * is genuinely nothing to pick between.
 */
export function useMetadata() {
  const { t } = useI18n()

  const candidates = useState<ReleaseCandidate[]>('metadata-candidates', () => [])
  const failures = useState<MetadataFault[]>('metadata-failures', () => [])
  const selectedId = useState<string | null>('metadata-selected', () => null)
  const searching = useState('metadata-searching', () => false)
  const searched = useState('metadata-searched', () => false)

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
    failures.value = []
    selectedId.value = null
    searched.value = false
  }

  async function lookup() {
    if (!isTauri() || searching.value) {
      return
    }

    searching.value = true
    candidates.value = []
    failures.value = []
    selectedId.value = null

    try {
      const report = await invoke<LookupReport>('lookup_metadata')
      candidates.value = report.candidates
      failures.value = report.failures

      // With a single candidate there is nothing to choose between; with more
      // than one the decision is the user's and stays unmade.
      if (report.candidates.length === 1) {
        selectedId.value = report.candidates[0]!.id
      }
    } finally {
      searching.value = false
      searched.value = true
    }
  }

  function select(id: string) {
    selectedId.value = id
  }

  return {
    candidates,
    failures,
    failureMessages,
    selectedId,
    release,
    searching,
    searched,
    lookup,
    reset,
    select,
  }
}
