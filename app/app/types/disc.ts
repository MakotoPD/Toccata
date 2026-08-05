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

export type SourceId =
  'manual' | 'musicBrainz' | 'ctdb' | 'discogs' | 'coverArtArchive' | 'itunes' | 'deezer'

export interface TrackMetadata {
  number: number
  title: string
  artist: string
  lengthMs: number | null
}

export interface Medium {
  position: number
  title: string | null
  format: string | null
  tracks: TrackMetadata[]
}

export interface ReleaseCandidate {
  sourceId: SourceId
  /** Set by sources that aggregate other databases rather than curate one. */
  relayedFrom: string | null
  id: string
  title: string
  artist: string
  date: string | null
  country: string | null
  label: string | null
  barcode: string | null
  disambiguation: string | null
  /** Tag fields no source reliably provides, filled in by hand when empty. */
  genre: string | null
  style: string | null
  composer: string | null
  comment: string | null
  /** Tracks by different artists, which changes how players group the album. */
  compilation: boolean
  discNumber: number
  discTotal: number | null
  /** Tracks on each disc of the release; search hits carry no tracks at all. */
  mediumTrackCounts: number[]
  /** Every disc of the release; empty on a search hit until it is fetched. */
  media: Medium[]
  coverArt: string | null
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

export type TrackStatus = 'waiting' | 'reading' | 'done' | 'failed'

export type RipFault =
  | { code: 'noSuchTrack'; number: number }
  | { code: 'notAudio'; number: number }
  | { code: 'drive' }
  | { code: 'encode' }
  | { code: 'write' }
  | { code: 'cancelled' }

export type RipEvent =
  | { event: 'started'; track: number; position: number; of: number; file: string }
  | { event: 'progress'; track: number; sectors: number; of: number }
  | { event: 'finished'; track: number; unreadableSectors: number }
  | { event: 'failed'; track: number; reason: RipFault }
  | { event: 'done'; folder: string; tracks: number; unreadableSectors: number }

export interface Artwork {
  sourceId: SourceId
  thumbnail: string
  full: string
  kind: string | null
  width: number | null
  height: number | null
}

/** Names of formats are never translated, so these double as their labels. */
export type Format = 'flac' | 'wav'

export interface Settings {
  /** Null means the system music folder, resolved by the backend. */
  outputRoot: string | null
  /** Folders and file name together, with `/` between them. */
  pattern: string
  format: Format
  driveOffsets: Record<string, number>
}
