//! `VIHashResourceID` / `VIHashRefID`: one string hash under two names,
//! `h = h * 131 + c` over the bytes as signed chars.

pub fn hash_resource_id(s: &str) -> u32 {
    s.bytes().fold(0u32, |h, c| h.wrapping_mul(131).wrapping_add(c as i8 as i32 as u32))
}
