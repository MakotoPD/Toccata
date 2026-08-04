// SPDX-License-Identifier: GPL-3.0-or-later

//! macOS reads the table of contents through `DKIOCCDREADTOC`, which hands
//! back the full TOC rather than the simple track list the other two systems
//! give. Positions arrive as M:S:F and have to be converted, which is where
//! the 150 frame lead-in gets subtracted back out.
//!
//! Note that the system mounts audio discs by itself. That does not get in the
//! way of an ioctl, so reading the TOC works on a mounted disc; unmounting only
//! becomes necessary once raw sectors are being read.

use std::ffi::{CStr, CString, c_char, c_void};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use super::{BYTES_PER_SECTOR, Drive, DriveError, DriveInfo};
use crate::toc::{Toc, TocEntry};

/// Full TOC, the only format that reports the lead-out position.
const CD_TOC_FORMAT_TOC: u8 = 0x02;

/// Point value the lead-in uses to describe where the lead-out starts.
const POINT_LEAD_OUT: u8 = 0xa2;

/// Q mode carrying track positions. Modes 2 and 3 hold the catalogue number
/// and the recording codes instead.
const ADR_POSITION: u8 = 1;

const TOC_BUFFER_LEN: usize = 2048;

/// Only the user data of an audio sector, which is all 2352 bytes of it.
const SECTOR_AREA_USER: u8 = 0x10;
const SECTOR_TYPE_CDDA: u8 = 0x01;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Msf {
    minute: u8,
    second: u8,
    frame: u8,
}

impl Msf {
    /// Sector address, with the lead-in taken back off.
    fn to_lba(self) -> u32 {
        ((u32::from(self.minute) * 60 + u32::from(self.second)) * 75 + u32::from(self.frame))
            .saturating_sub(crate::toc::LEAD_IN_FRAMES)
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TocDescriptor {
    session: u8,
    /// On a little endian target `control` is declared first, so it occupies
    /// the low nibble and `adr` the high one.
    control_adr: u8,
    tno: u8,
    point: u8,
    address: Msf,
    zero: u8,
    position: Msf,
}

#[repr(C)]
struct TocHeader {
    length: u16,
    session_first: u8,
    session_last: u8,
}

#[repr(C)]
struct ReadRequest {
    /// A byte offset, so sector addresses are scaled by the audio sector size.
    offset: u64,
    sector_area: u8,
    sector_type: u8,
    reserved_0080: [u8; 10],
    buffer_length: u32,
    buffer: *mut c_void,
}

#[repr(C)]
struct ReadTocRequest {
    format: u8,
    format_as_time: u8,
    reserved_0016: [u8; 5],
    address: u8,
    reserved_0064: [u8; 6],
    buffer_length: u16,
    buffer: *mut c_void,
}

/// `_IOWR('d', 100, dk_cd_read_toc_t)` expanded by hand, since the macro is
/// not available from Rust.
const fn iowr(group: u8, number: u8, size: usize) -> libc::c_ulong {
    const IOC_INOUT: libc::c_ulong = 0xc000_0000;
    const IOCPARM_MASK: usize = 0x1fff;

    IOC_INOUT
        | (((size & IOCPARM_MASK) as libc::c_ulong) << 16)
        | ((group as libc::c_ulong) << 8)
        | number as libc::c_ulong
}

const DKIOCCDREAD: libc::c_ulong = iowr(b'd', 96, size_of::<ReadRequest>());
const DKIOCCDREADTOC: libc::c_ulong = iowr(b'd', 100, size_of::<ReadTocRequest>());
const DKIOCEJECT: libc::c_ulong = iowr(b'd', 21, 0);

pub fn list() -> Vec<DriveInfo> {
    iokit::cd_block_devices()
        .into_iter()
        .map(|node| DriveInfo {
            // The raw node skips the buffer cache, which is what the ioctls
            // want.
            id: format!("/dev/r{node}"),
            name: node,
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

    let path = CString::new(id).map_err(|_| DriveError::NotFound {
        device: id.to_owned(),
    })?;

    let raw = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if raw < 0 {
        return Err(last_error(id, "open"));
    }

    Ok(Box::new(MacosDrive {
        info,
        fd: unsafe { OwnedFd::from_raw_fd(raw) },
    }))
}

struct MacosDrive {
    info: DriveInfo,
    fd: OwnedFd,
}

impl Drive for MacosDrive {
    fn info(&self) -> &DriveInfo {
        &self.info
    }

    fn read_toc(&mut self) -> Result<Toc, DriveError> {
        let mut buffer = vec![0u8; TOC_BUFFER_LEN];
        let mut request = ReadTocRequest {
            format: CD_TOC_FORMAT_TOC,
            format_as_time: 0,
            reserved_0016: [0; 5],
            address: 0,
            reserved_0064: [0; 6],
            buffer_length: TOC_BUFFER_LEN as u16,
            buffer: buffer.as_mut_ptr().cast(),
        };

        if unsafe { libc::ioctl(self.fd.as_raw_fd(), DKIOCCDREADTOC, &raw mut request) } < 0 {
            return Err(last_error(&self.info.id, "DKIOCCDREADTOC"));
        }

        parse_toc(&self.info.id, &buffer[..usize::from(request.buffer_length)])
    }

    fn read_audio(&mut self, start: u32, sectors: u32, into: &mut [u8]) -> Result<(), DriveError> {
        let wanted = sectors as usize * BYTES_PER_SECTOR;
        debug_assert!(into.len() >= wanted);

        let mut request = ReadRequest {
            offset: u64::from(start) * BYTES_PER_SECTOR as u64,
            sector_area: SECTOR_AREA_USER,
            sector_type: SECTOR_TYPE_CDDA,
            reserved_0080: [0; 10],
            buffer_length: wanted as u32,
            buffer: into.as_mut_ptr().cast(),
        };

        let failed = unsafe { libc::ioctl(self.fd.as_raw_fd(), DKIOCCDREAD, &raw mut request) } < 0;

        if failed || request.buffer_length as usize != wanted {
            return Err(DriveError::UnreadableAudio {
                device: self.info.id.clone(),
                start,
                sectors,
                status: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
            });
        }

        Ok(())
    }

    fn eject(&mut self) -> Result<(), DriveError> {
        if unsafe { libc::ioctl(self.fd.as_raw_fd(), DKIOCEJECT) } < 0 {
            return Err(last_error(&self.info.id, "DKIOCEJECT"));
        }
        Ok(())
    }
}

fn parse_toc(device: &str, buffer: &[u8]) -> Result<Toc, DriveError> {
    if buffer.len() < size_of::<TocHeader>() {
        return Err(DriveError::NoDisc {
            device: device.to_owned(),
        });
    }

    // The length field counts everything after itself.
    let declared = usize::from(u16::from_be_bytes([buffer[0], buffer[1]]));
    let available = buffer.len() - size_of::<TocHeader>();
    let payload = declared
        .saturating_sub(size_of::<TocHeader>() - size_of::<u16>())
        .min(available);
    let count = payload / size_of::<TocDescriptor>();

    let mut entries = Vec::new();
    let mut lead_out = None;

    for index in 0..count {
        let start = size_of::<TocHeader>() + index * size_of::<TocDescriptor>();
        let bytes = &buffer[start..start + size_of::<TocDescriptor>()];

        // Read field by field rather than transmuting: the buffer comes from
        // the kernel with no alignment promise.
        let control_adr = bytes[1];
        let point = bytes[3];
        let position = Msf {
            minute: bytes[8],
            second: bytes[9],
            frame: bytes[10],
        };

        if control_adr >> 4 != ADR_POSITION {
            continue;
        }

        if point == POINT_LEAD_OUT {
            lead_out = Some(position.to_lba());
        } else if (1..=99).contains(&point) {
            entries.push(TocEntry {
                number: point,
                start: position.to_lba(),
                control: control_adr & 0x0f,
            });
        }
    }

    entries.sort_by_key(|entry| entry.number);

    let lead_out = lead_out.ok_or_else(|| DriveError::NoDisc {
        device: device.to_owned(),
    })?;

    Toc::from_entries(&entries, lead_out).map_err(|reason| DriveError::UnreadableToc {
        device: device.to_owned(),
        reason,
    })
}

fn last_error(device: &str, operation: &'static str) -> DriveError {
    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);

    match errno {
        libc::EACCES | libc::EPERM => DriveError::PermissionDenied {
            device: device.to_owned(),
            group: None,
        },
        libc::ENXIO | libc::ENODEV => DriveError::NoDisc {
            device: device.to_owned(),
        },
        _ => DriveError::Io {
            device: device.to_owned(),
            operation,
            status: errno,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(point: u8, control: u8, adr: u8, position: (u8, u8, u8)) -> [u8; 11] {
        [
            1,
            (adr << 4) | control,
            0,
            point,
            0,
            0,
            0,
            0,
            position.0,
            position.1,
            position.2,
        ]
    }

    fn buffer_with(descriptors: &[[u8; 11]]) -> Vec<u8> {
        let declared = (2 + descriptors.len() * size_of::<TocDescriptor>()) as u16;

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&declared.to_be_bytes());
        buffer.extend_from_slice(&[1, 1]);
        for entry in descriptors {
            buffer.extend_from_slice(entry);
        }
        buffer
    }

    #[test]
    fn converts_positions_out_of_minute_second_frame() {
        // 00:02:00 is the very first sector of a disc, two seconds in.
        let buffer = buffer_with(&[
            descriptor(1, 0b0001, ADR_POSITION, (0, 2, 0)),
            descriptor(2, 0b0100, ADR_POSITION, (3, 0, 0)),
            descriptor(POINT_LEAD_OUT, 0, ADR_POSITION, (5, 30, 15)),
        ]);

        let toc = parse_toc("/dev/rdisk4", &buffer).expect("a well formed toc");

        assert_eq!(toc.tracks[0].start, 0);
        assert_eq!(toc.tracks[1].start, 3 * 60 * 75 - 150);
        assert_eq!(toc.lead_out, (5 * 60 + 30) * 75 + 15 - 150);
        assert!(toc.tracks[0].pre_emphasis);
        assert!(!toc.tracks[1].audio);
    }

    #[test]
    fn skips_catalogue_and_recording_code_entries() {
        let buffer = buffer_with(&[
            descriptor(1, 0, ADR_POSITION, (0, 2, 0)),
            // Q mode 2 carries the catalogue number, not a position.
            descriptor(1, 0, 2, (9, 9, 9)),
            descriptor(POINT_LEAD_OUT, 0, ADR_POSITION, (4, 0, 0)),
        ]);

        let toc = parse_toc("/dev/rdisk4", &buffer).expect("a well formed toc");
        assert_eq!(toc.tracks.len(), 1);
    }

    #[test]
    fn a_toc_without_a_lead_out_means_no_disc() {
        let buffer = buffer_with(&[descriptor(1, 0, ADR_POSITION, (0, 2, 0))]);

        assert!(matches!(
            parse_toc("/dev/rdisk4", &buffer),
            Err(DriveError::NoDisc { .. })
        ));
    }
}

/// Enough of IOKit to ask which BSD device nodes belong to optical drives.
/// There is no way to tell that from `/dev` alone.
mod iokit {
    use super::{CStr, c_char, c_void};

    type IoObject = u32;
    type KernReturn = i32;
    type CfTypeRef = *const c_void;

    const KERN_SUCCESS: KernReturn = 0;
    const IO_REGISTRY_ITERATE_RECURSIVELY: u32 = 1;
    const CF_STRING_ENCODING_ASCII: u32 = 0x0600;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOServiceMatching(name: *const c_char) -> CfTypeRef;
        fn IOServiceGetMatchingServices(
            main_port: u32,
            matching: CfTypeRef,
            existing: *mut IoObject,
        ) -> KernReturn;
        fn IOIteratorNext(iterator: IoObject) -> IoObject;
        fn IOObjectRelease(object: IoObject) -> KernReturn;
        fn IORegistryEntrySearchCFProperty(
            entry: IoObject,
            plane: *const c_char,
            key: CfTypeRef,
            allocator: CfTypeRef,
            options: u32,
        ) -> CfTypeRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: CfTypeRef,
            cstr: *const c_char,
            encoding: u32,
        ) -> CfTypeRef;
        fn CFStringGetCString(
            string: CfTypeRef,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> bool;
        fn CFRelease(value: CfTypeRef);
    }

    pub fn cd_block_devices() -> Vec<String> {
        let class = c"IOCDBlockStorageDevice";
        let plane = c"IOService";
        let bsd_name_key = c"BSD Name";

        let mut nodes = Vec::new();

        unsafe {
            let matching = IOServiceMatching(class.as_ptr());
            if matching.is_null() {
                return nodes;
            }

            // IOServiceGetMatchingServices consumes the dictionary, so it must
            // not be released here.
            let mut iterator: IoObject = 0;
            if IOServiceGetMatchingServices(0, matching, &raw mut iterator) != KERN_SUCCESS {
                return nodes;
            }

            let key = CFStringCreateWithCString(
                std::ptr::null(),
                bsd_name_key.as_ptr(),
                CF_STRING_ENCODING_ASCII,
            );

            loop {
                let device = IOIteratorNext(iterator);
                if device == 0 {
                    break;
                }

                let value = IORegistryEntrySearchCFProperty(
                    device,
                    plane.as_ptr(),
                    key,
                    std::ptr::null(),
                    IO_REGISTRY_ITERATE_RECURSIVELY,
                );

                if !value.is_null() {
                    let mut buffer = [0i8; 128];
                    if CFStringGetCString(
                        value,
                        buffer.as_mut_ptr(),
                        buffer.len() as isize,
                        CF_STRING_ENCODING_ASCII,
                    ) {
                        if let Ok(node) = CStr::from_ptr(buffer.as_ptr()).to_str() {
                            nodes.push(node.to_owned());
                        }
                    }
                    CFRelease(value);
                }

                IOObjectRelease(device);
            }

            if !key.is_null() {
                CFRelease(key);
            }
            IOObjectRelease(iterator);
        }

        nodes
    }
}
