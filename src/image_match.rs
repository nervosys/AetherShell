//! Exact-with-tolerance template matching over RGBA8 buffers.
//!
//! Pure: no I/O, no image decoding. `gui_find_image` reads and decodes the
//! files, then hands the pixels here. Every bound is checked before any pixel
//! is compared, so a refusal never costs a scan.

/// Largest haystack or needle, in pixels, that will be decoded at all.
pub const MAX_PIXELS: u64 = 40_000_000;

/// Worst-case pixel comparisons one search may perform:
/// candidate positions times needle pixels.
pub const MAX_WORK: u64 = 40_000_000_000;

/// Read-only view of a tightly packed RGBA8 buffer.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

/// One match: top-left corner in haystack coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchError {
    /// A needle or haystack dimension is zero.
    ZeroSize,
    /// The needle is larger than the haystack in width or height.
    NeedleTooLarge {
        haystack: (u32, u32),
        needle: (u32, u32),
    },
    /// The worst case needs more comparisons than [`MAX_WORK`] (or overflows).
    WorkExceeded { work: Option<u64> },
    /// The buffer length does not match width * height * 4.
    BadBuffer,
}

/// Validate dimensions and return the worst-case comparison count.
pub fn plan(hw: u32, hh: u32, nw: u32, nh: u32) -> Result<u64, MatchError> {
    if hw == 0 || hh == 0 || nw == 0 || nh == 0 {
        return Err(MatchError::ZeroSize);
    }
    if nw > hw || nh > hh {
        return Err(MatchError::NeedleTooLarge {
            haystack: (hw, hh),
            needle: (nw, nh),
        });
    }
    let positions = (u64::from(hw - nw) + 1).checked_mul(u64::from(hh - nh) + 1);
    let needle_px = u64::from(nw).checked_mul(u64::from(nh));
    let work = positions.zip(needle_px).and_then(|(p, n)| p.checked_mul(n));
    match work {
        Some(w) if w <= MAX_WORK => Ok(w),
        other => Err(MatchError::WorkExceeded { work: other }),
    }
}

/// Find positions where the needle matches the haystack, scanning rows top to
/// bottom and columns left to right.
///
/// A position matches when every pixel of the needle is within `tolerance` of
/// the haystack pixel under it on every RGBA channel (absolute difference).
/// Overlapping matches are all reported. With `all == false` at most one hit
/// is returned; with `all == true` at most `limit` hits are returned.
pub fn find_matches(
    hay: Frame<'_>,
    needle: Frame<'_>,
    tolerance: u8,
    all: bool,
    limit: usize,
) -> Result<Vec<Hit>, MatchError> {
    plan(hay.width, hay.height, needle.width, needle.height)?;
    if hay.rgba.len() as u64 != u64::from(hay.width) * u64::from(hay.height) * 4
        || needle.rgba.len() as u64 != u64::from(needle.width) * u64::from(needle.height) * 4
    {
        return Err(MatchError::BadBuffer);
    }
    let cap = if all { limit } else { 1 };
    let mut hits = Vec::new();
    if cap == 0 {
        return Ok(hits);
    }
    let (hw, nw) = (hay.width as usize, needle.width as usize);
    let (nh, row_bytes) = (needle.height as usize, needle.width as usize * 4);
    let last_x = hay.width - needle.width;
    let last_y = hay.height - needle.height;
    for y in 0..=last_y {
        for x in 0..=last_x {
            if matches_at(
                &hay, &needle, x as usize, y as usize, hw, nw, nh, row_bytes, tolerance,
            ) {
                hits.push(Hit { x, y });
                if hits.len() >= cap {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}

#[allow(clippy::too_many_arguments)]
fn matches_at(
    hay: &Frame<'_>,
    needle: &Frame<'_>,
    x: usize,
    y: usize,
    hw: usize,
    nw: usize,
    nh: usize,
    row_bytes: usize,
    tolerance: u8,
) -> bool {
    for ny in 0..nh {
        let h_start = ((y + ny) * hw + x) * 4;
        let n_start = ny * row_bytes;
        let h_row = &hay.rgba[h_start..h_start + row_bytes];
        let n_row = &needle.rgba[n_start..n_start + row_bytes];
        debug_assert_eq!(row_bytes, nw * 4);
        for (a, b) in h_row.iter().zip(n_row.iter()) {
            if a.abs_diff(*b) > tolerance {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic, mostly distinct RGBA pixels for a needle.
    fn pattern(w: u32, h: u32, seed: u8) -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..(w * h) {
            let b = (i as u8).wrapping_mul(53).wrapping_add(seed);
            v.extend_from_slice(&[b, b.wrapping_add(90), b.wrapping_mul(3), 255]);
        }
        v
    }

    fn frame(w: u32, h: u32, rgba: &[u8]) -> Frame<'_> {
        Frame {
            width: w,
            height: h,
            rgba,
        }
    }

    fn paste(hay: &mut [u8], hw: u32, needle: &[u8], nw: u32, nh: u32, x: u32, y: u32) {
        for ny in 0..nh {
            let src = (ny * nw * 4) as usize;
            let dst = (((y + ny) * hw + x) * 4) as usize;
            let len = (nw * 4) as usize;
            hay[dst..dst + len].copy_from_slice(&needle[src..src + len]);
        }
    }

    /// An 8x4 gray haystack holding a 3x2 needle at (1,1) and (4,2); the two
    /// copies do not overlap.
    fn fixture() -> (Vec<u8>, Vec<u8>) {
        let (hw, hh, nw, nh) = (8u32, 4u32, 3u32, 2u32);
        let needle = pattern(nw, nh, 7);
        let mut hay = vec![16u8; (hw * hh * 4) as usize];
        for alpha in hay.iter_mut().skip(3).step_by(4) {
            *alpha = 255;
        }
        paste(&mut hay, hw, &needle, nw, nh, 1, 1);
        paste(&mut hay, hw, &needle, nw, nh, 4, 2);
        (hay, needle)
    }

    #[test]
    fn exact_match_reports_known_top_left_first() {
        let (hay, needle) = fixture();
        let hits = find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 0, false, 100).unwrap();
        assert_eq!(hits, vec![Hit { x: 1, y: 1 }]);
    }

    #[test]
    fn all_matches_come_back_in_row_major_order() {
        let (hay, needle) = fixture();
        let hits = find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 0, true, 100).unwrap();
        assert_eq!(hits, vec![Hit { x: 1, y: 1 }, Hit { x: 4, y: 2 }]);
    }

    #[test]
    fn overlapping_matches_are_all_reported() {
        // A uniform 2x2 needle matches at every position of a 4x3 haystack:
        // 3 columns by 2 rows of positions, each overlapping its neighbours.
        let needle = [9u8; 16];
        let hay = [9u8; 4 * 3 * 4];
        let hits = find_matches(frame(4, 3, &hay), frame(2, 2, &needle), 0, true, 100).unwrap();
        let expected: Vec<Hit> = (0..2)
            .flat_map(|y| (0..3).map(move |x| Hit { x, y }))
            .collect();
        assert_eq!(hits, expected);
    }

    #[test]
    fn limit_caps_all_matches() {
        let (hay, needle) = fixture();
        let hits = find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 0, true, 1).unwrap();
        assert_eq!(hits, vec![Hit { x: 1, y: 1 }]);
        let none = find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 0, true, 0).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn no_match_is_empty() {
        let (hay, mut needle) = fixture();
        // A one-unit change to the needle's first channel: no copy in the
        // haystack is that close at tolerance 0.
        needle[0] = needle[0].wrapping_add(1);
        let hits = find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 0, true, 100).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn tolerance_admits_a_near_match() {
        let (hay, mut needle) = fixture();
        // Shift one channel of one needle pixel by 3: both copies now differ
        // from the needle by exactly 3 on that channel.
        needle[5] = needle[5].wrapping_add(3);
        let exact = find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 0, true, 100).unwrap();
        assert!(exact.is_empty());
        let near = find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 3, true, 100).unwrap();
        assert_eq!(near, vec![Hit { x: 1, y: 1 }, Hit { x: 4, y: 2 }]);
        let too_strict =
            find_matches(frame(8, 4, &hay), frame(3, 2, &needle), 2, true, 100).unwrap();
        assert!(too_strict.is_empty());
    }

    #[test]
    fn needle_larger_than_haystack_is_refused() {
        let (hay, needle) = fixture();
        let err = find_matches(frame(3, 2, &needle), frame(8, 4, &hay), 0, false, 1).unwrap_err();
        assert!(matches!(err, MatchError::NeedleTooLarge { .. }));
    }

    #[test]
    fn plan_counts_worst_case_work() {
        // 8x4 hay, 3x2 needle: 6 * 3 positions times 6 pixels.
        assert_eq!(plan(8, 4, 3, 2), Ok(108));
        // Needle equal to haystack: one position.
        assert_eq!(plan(3, 2, 3, 2), Ok(6));
    }

    #[test]
    fn plan_refuses_work_above_the_bound() {
        // 9001 * 2001 positions * 2_000_000 needle pixels is about 3.6e13.
        assert!(matches!(
            plan(10_000, 4_000, 1_000, 2_000),
            Err(MatchError::WorkExceeded { work: Some(_) })
        ));
        // 8001 * 2001 positions * 4_000_000 needle pixels, also far over.
        assert!(matches!(
            plan(10_000, 4_000, 2_000, 2_000),
            Err(MatchError::WorkExceeded { .. })
        ));
        // Exactly at the bound is allowed: 200 * 200 positions of a
        // 1000x1000 needle is 40_000 * 1_000_000 = 4e10 comparisons.
        assert_eq!(plan(1_199, 1_199, 1_000, 1_000), Ok(MAX_WORK));
        // One more row of positions is over it.
        assert!(matches!(
            plan(1_200, 1_199, 1_000, 1_000),
            Err(MatchError::WorkExceeded { .. })
        ));
    }

    #[test]
    fn plan_refuses_zero_dimensions() {
        assert_eq!(plan(0, 4, 1, 1), Err(MatchError::ZeroSize));
        assert_eq!(plan(4, 4, 0, 1), Err(MatchError::ZeroSize));
    }

    #[test]
    fn bad_buffer_length_is_refused() {
        let (hay, needle) = fixture();
        let short = &hay[..hay.len() - 1];
        let err = find_matches(frame(8, 4, short), frame(3, 2, &needle), 0, false, 1).unwrap_err();
        assert_eq!(err, MatchError::BadBuffer);
    }
}
