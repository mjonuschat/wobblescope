#[macro_use]
mod common;

use common::{assert_close, shapers};
use wobblescope_core::CoreError;
use wobblescope_core::shaper::{Shaper, builtin};

parity_test! {
    fn impulses_match_klipper_for_every_builtin() {
        let rows = shapers();
        assert_eq!(rows.len(), 24);
        for row in rows {
            let imp = builtin(&row.name).unwrap().impulses(row.freq, row.damping_ratio).unwrap();
            let label = format!("{}@{}/{}", row.name, row.freq, row.damping_ratio);
            assert_close(&format!("{label} a"), &imp.a, &row.a);
            assert_close(&format!("{label} t"), &imp.t, &row.t);
        }
    }
}

parity_test! {
    fn unknown_shaper_name_is_none() {
        assert!(builtin("bogus").is_none());
        assert!(builtin("mzv(n=4)").is_none());
        assert!(builtin("MZV").is_none());
    }
}

parity_test! {
    fn mzv_amplitudes_sum_to_one_at_zero_damping() {
        let imp = builtin("mzv").unwrap().impulses(50.0, 0.0).unwrap();
        assert!((imp.a.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }
}

parity_test! {
    fn damping_limit_boundaries_are_enforced() {
        let limits = [
            ("zv", 0.99),
            ("mzv", 0.99),
            ("zvd", 0.99),
            ("ei", 0.4),
            ("2hump_ei", 0.3),
            ("3hump_ei", 0.2),
        ];
        for (name, max) in limits {
            let s: &dyn Shaper = builtin(name).unwrap();
            assert!(s.impulses(50.0, max - 1e-6).is_ok(), "{name} just below");
            assert!(
                matches!(s.impulses(50.0, max), Err(CoreError::ShaperInit(_))),
                "{name} at limit"
            );
        }
    }
}
