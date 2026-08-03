// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{CStr, CString};
use std::fs;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;

use super::{Drive, DriveError, DriveInfo};
use crate::toc::{Toc, TocEntry};

// From linux/cdrom.h.
const CDROMREADTOCHDR: libc::c_ulong = 0x5305;
const CDROMREADTOCENTRY: libc::c_ulong = 0x5306;
const CDROMEJECT: libc::c_ulong = 0x5309;
const CDROM_DRIVE_STATUS: libc::c_ulong = 0x5326;

const CDROM_LBA: u8 = 0x01;
const CDROM_LEADOUT: u8 = 0xaa;
const CDSL_CURRENT: libc::c_int = 0x7fff_ffff;

const CDS_NO_DISC: libc::c_int = 1;
const CDS_TRAY_OPEN: libc::c_int = 2;
const CDS_DRIVE_NOT_READY: libc::c_int = 3;

#[repr(C)]
#[derive(Default)]
struct TocHeader {
    first_track: u8,
    last_track: u8,
}

#[repr(C)]
#[derive(Default)]
struct TocEntryRaw {
    track: u8,
    /// `cdte_adr:4` then `cdte_ctrl:4`, so the control bits are the high
    /// nibble here. Windows packs the same pair the other way round.
    adr_ctrl: u8,
    format: u8,
    address: i32,
    datamode: u8,
}

pub fn list() -> Vec<DriveInfo> {
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return Vec::new();
    };

    let mut drives: Vec<DriveInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let node = entry.file_name().into_string().ok()?;
            if !node.starts_with("sr") {
                return None;
            }

            let describe = |field: &str| {
                fs::read_to_string(entry.path().join("device").join(field))
                    .map(|value| value.trim().to_owned())
                    .unwrap_or_default()
            };

            let name = format!("{} {}", describe("vendor"), describe("model"));
            let name = name.trim();

            Some(DriveInfo {
                id: format!("/dev/{node}"),
                name: if name.is_empty() {
                    format!("/dev/{node}")
                } else {
                    name.to_owned()
                },
            })
        })
        .collect();

    drives.sort_by(|a, b| a.id.cmp(&b.id));
    drives
}

pub fn open(id: &str) -> Result<Box<dyn Drive>, DriveError> {
    let info = list()
        .into_iter()
        .find(|drive| drive.id == id)
        .ok_or_else(|| DriveError::NotFound {
            device: id.to_owned(),
        })?;

    let path = CString::new(id).map_err(|_| DriveError::NotFound {
        device: id.to_owned(),
    })?;

    // O_NONBLOCK matters: without it the open blocks until a disc is loaded.
    let raw = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if raw < 0 {
        return Err(last_error(id, "open"));
    }

    Ok(Box::new(LinuxDrive {
        info,
        fd: unsafe { OwnedFd::from_raw_fd(raw) },
    }))
}

struct LinuxDrive {
    info: DriveInfo,
    fd: OwnedFd,
}

impl Drive for LinuxDrive {
    fn info(&self) -> &DriveInfo {
        &self.info
    }

    fn read_toc(&mut self) -> Result<Toc, DriveError> {
        self.require_disc()?;

        let mut header = TocHeader::default();
        if unsafe { libc::ioctl(self.fd.as_raw_fd(), CDROMREADTOCHDR, &raw mut header) } < 0 {
            return Err(last_error(&self.info.id, "CDROMREADTOCHDR"));
        }

        let mut entries = Vec::new();
        for number in header.first_track..=header.last_track {
            let entry = self.read_entry(number)?;
            entries.push(TocEntry {
                number,
                start: entry.address as u32,
                control: entry.adr_ctrl >> 4,
            });
        }

        let lead_out = self.read_entry(CDROM_LEADOUT)?.address as u32;

        Toc::from_entries(&entries, lead_out).map_err(|reason| DriveError::UnreadableToc {
            device: self.info.id.clone(),
            reason,
        })
    }

    fn eject(&mut self) -> Result<(), DriveError> {
        if unsafe { libc::ioctl(self.fd.as_raw_fd(), CDROMEJECT) } < 0 {
            return Err(last_error(&self.info.id, "CDROMEJECT"));
        }
        Ok(())
    }
}

impl LinuxDrive {
    fn read_entry(&self, number: u8) -> Result<TocEntryRaw, DriveError> {
        let mut entry = TocEntryRaw {
            track: number,
            format: CDROM_LBA,
            ..Default::default()
        };

        if unsafe { libc::ioctl(self.fd.as_raw_fd(), CDROMREADTOCENTRY, &raw mut entry) } < 0 {
            return Err(last_error(&self.info.id, "CDROMREADTOCENTRY"));
        }

        Ok(entry)
    }

    /// Turns the drive's own idea of what is loaded into a specific error,
    /// instead of letting the TOC read fail with a bare errno.
    fn require_disc(&self) -> Result<(), DriveError> {
        let status = unsafe { libc::ioctl(self.fd.as_raw_fd(), CDROM_DRIVE_STATUS, CDSL_CURRENT) };

        match status {
            CDS_NO_DISC | CDS_TRAY_OPEN | CDS_DRIVE_NOT_READY => Err(DriveError::NoDisc {
                device: self.info.id.clone(),
            }),
            _ => Ok(()),
        }
    }
}

fn last_error(device: &str, operation: &'static str) -> DriveError {
    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);

    match errno {
        libc::EACCES | libc::EPERM => DriveError::PermissionDenied {
            device: device.to_owned(),
            group: owning_group(device),
        },
        libc::ENOMEDIUM | libc::ENXIO => DriveError::NoDisc {
            device: device.to_owned(),
        },
        _ => DriveError::Io {
            device: device.to_owned(),
            operation,
            status: errno,
        },
    }
}

/// Name of the group that owns the device node, so the error can tell the user
/// which group to join rather than just saying access was denied.
fn owning_group(device: &str) -> Option<String> {
    let gid = fs::metadata(Path::new(device))
        .ok()
        .map(|metadata| std::os::unix::fs::MetadataExt::gid(&metadata))?;

    // getgrgid returns a pointer into storage that may be reused by the next
    // call, so the name is copied out straight away.
    let entry = unsafe { libc::getgrgid(gid) };
    if entry.is_null() {
        return None;
    }

    let name = unsafe { CStr::from_ptr((*entry).gr_name) };
    name.to_str().ok().map(str::to_owned)
}
