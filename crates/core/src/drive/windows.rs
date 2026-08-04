// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Devices::Cdrom::{
    CDDA, CDROM_READ_TOC_EX, CDROM_TOC, IOCTL_CDROM_RAW_READ, IOCTL_CDROM_READ_TOC_EX,
    RAW_READ_INFO, TRACK_DATA,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_NOT_READY, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetDriveTypeW, GetLogicalDrives, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::IOCTL_STORAGE_EJECT_MEDIA;
use windows::Win32::System::WindowsProgramming::DRIVE_CDROM;
use windows::core::PCWSTR;

use super::{BYTES_PER_SECTOR, Drive, DriveError, DriveInfo};
use crate::toc::{Toc, TocEntry};

/// Track number the TOC uses for the lead-out entry.
const LEAD_OUT_TRACK: u8 = 0xaa;

/// Data sector size, which is what the raw read request counts its offset in
/// even though the sectors it returns are 2352 byte audio ones.
const COOKED_SECTOR_SIZE: i64 = 2048;

/// One transfer may not exceed 64 KiB, and 2352 goes into that 27 times.
/// Asking for 28 fails outright with an invalid parameter rather than a short
/// read, so requests are split here instead.
const MAX_SECTORS_PER_READ: u32 = 27;

pub fn list() -> Vec<DriveInfo> {
    let mask = unsafe { GetLogicalDrives() };

    (0..26u32)
        .filter(|bit| mask & (1 << bit) != 0)
        .filter_map(|bit| {
            let letter = char::from(b'A' + bit as u8);
            let root = wide(&format!("{letter}:\\"));

            if unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } != DRIVE_CDROM {
                return None;
            }

            Some(DriveInfo {
                id: format!(r"\\.\{letter}:"),
                name: format!("{letter}:"),
            })
        })
        .collect()
}

pub fn open(id: &str) -> Result<Box<dyn Drive>, DriveError> {
    let info = list()
        .into_iter()
        .find(|drive| drive.id == id)
        .ok_or_else(|| DriveError::NotFound {
            device: id.to_owned(),
        })?;

    let path = wide(id);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|error| map_error(id, "CreateFile", &error))?;

    Ok(Box::new(WindowsDrive { info, handle }))
}

struct WindowsDrive {
    info: DriveInfo,
    handle: HANDLE,
}

impl Drive for WindowsDrive {
    fn info(&self) -> &DriveInfo {
        &self.info
    }

    fn read_toc(&mut self) -> Result<Toc, DriveError> {
        // Format in the low nibble, Msf in the top bit. Both zero asks for the
        // plain TOC with sector addresses, which is what the rest of the code
        // expects; the MSF form would need converting back and is one more
        // place to lose the 150 frame lead-in.
        let request = CDROM_READ_TOC_EX {
            _bitfield: 0,
            SessionTrack: 1,
            ..Default::default()
        };

        let mut raw = CDROM_TOC::default();
        unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_CDROM_READ_TOC_EX,
                Some(&request as *const _ as *const c_void),
                size_of::<CDROM_READ_TOC_EX>() as u32,
                Some(&mut raw as *mut _ as *mut c_void),
                size_of::<CDROM_TOC>() as u32,
                None,
                None,
            )
        }
        .map_err(|error| map_error(&self.info.id, "IOCTL_CDROM_READ_TOC_EX", &error))?;

        parse_toc(&self.info.id, &raw)
    }

    fn read_audio(&mut self, start: u32, sectors: u32, into: &mut [u8]) -> Result<(), DriveError> {
        debug_assert!(into.len() >= sectors as usize * BYTES_PER_SECTOR);

        let mut done = 0;
        while done < sectors {
            let batch = (sectors - done).min(MAX_SECTORS_PER_READ);
            let wanted = batch as usize * BYTES_PER_SECTOR;
            let offset = done as usize * BYTES_PER_SECTOR;

            let request = RAW_READ_INFO {
                // Counted in cooked sectors even when reading raw audio, which
                // is the one surprising thing about this call.
                DiskOffset: i64::from(start + done) * COOKED_SECTOR_SIZE,
                SectorCount: batch,
                TrackMode: CDDA,
            };

            let mut returned = 0u32;
            let failed = |status| DriveError::UnreadableAudio {
                device: self.info.id.clone(),
                start: start + done,
                sectors: batch,
                status,
            };

            unsafe {
                DeviceIoControl(
                    self.handle,
                    IOCTL_CDROM_RAW_READ,
                    Some(&request as *const _ as *const c_void),
                    size_of::<RAW_READ_INFO>() as u32,
                    Some(into[offset..].as_mut_ptr().cast()),
                    wanted as u32,
                    Some(&mut returned),
                    None,
                )
            }
            .map_err(|error| failed(error.code().0 & 0xffff))?;

            if returned as usize != wanted {
                return Err(failed(0));
            }

            done += batch;
        }

        Ok(())
    }

    fn eject(&mut self) -> Result<(), DriveError> {
        unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_STORAGE_EJECT_MEDIA,
                None,
                0,
                None,
                0,
                None,
                None,
            )
        }
        .map_err(|error| map_error(&self.info.id, "IOCTL_STORAGE_EJECT_MEDIA", &error))
    }
}

impl Drop for WindowsDrive {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

fn parse_toc(device: &str, raw: &CDROM_TOC) -> Result<Toc, DriveError> {
    // The length field counts the bytes that follow it, which includes the
    // first and last track numbers.
    let payload = usize::from(u16::from_be_bytes(raw.Length)).saturating_sub(2);
    let reported = (payload / size_of::<TRACK_DATA>()).min(raw.TrackData.len());

    let mut entries = Vec::with_capacity(reported);
    let mut lead_out = None;

    for entry in &raw.TrackData[..reported] {
        let address = u32::from_be_bytes(entry.Address);

        if entry.TrackNumber == LEAD_OUT_TRACK {
            lead_out = Some(address);
        } else {
            entries.push(TocEntry {
                number: entry.TrackNumber,
                start: address,
                // Adr sits in the high nibble, control in the low one.
                control: entry._bitfield & 0x0f,
            });
        }
    }

    let lead_out = lead_out.ok_or_else(|| DriveError::NoDisc {
        device: device.to_owned(),
    })?;

    Toc::from_entries(&entries, lead_out).map_err(|reason| DriveError::UnreadableToc {
        device: device.to_owned(),
        reason,
    })
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn map_error(device: &str, operation: &'static str, error: &windows::core::Error) -> DriveError {
    let code = (error.code().0 & 0xffff) as u32;

    if code == ERROR_NOT_READY.0 {
        DriveError::NoDisc {
            device: device.to_owned(),
        }
    } else if code == ERROR_ACCESS_DENIED.0 {
        DriveError::PermissionDenied {
            device: device.to_owned(),
            group: None,
        }
    } else {
        DriveError::Io {
            device: device.to_owned(),
            operation,
            status: code as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(track: u8, control: u8, address: u32) -> TRACK_DATA {
        TRACK_DATA {
            Reserved: 0,
            // Adr 1 in the high nibble, control in the low one.
            _bitfield: 0x10 | control,
            TrackNumber: track,
            Reserved1: 0,
            Address: address.to_be_bytes(),
        }
    }

    fn toc_with(entries: &[TRACK_DATA]) -> CDROM_TOC {
        let mut raw = CDROM_TOC {
            // Two bytes for the track numbers plus the descriptors.
            Length: ((2 + size_of_val(entries)) as u16).to_be_bytes(),
            FirstTrack: 1,
            LastTrack: entries.len().saturating_sub(1) as u8,
            TrackData: [TRACK_DATA::default(); 100],
        };
        raw.TrackData[..entries.len()].copy_from_slice(entries);
        raw
    }

    #[test]
    fn reads_addresses_and_control_bits() {
        let raw = toc_with(&[
            descriptor(1, 0b0001, 0),
            descriptor(2, 0b0100, 10_000),
            descriptor(LEAD_OUT_TRACK, 0, 25_000),
        ]);

        let toc = parse_toc(r"\\.\E:", &raw).expect("a well formed toc");

        assert_eq!(toc.lead_out, 25_000);
        assert_eq!(toc.tracks.len(), 2);
        assert_eq!(toc.tracks[0].start, 0);
        assert!(toc.tracks[0].pre_emphasis);
        assert!(toc.tracks[0].audio);
        assert_eq!(toc.tracks[1].length, 15_000);
        assert!(!toc.tracks[1].audio);
    }

    #[test]
    fn a_toc_without_a_lead_out_means_no_disc() {
        let raw = toc_with(&[descriptor(1, 0, 0)]);

        assert!(matches!(
            parse_toc(r"\\.\E:", &raw),
            Err(DriveError::NoDisc { .. })
        ));
    }
}
