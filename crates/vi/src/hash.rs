//! `VIHashResourceID` / `VIHashRefID`: one string hash under two names,
//! `h = h * 131 + c` over the bytes as signed chars.

pub fn hash_resource_id(s: &str) -> u32 {
    // A byte above 127 adds negatively (`char` is signed on the EE).
    s.bytes().fold(0u32, |h, c| h.wrapping_mul(131).wrapping_add(c as i8 as i32 as u32))
}

#[cfg(test)]
mod tests {
    #[test]
    fn bytes_are_signed() {
        assert_eq!(super::hash_resource_id("Arial"), [65u32, 114, 105, 97, 108].iter().fold(0u32, |h, &c| h.wrapping_mul(131).wrapping_add(c)));
        // 0xE9 adds as -23, not 233.
        assert_eq!(super::hash_resource_id("\u{e9}"), (0xC3u8 as i8 as i32 as u32).wrapping_mul(131).wrapping_add(0xA9u8 as i8 as i32 as u32));
    }
}
