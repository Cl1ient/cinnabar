//! Paired door states and the neighboring facts used to choose their hinge.

/// A door only on the negative side forces the hinge; otherwise solid counts break ties.
fn hinge(negative: (u8, bool), positive: (u8, bool)) -> bool {
    (negative.1 && !positive.1) || negative.0 < positive.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hinge_table_covers_neighbor_doors_and_both_solid_counts() {
        const COUNTS: [[bool; 3]; 3] = [
            [false, true, true],
            [false, false, true],
            [false, false, false],
        ];
        for (negative, row) in COUNTS.into_iter().enumerate() {
            for (positive, expected) in row.into_iter().enumerate() {
                assert_eq!(
                    hinge((negative as u8, false), (positive as u8, false)),
                    expected
                );
                assert!(hinge((negative as u8, true), (positive as u8, false)));
                assert_eq!(
                    hinge((negative as u8, true), (positive as u8, true)),
                    expected
                );
                assert_eq!(
                    hinge((negative as u8, false), (positive as u8, true)),
                    expected
                );
            }
        }
    }
}
