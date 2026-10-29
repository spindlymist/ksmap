mod bounds;
mod hilbert;
mod grid;
mod islands;

use libks::ScreenCoord;

use crate::screen_map::ScreenMap;

pub use bounds::Bounds;
pub use grid::GridPartitioner;
pub use islands::IslandsPartitioner;

pub trait Partitioner {
    fn partitions(&self, screens: &ScreenMap) -> Vec<Partition>;
}

#[derive(Debug, Clone)]
pub struct Partition {
    positions: Vec<ScreenCoord>,
    bounds: Bounds,
}

pub fn merge_nested_partitions(partitions: &mut Vec<Partition>) {
    // This algorithm is not great, but it's fine for small numbers of partitions
    // For each partition, consider each partition that comes after it in the list
    let mut i = 0;
    'outer: while i < partitions.len() {
        let current = partitions[i].bounds().clone();
        let mut j = i + 1;
        while j < partitions.len() {
            let other = partitions[j].bounds().clone();
            // If we fully contain the other partition, remove it from the list
            if current.contains(&other) {
                let removed = partitions.swap_remove(j);
                partitions[i].merge(removed);
                // Don't increment j because j is a different partition now
            }
            // If the other partition fully contains us, replace the current partition with that one
            // and remove the other one from the list
            else if other.contains(&current) {
                let removed = partitions.swap_remove(j);
                partitions[i].merge(removed);
                // We need to recheck the partitions between i and j because they weren't given a chance to be consumed
                // by j yet. We could try to merge more partitions before restarting, but I think that's worse in the
                // average case. That would guarantee another ~(n - i) iterations of the inner loop. This way, we're
                // only guaranteeing another ~(j - i) iterations.
                continue 'outer;
            }
            else {
                j += 1;
            }
        }
        i += 1;
    }
}

pub fn sort_partitions_spatially(partitions: &mut [Partition]) {
    let bounds_combined = {
        let bounds_iter = partitions.iter().map(|partition| &partition.bounds);
        Bounds::from_iter(bounds_iter)
    };
    let grid_size = hilbert::grid_size_from_bounds(&bounds_combined);
    
    // Sort the partitions by their index on a Hilbert curve that fills the combined bounds
    partitions.sort_by_cached_key(|partition| {
        let center = partition.bounds.center();
        let center = (center.0 as i32, center.1 as i32);
        let center_relative = hilbert::get_point_relative_to_bounds(center, &bounds_combined);
        hilbert::point_to_index(center_relative, grid_size)
    });
}

impl Partition {
    pub fn new(positions: Vec<ScreenCoord>) -> Self {
        let bounds = Bounds::from(positions.as_slice());
        Self {
            positions,
            bounds,
        }
    }

    pub fn positions(&self) -> &[ScreenCoord] {
        &self.positions
    }
    
    pub fn bounds(&self) -> &Bounds {
        &self.bounds
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }
    
    pub fn merge(&mut self, other: Partition) {
        self.positions.extend(other.positions.into_iter());
        self.bounds = Bounds::union(&self.bounds, &other.bounds);
    }
}

impl IntoIterator for Partition {
    type Item = ScreenCoord;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.positions.into_iter()
    }
}

impl<'a> IntoIterator for &'a Partition {
    type Item = &'a ScreenCoord;
    type IntoIter = std::slice::Iter<'a, ScreenCoord>;

    fn into_iter(self) -> Self::IntoIter {
        self.positions.iter()
    }
}
