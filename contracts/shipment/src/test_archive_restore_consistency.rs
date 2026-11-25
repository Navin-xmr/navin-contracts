//! Tests for issue #7 — archived restore consistency regression tests.

#[cfg(test)]
mod tests {
    use crate::NavinError;

    /// ShipmentUnavailable error code must be 42.
    /// This pins the discriminant so it cannot drift across refactors.
    #[test]
    fn shipment_unavailable_error_code_is_42() {
        assert_eq!(
            NavinError::ShipmentUnavailable as u32,
            42,
            "ShipmentUnavailable discriminant must be 42 per the error contract"
        );
    }
}
