pub fn from_bits_to_u64(bits: &[bool]) -> u64 {
    let mut result: u64 = 0;
    let mut shift = 0;
    for &bit in bits {
        if bit {
            result |= 1 << shift;
        }
        shift += 1;
        if shift == 64 {
            break;
        }
    }
    result
}

pub fn from_bits_to_u32(bits: &[bool]) -> u32 {
    let mut result = 0;
    let mut shift = 0;
    for &bit in bits {
        if bit {
            result |= 1 << shift;
        }
        shift += 1;
        if shift == 32 {
            break;
        }
    }
    result
}

pub fn from_bits_to_u8(bits: &[bool]) -> u8 {
    let mut result = 0;
    let mut shift = 0;
    for &bit in bits {
        if bit {
            result |= 1 << shift;
        }
        shift += 1;
        if shift == 8 {
            break;
        }
    }
    result
}

pub fn u64_to_bits(num: u64) -> Vec<bool> {
    let mut result = Vec::with_capacity(64);
    let mut n = num;
    for _ in 0..64 {
        result.push(n & 1 == 1);
        n >>= 1;
    }
    result
}

pub fn u32_to_bits(num: u32) -> Vec<bool> {
    let mut result = Vec::with_capacity(32);
    let mut n = num;
    for _ in 0..32 {
        result.push(n & 1 == 1);
        n >>= 1;
    }
    result
}

pub fn u8_to_bits(num: u8) -> Vec<bool> {
    let mut result = Vec::with_capacity(8);
    let mut n = num;
    for _ in 0..8 {
        result.push(n & 1 == 1);
        n >>= 1;
    }
    result
}

pub fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| from_bits_to_u8(chunk))
        .collect()
}

pub fn bits_to_u32(bits: &[bool]) -> Vec<u32> {
    bits.chunks(32)
        .map(|chunk| from_bits_to_u32(chunk))
        .collect()
}

pub fn bytes_to_bits(bytes: &[u8], use_be: bool) -> Vec<bool> {
    bytes
        .iter()
        .flat_map(|x| {
            let mut u = u8_to_bits(* x);
            if use_be { u.reverse(); }
            u
        })
        .collect()
}

pub fn find_subsequence_u8(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
