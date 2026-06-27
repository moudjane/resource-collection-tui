use std::collections::VecDeque;

use super::*;

#[test]
fn prefers_fresh_positions_over_recent_ones() {
    let recent = VecDeque::from([Position { x: 1, y: 0 }, Position { x: 0, y: 1 }]);
    let candidates = [
        Position { x: 1, y: 0 },
        Position { x: 2, y: 0 },
        Position { x: 0, y: 1 },
    ];
    assert_eq!(
        choose_non_repeating_position(&candidates, &recent),
        Some(Position { x: 2, y: 0 })
    );
}
