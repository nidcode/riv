//! ULID generation (the `ulid` crate's own RNG integration targets a different `rand` major).

use rand::RngExt;

/// A fresh ULID string from the current time and 80 random bits.
pub fn new_ulid() -> String {
    let random: u128 = rand::rng().random::<u128>() & ((1u128 << 80) - 1);
    ulid::Ulid::from_parts(chrono::Utc::now().timestamp_millis().max(0) as u64, random).to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn ulid_shape_and_uniqueness() {
        let a = super::new_ulid();
        assert_eq!(a.len(), 26);
        assert_ne!(a, super::new_ulid());
    }
}
