use log::error;

/// Takes a string representation of a 24 bits number like "032723"
/// and returns the "bytes" in reverse order, of course dealing with
/// a string doesn't make hex numbers pop out of thin air but it will
/// return "232703"
pub fn str_24bits_to_u32(input: String) -> Option<u32> {
    if let Ok(num) = u32::from_str_radix(&input, 16) {
        Some(num.swap_bytes())
    } else {
        error!("Failed to convert 24bits str to u32");
        None
    }
}

pub fn str_to_u16(input: String) -> Option<u16> {
    if let Ok(num) = u16::from_str_radix(&input, 16) {
        Some(num)
    } else {
        error!("Failed to convert str to u16");
        None
    }
}

pub fn str_to_u32(input: String) -> Option<u32> {
    if let Ok(num) = u32::from_str_radix(&input, 16) {
        Some(num)
    } else {
        error!("Failed to convert str to u16");
        None
    }
}

pub fn revolutions_to_degrees(rev: u16) -> f32 {
    rev as f32 / 65_536 as f32 * 360 as f32
}

pub fn degrees_to_revolutions(deg: f32) -> i16 {
    ((deg / 360 as f32) * 65_536 as f32) as i16
}

pub fn precise_revolutions_to_degrees(rev: u32) -> f32 {
    rev as f32 / 16_777_216 as f32 * 360 as f32
}

pub fn degrees_to_precise_revolutions(deg: f64) -> i32 {
    ((deg / 360.0) * 16_777_216_f64) as i32
}

pub enum TrackingMode {
    Off = 0,
    AltAz = 1,
    Eq = 2,
    Pec = 3,
}

#[cfg(test)]
mod test {
    use crate::{
        degrees_to_precise_revolutions, degrees_to_revolutions, precise_revolutions_to_degrees,
        revolutions_to_degrees, str_24bits_to_u32, str_to_u16, str_to_u32,
    };
    use assert_approx_eq::assert_approx_eq;
    #[test]
    fn test_reverse_str() {
        assert_eq!(str_24bits_to_u32(String::from("c3b2a1")), Some(0xa1b2c300));
    }

    #[test]
    fn test_str_to_u16() {
        assert_eq!(str_to_u16(String::from("12CE")), Some(4814));
        assert_eq!(str_to_u16(String::from("34AB")), Some(13483));
    }

    #[test]
    fn test_str_to_u32() {
        assert_eq!(str_to_u32(String::from("12AB05")), Some(1_223_429));
    }

    #[test]
    fn rev_to_degrees() {
        assert_approx_eq!(revolutions_to_degrees(4814), 26.4441, 1e-4_f32);
        assert_approx_eq!(
            precise_revolutions_to_degrees(1_223_429),
            26.251938,
            1e-6_f32
        );
    }

    #[test]
    fn degrees_to_rev() {
        assert_eq!(degrees_to_revolutions(26.4441), 4814);
        assert_eq!(degrees_to_precise_revolutions(26.251938), 1_223_429);
    }
}
