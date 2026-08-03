// Mirrors the payloads in src-tauri/src/main.rs. Generating these from the
// Rust types is worth doing once the rip progress events land, since those
// change far more often than this handful of structs.

export interface DriveInfo {
  id: string
  name: string
}

export interface Track {
  number: number
  /** Sector address of the track start. */
  start: number
  /** Length in frames. */
  length: number
  audio: boolean
  preEmphasis: boolean
}

export interface Toc {
  tracks: Track[]
  leadOut: number
}

export interface Disc {
  drive: DriveInfo
  toc: Toc
  musicbrainzDiscId: string
  freedbId: string
}

export type TocFault =
  | { code: 'empty' }
  | { code: 'outOfOrder'; number: number; start: number; previous: number }
  | { code: 'leadOutTooEarly'; leadOut: number; lastTrackStart: number }

export type DriveFault =
  | { code: 'notFound'; device: string }
  | { code: 'permissionDenied'; device: string; group: string | null }
  | { code: 'noDisc'; device: string }
  | { code: 'notAnAudioDisc'; device: string }
  | { code: 'io'; device: string; operation: string; status: number }
  | { code: 'unreadableToc'; device: string; reason: TocFault }
