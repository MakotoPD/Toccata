export default defineI18nConfig(() => ({
  legacy: false,
  pluralRules: {
    // Polish has three forms and the default English rule cannot pick between
    // them. Adding a language with its own rule means adding an entry here and
    // nothing else.
    pl: (choice: number) => {
      if (choice === 1) {
        return 0
      }

      const lastDigit = choice % 10
      const lastTwoDigits = choice % 100
      const few = lastDigit >= 2 && lastDigit <= 4 && (lastTwoDigits < 12 || lastTwoDigits > 14)

      return few ? 1 : 2
    },
  },
}))
