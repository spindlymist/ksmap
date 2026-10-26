use crate::partition::Bounds;

/// Transforms the point to be relative to the top-left corner of `bounds`.
/// - The bounded coordinates are assumed to be in the normal range for screen coordinates, \[`i32::MIN`, `i32::MAX`\].
pub fn get_point_relative_to_bounds(point: (i32, i32), bounds: &Bounds) -> (u32, u32) {
    let x = ((point.0 as i64) - bounds.x_min()) as u32;
    let y = ((point.1 as i64) - bounds.y_min()) as u32;
    (x, y)
}

/// Determines the grid size needed to contain these bounds.
/// - The returned size is the edge length of the grid minus one, i.e. 0 indicates a 1 by 1 grid
///     and u32::MAX indicates a 2^32 by 2^32 grid. It can be passed to [`point_to_index`] as is.
/// - The bounded coordinates are assumed to be in the normal range for screen coordinates, \[`i32::MIN`, `i32::MAX`\].
pub fn grid_size_from_bounds(bounds: &Bounds) -> u32 {
    if bounds.is_empty() {
        return 0;
    }
    
    let width = (bounds.width() - 1) as u32;
    let height = (bounds.height() - 1) as u32;
    
    width.max(height)
}

/// Calculates the index of the point on a Hilbert curve that fills the square with vertices
/// at (0, 0) and (n, n), where n is the next power of 2 greater than or equal to the grid size.
pub fn point_to_index(point: (u32, u32), grid_size_minus_one: u32) -> u64 {
    let mut x = point.0 as u64;
    let mut y = point.1 as u64;
    let n = (grid_size_minus_one as u64 + 1).next_power_of_two(); // At most 2^32
    let mut s = n / 2;
    let mut d = 0;
    
    while s > 0 {
        let rx = ((x & s) > 0) as u64;
        let ry = ((y & s) > 0) as u64;
        d += s * s * ((3 * rx) ^ ry);
        (x, y) = rotate(n, x, y, rx, ry);
        s /= 2;
    }
    
    d
}

/// Calculates the `index`th point on a Hilbert curve that fills the square with vertices
/// at (0, 0) and (n, n), where n is the next power of 2 greater than or equal to the grid size.
#[cfg(test)]
fn index_to_point(index: u64, grid_size_minus_one: u32) -> (u32, u32) {
    let n = (grid_size_minus_one as u64 + 1).next_power_of_two();
    let mut s = 1;
    let mut t = index;
    let mut x = 0;
    let mut y = 0;
    
    while s < n {
        let rx = 1 & (t / 2);
        let ry = 1 & (t ^ rx);
        (x, y) = rotate(s, x, y, rx, ry);
        x += s * rx;
        y += s * ry;
        t /= 4;
        s *= 2;
    }
    
    (x as u32, y as u32)
}

#[inline(always)]
fn rotate(n: u64, mut x: u64, mut y: u64, rx: u64, ry: u64) -> (u64, u64) {
    assert!(n.is_power_of_two());
    if ry == 0 {
        if rx == 1 {
            x = n - 1 - x;
            y = n - 1 - y;
        }
        std::mem::swap(&mut x, &mut y);
    }
    
    (x, y)
}


#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};
    use crate::partition::Bounds;
    use super::*;
    
    const CORNERS: [(u32, u32); 4] = [
        (0, 0),
        (0, u32::MAX),
        (u32::MAX, 0),
        (u32::MAX, u32::MAX)
    ];
    
    #[test]
    fn get_point_relative_to_bounds_handles_positive_coords() {
        // Positive coords
        {
            let bounds = Bounds {
                x: 10..21,
                y: 10..21
            };
            for (point_abs, point_rel_expected) in [
                ((10, 10), (0, 0)),
                ((15, 15), (5, 5)),
                ((20, 20), (10, 10)),
            ] {
                let point_rel_actual = get_point_relative_to_bounds(point_abs, &bounds);
                assert_eq!(point_rel_actual, point_rel_expected, "\
                    {point_abs:?} was transformed to {point_rel_actual:?}; \
                    expected {point_rel_expected:?}");
            }
        }
    }
    
    #[test]
    fn get_point_relative_to_bounds_handles_negative_coords() {
        {
            let bounds = Bounds {
                x: -20..-9,
                y: -20..-9
            };
            for (point_abs, point_rel_expected) in [
                ((-10, -10), (10, 10)),
                ((-15, -15), (5, 5)),
                ((-20, -20), (0, 0)),
            ] {
                let point_rel_actual = get_point_relative_to_bounds(point_abs, &bounds);
                assert_eq!(point_rel_actual, point_rel_expected, "\
                    {point_abs:?} was transformed to {point_rel_actual:?}; \
                    expected {point_rel_expected:?}");
            }
        }
    }
    
    #[test]
    fn get_point_relative_to_bounds_handles_extreme_points() {
        {
            let bounds = Bounds {
                x: (i32::MIN as i64)..(i32::MAX as i64) + 1,
                y: (i32::MIN as i64)..(i32::MAX as i64) + 1
            };
            for (point_abs, point_rel_expected) in [
                ((i32::MIN, i32::MIN), (0, 0)),
                ((0, 0), (u32::pow(2, 31), u32::pow(2, 31))),
                ((i32::MAX, i32::MAX), (u32::MAX, u32::MAX)),
            ] {
                let point_rel_actual = get_point_relative_to_bounds(point_abs, &bounds);
                assert_eq!(point_rel_actual, point_rel_expected, "\
                    {point_abs:?} was transformed to {point_rel_actual:?}; \
                    expected {point_rel_expected:?}");
            }
        }
    }
    
    #[test]
    fn index_to_points_works() {
        let mut points_used = HashSet::new();
        
        for grid_size in 1..=16 {
            points_used.clear();
            
            let i_max = (grid_size as u64).pow(2);
            let mut x_prev = 0;
            let mut y_prev = 0;
            
            for i in 0..i_max {
                let (x, y) = index_to_point(i, grid_size - 1);
                
                let is_unique = points_used.insert((x, y));
                assert!(is_unique, "At grid size {grid_size}, \
                    index {i} was not unique; \
                    point was ({x}, {y})");
                
                if i > 0 {
                    let manhattan_dist = x.abs_diff(x_prev) + y.abs_diff(y_prev);
                    assert_eq!(manhattan_dist, 1, "At grid size {grid_size}, \
                        index {i} had manhattan distance {manhattan_dist} from previous point; \
                        point was ({x}, {y}) and previous point was ({x_prev}, {y_prev})");
                }
                
                x_prev = x;
                y_prev = y;
            }
        }
    }
    
    #[test]
    fn index_to_point_handles_extreme_indices() {
        // First index should map to (0, 0)
        let first_point = index_to_point(0, u32::MAX);
        assert_eq!(first_point, (0, 0));
        
        // Last index should map to one of the other 3 corners
        let last_point = index_to_point(u64::MAX, u32::MAX);
        assert_ne!(last_point, (0, 0));
        assert!(CORNERS.contains(&last_point));
    }
    
    #[test]
    fn point_to_index_works() {
        let mut lookup_index_by_point = HashMap::new();
        
        for grid_size in 1..=16 {
            lookup_index_by_point.clear();
            let grid_size_po2 = (grid_size as u64).next_power_of_two();
            let i_max = grid_size_po2.pow(2);
            for i in 0..i_max {
                let point = index_to_point(i, (grid_size_po2 - 1) as u32);
                lookup_index_by_point.insert(point, i);
            }
            
            for y in 0..grid_size {
                for x in 0..grid_size {
                    let actual_idx = point_to_index((x, y), grid_size - 1);
                    let expected_idx = *lookup_index_by_point.get(&(x, y)).unwrap();
                    assert_eq!(actual_idx, expected_idx, "At grid size {grid_size}, \
                        calculated index for point ({x}, {y}) was {actual_idx}; \
                        expected index was {expected_idx}");
                }
            }
        }
    }
    
    #[test]
    fn point_to_index_handles_extreme_points() {
        let mut index_min = u64::MAX;
        let mut index_max = u64::MIN;
        
        // Check all four corners of a 2^32 by 2^32 grid
        for p in CORNERS {
            let i = point_to_index(p, u32::MAX);
            assert_eq!(index_to_point(i, u32::MAX), p);
            
            index_min = index_min.min(i);
            index_max = index_max.max(i);
        }
        
        // The curve should start in one corner and end in another
        assert_eq!(index_min, u64::MIN);
        assert_eq!(index_max, u64::MAX);
    }
}
