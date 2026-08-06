import { invoke, isTauri } from '@tauri-apps/api/core'

/** How often the tray is asked. Often enough to feel immediate, rarely enough
 *  that the drive is left alone. */
const INTERVAL = 2000

/**
 * Watches the drive so that putting a disc in is all anybody has to do.
 *
 * The question asked is only "is there something in the tray", which is a test
 * unit ready underneath and does not spin the disc up. Reading the table of
 * contents on a timer would keep the drive awake and wear it out for nothing.
 */
export function useDiscWatch(
  driveId: Ref<string | null>,
  busy: Ref<boolean>,
  onInserted: () => void | Promise<void>,
) {
  /** Nothing is done on the first answer: a disc already in the tray when the
   *  window opens was not just put there, and reading it unasked would be a
   *  surprise rather than a convenience. */
  let present: boolean | null = null
  let timer: ReturnType<typeof setInterval> | null = null

  async function look() {
    if (!isTauri() || !driveId.value || busy.value) {
      return
    }

    const now = await invoke<boolean>('disc_present', { driveId: driveId.value })

    if (present !== null && now && !present) {
      await onInserted()
    }

    present = now
  }

  /** A drive change makes the last answer meaningless. */
  watch(driveId, () => {
    present = null
  })

  onMounted(() => {
    timer = setInterval(look, INTERVAL)
  })

  onScopeDispose(() => {
    if (timer) {
      clearInterval(timer)
    }
  })

  return { look }
}
