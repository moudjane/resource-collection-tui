use rand::rngs::StdRng;
use rand::SeedableRng;

use super::*;

#[test]
fn generated_resources_have_expected_amount_range() {
    let mut rng = StdRng::seed_from_u64(42);
    let map = Map::generate(30, 15, &mut rng);
    let mut energy_count = 0;
    let mut crystal_count = 0;
    for tile in &map.tiles {
        if let Tile::Resource { kind, amount } = tile {
            assert!((50..=200).contains(amount));
            match kind {
                ResourceKind::Energy => energy_count += 1,
                ResourceKind::Crystal => crystal_count += 1,
            }
        }
    }
    assert!(energy_count > 0);
    assert!(crystal_count > 0);
}

#[test]
fn scout_cannot_collect_resources() {
    assert!(!RobotKind::Scout.can_collect());
    assert!(RobotKind::Collector.can_collect());
}
