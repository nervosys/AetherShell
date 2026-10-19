//! The file version of a Windows PE image, read from its VS_FIXEDFILEINFO.
//!
//! No PE parser is linked for this: the fixed file info is a 52-byte structure
//! that begins with the signature 0xFEEF04BD, which the version resource
//! stores DWORD-aligned. The structure is found by scanning for that
//! signature at 4-byte-aligned offsets, so an unaligned match inside other
//! data is ignored. Only the first four DWORDs after the signature are read:
//! dwStrucVersion, dwFileVersionMS and dwFileVersionLS.

/// VS_FIXEDFILEINFO.dwSignature, stored little-endian as BD 04 EF FE.
const VS_FFI_SIGNATURE: u32 = 0xFEEF_04BD;

/// Bytes read after the signature: dwStrucVersion, dwFileVersionMS and
/// dwFileVersionLS, each a little-endian u32.
const FIELDS_AFTER_SIGNATURE: usize = 12;

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let raw: [u8; 4] = bytes.get(at..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(raw))
}

/// The "a.b.c.d" file version of a PE image, or `None` when no plausible
/// VS_FIXEDFILEINFO is present.
///
/// A candidate is plausible when its dwStrucVersion has major version 1 (the
/// high word, as in 0x00010000) and its file version is not 0.0.0.0. Candidates
/// that fail either test are passed over, and the first plausible one wins.
/// A buffer that ends inside a structure contributes nothing.
pub(crate) fn pe_file_version(bytes: &[u8]) -> Option<String> {
    let mut at = 0usize;
    while at.checked_add(4 + FIELDS_AFTER_SIGNATURE)? <= bytes.len() {
        if u32_at(bytes, at) == Some(VS_FFI_SIGNATURE) {
            let struc = u32_at(bytes, at + 4)?;
            let ms = u32_at(bytes, at + 8)?;
            let ls = u32_at(bytes, at + 12)?;
            let plausible_struc = struc >> 16 == 1;
            if plausible_struc && (ms != 0 || ls != 0) {
                return Some(format!(
                    "{}.{}.{}.{}",
                    ms >> 16,
                    ms & 0xFFFF,
                    ls >> 16,
                    ls & 0xFFFF
                ));
            }
        }
        at += 4;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::pe_file_version;

    /// One VS_FIXEDFILEINFO: signature, struc version 0x00010000, then the
    /// file version MS and LS.
    fn ffi(ms: u32, ls: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0xFEEF_04BDu32.to_le_bytes());
        v.extend_from_slice(&0x0001_0000u32.to_le_bytes());
        v.extend_from_slice(&ms.to_le_bytes());
        v.extend_from_slice(&ls.to_le_bytes());
        v
    }

    fn pack(major: u16, minor: u16, build: u16, rev: u16) -> (u32, u32) {
        (
            (u32::from(major) << 16) | u32::from(minor),
            (u32::from(build) << 16) | u32::from(rev),
        )
    }

    #[test]
    fn signature_at_an_aligned_offset_gives_the_version() {
        let (ms, ls) = pack(14, 38, 33135, 0);
        let mut buf = vec![0u8; 64];
        buf.extend(ffi(ms, ls));
        buf.extend([0u8; 16]);
        assert_eq!(pe_file_version(&buf).as_deref(), Some("14.38.33135.0"));
    }

    #[test]
    fn an_unaligned_decoy_is_ignored() {
        let (ms, ls) = pack(14, 38, 33135, 0);
        // One byte of padding leaves the structure at offset 65, which is not
        // 4-aligned, so the scan must not find it.
        let mut buf = vec![0u8; 65];
        buf.extend(ffi(ms, ls));
        assert_eq!(pe_file_version(&buf), None);
        // The same structure placed at an aligned offset is found.
        let mut aligned = vec![0u8; 64];
        aligned.extend(ffi(ms, ls));
        assert_eq!(pe_file_version(&aligned).as_deref(), Some("14.38.33135.0"));
    }

    #[test]
    fn a_truncated_structure_gives_none() {
        let (ms, ls) = pack(14, 38, 33135, 0);
        let full = ffi(ms, ls);
        for cut in 0..full.len() {
            assert_eq!(pe_file_version(&full[..cut]), None, "cut at {cut}");
        }
        assert_eq!(pe_file_version(&[]), None);
    }

    #[test]
    fn an_implausible_struc_version_is_passed_over() {
        let mut bad = Vec::new();
        bad.extend_from_slice(&0xFEEF_04BDu32.to_le_bytes());
        bad.extend_from_slice(&0x0002_0000u32.to_le_bytes());
        bad.extend_from_slice(&0x000E_0026u32.to_le_bytes());
        bad.extend_from_slice(&0x8A6D_0000u32.to_le_bytes());
        assert_eq!(pe_file_version(&bad), None);
    }

    #[test]
    fn a_zero_file_version_is_not_reported() {
        assert_eq!(pe_file_version(&ffi(0, 0)), None);
    }

    #[test]
    fn the_first_valid_structure_wins() {
        let (ms1, ls1) = pack(14, 38, 33135, 0);
        let (ms2, ls2) = pack(14, 40, 1, 2);
        let mut buf = ffi(ms1, ls1);
        buf.extend(ffi(ms2, ls2));
        assert_eq!(pe_file_version(&buf).as_deref(), Some("14.38.33135.0"));
    }

    #[test]
    fn an_implausible_candidate_does_not_hide_a_later_valid_one() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0xFEEF_04BDu32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        let (ms, ls) = pack(14, 38, 33135, 0);
        buf.extend(ffi(ms, ls));
        assert_eq!(pe_file_version(&buf).as_deref(), Some("14.38.33135.0"));
    }
}
