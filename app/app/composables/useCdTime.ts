/** A CD is addressed in frames of 1/75 of a second. */
const FRAMES_PER_SECOND = 75

/**
 * Formats sector counts as playing time. Digits go through `Intl` so that
 * locales using other numerals still read correctly.
 */
export function useCdTime() {
  const { locale } = useI18n()

  const minutes = computed(() => new Intl.NumberFormat(locale.value, { useGrouping: false }))
  const seconds = computed(
    () => new Intl.NumberFormat(locale.value, { minimumIntegerDigits: 2, useGrouping: false }),
  )

  function fromFrames(frames: number) {
    const total = Math.round(frames / FRAMES_PER_SECOND)
    return `${minutes.value.format(Math.floor(total / 60))}:${seconds.value.format(total % 60)}`
  }

  function secondsFromFrames(frames: number) {
    return Math.round(frames / FRAMES_PER_SECOND)
  }

  return { fromFrames, secondsFromFrames }
}
