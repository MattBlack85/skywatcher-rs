use log::{error, warn};

/// Takes a string representation of a 24 bits number like "032723"
/// and returns the "bytes" in reverse order, of course dealing with
/// a string doesn't make hex numbers pop out of thin air but it will
/// return "232703"
pub fn str_24bits_to_u32(input: String) -> u32 {
    if let Ok(num) = u32::from_str_radix(&input, 16) {
        num.swap_bytes()
    } else {
        error!("ERROR");
        0
    }
}

#[cfg(test)]
mod test {
    use crate::str_24bits_to_u32;

    #[test]
    fn test_reverse_str() {
        assert_eq!(str_24bits_to_u32(String::from("c3b2a1")), 0xa1b2c300);
    }
}
