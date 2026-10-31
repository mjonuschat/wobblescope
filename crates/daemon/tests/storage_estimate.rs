use wobblescope::storage::estimate::estimate_bytes;

#[test]
fn one_full_sweep_at_3200_hz() {
    assert_eq!(estimate_bytes(&[(3200.0, 130.0)]).unwrap(), 19_968_000);
}

#[test]
fn recordings_add_up() {
    assert_eq!(
        estimate_bytes(&[(3200.0, 10.0), (1600.0, 10.0)]).unwrap(),
        2_304_000
    );
}

#[test]
fn a_fractional_sample_count_rounds_up() {
    assert_eq!(estimate_bytes(&[(3200.5, 1.0)]).unwrap(), 153_648);
}

#[test]
fn no_recordings_need_no_space() {
    assert_eq!(estimate_bytes(&[]).unwrap(), 0);
}

#[test]
fn nan_infinite_and_negative_inputs_are_errors() {
    assert!(estimate_bytes(&[(f64::NAN, 1.0)]).is_err());
    assert!(estimate_bytes(&[(3200.0, f64::INFINITY)]).is_err());
    assert!(estimate_bytes(&[(3200.0, -1.0)]).is_err());
}
