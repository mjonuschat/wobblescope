#[macro_use]
mod common;

use common::belt_fixtures;

parity_test! {
    fn fixtures_load_with_matching_lengths() {
        for (file, chips) in [("belts_unequal_power", 2), ("belts_balanced", 1)] {
            let fixtures = belt_fixtures(file);
            assert_eq!(fixtures.len(), chips, "{file}: chip count");
            for f in &fixtures {
                assert_eq!(f.freqs_a.len(), f.psd_a.len(), "{file} {}: belt A", f.name);
                assert_eq!(f.freqs_b.len(), f.psd_b.len(), "{file} {}: belt B", f.name);
                assert!(!f.freqs_a.is_empty() && !f.freqs_b.is_empty(), "{file} {}: empty", f.name);
                assert_eq!(f.extract_max_freq, 200.0, "{file} {}: extraction limit", f.name);
            }
        }
    }
}
