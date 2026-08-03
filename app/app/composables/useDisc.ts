import { invoke, isTauri } from '@tauri-apps/api/core'

import type { Disc, DriveFault, DriveInfo } from '~/types/disc'

/**
 * Everything the disc screen needs: which drives exist, which one is selected
 * and what was last read from it. Faults are kept as the structured value the
 * backend sent, so the message stays the frontend's business.
 */
export function useDisc() {
  const { t } = useI18n()
  const drives = useState<DriveInfo[]>('drives', () => [])
  const selectedId = useState<string | null>('selected-drive', () => null)
  const disc = useState<Disc | null>('disc', () => null)
  const fault = useState<DriveFault | null>('disc-fault', () => null)
  const busy = useState('disc-busy', () => false)

  const selected = computed(() => drives.value.find((drive) => drive.id === selectedId.value))

  // The backend only ever sends a code and its parameters; the sentence is
  // assembled here so that adding a language never means touching Rust.
  const faultMessage = computed(() => {
    const value = fault.value
    if (!value) {
      return null
    }

    switch (value.code) {
      case 'permissionDenied':
        return value.group
          ? t('error.drive.permissionDeniedGroup', { device: value.device, group: value.group })
          : t('error.drive.permissionDenied', { device: value.device })
      case 'unreadableToc':
        return [
          t('error.drive.unreadableToc', { device: value.device }),
          t(`error.toc.${value.reason.code}`, { ...value.reason }),
        ].join(' ')
      case 'notFound':
      case 'noDisc':
      case 'notAnAudioDisc':
      case 'io':
        return t(`error.drive.${value.code}`, { ...value })
      default:
        return t('error.unknown')
    }
  })

  async function refresh() {
    if (!isTauri()) {
      return
    }

    drives.value = await invoke<DriveInfo[]>('list_drives')

    if (!drives.value.some((drive) => drive.id === selectedId.value)) {
      selectedId.value = drives.value[0]?.id ?? null
      disc.value = null
      fault.value = null
    }
  }

  async function read() {
    if (!selectedId.value || busy.value) {
      return
    }

    busy.value = true
    fault.value = null

    try {
      disc.value = await invoke<Disc>('read_disc', { driveId: selectedId.value })
    } catch (error) {
      disc.value = null
      fault.value = error as DriveFault
    } finally {
      busy.value = false
    }
  }

  async function eject() {
    if (!selectedId.value || busy.value) {
      return
    }

    busy.value = true
    fault.value = null

    try {
      await invoke('eject', { driveId: selectedId.value })
      disc.value = null
    } catch (error) {
      fault.value = error as DriveFault
    } finally {
      busy.value = false
    }
  }

  function select(id: string) {
    selectedId.value = id
    disc.value = null
    fault.value = null
  }

  return {
    drives,
    selectedId,
    selected,
    disc,
    fault,
    faultMessage,
    busy,
    refresh,
    read,
    eject,
    select,
  }
}
