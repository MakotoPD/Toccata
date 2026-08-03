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

export type SourceId = 'musicBrainz'

export interface TrackMetadata {
  number: number
  title: string
  artist: string
  lengthMs: number | null
}

export interface ReleaseCandidate {
  sourceId: SourceId
  id: string
  title: string
  artist: string
  date: string | null
  country: string | null
  label: string | null
  barcode: string | null
  disambiguation: string | null
  discNumber: number
  discTotal: number | null
  tracks: TrackMetadata[]
}

export type MetadataFault =
  | { code: 'unreachable'; sourceId: SourceId }
  | { code: 'rejected'; sourceId: SourceId; status: number }
  | { code: 'unreadable'; sourceId: SourceId }

export interface LookupReport {
  candidates: ReleaseCandidate[]
  failures: MetadataFault[]
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
