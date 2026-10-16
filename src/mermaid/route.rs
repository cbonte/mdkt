//! Orthogonal edge routing on the character grid.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::canvas::{Canvas, Dir, E, N, S, W};

const TURN_COST: u32 = 3;
const OVERLAP_COST: u32 = 6;
const CROSSING_COST: u32 = 4;
const NEAR_NODE_COST: u32 = 1;

/// Cells a path must not enter, indexed `y * width + x`.
pub struct Blocked {
    w: usize,
    h: usize,
    cells: Vec<bool>,
}

impl Blocked {
    pub fn new(w: usize, h: usize) -> Blocked {
        Blocked {
            w,
            h,
            cells: vec![false; w * h],
        }
    }

    pub fn block_rect(&mut self, x: usize, y: usize, w: usize, h: usize) {
        for cy in y..y + h {
            for cx in x..x + w {
                self.cells[cy * self.w + cx] = true;
            }
        }
    }

    fn is_blocked(&self, x: usize, y: usize) -> bool {
        self.cells[y * self.w + x]
    }

    fn near_block(&self, x: usize, y: usize) -> bool {
        Dir::ALL.iter().any(|d| {
            let (nx, ny) = match d {
                Dir::N if y > 0 => (x, y - 1),
                Dir::E if x + 1 < self.w => (x + 1, y),
                Dir::S if y + 1 < self.h => (x, y + 1),
                Dir::W if x > 0 => (x - 1, y),
                _ => return false,
            };
            self.is_blocked(nx, ny)
        })
    }
}

/// Cheapest path of cells from `start`, left heading `out`, to `goal`, entered heading `arrive`.
/// Paths prefer few turns, avoid nodes and cross other edges only when needed.
pub fn route(
    canvas: &Canvas,
    blocked: &Blocked,
    start: (usize, usize),
    out: Dir,
    goal: (usize, usize),
    arrive: Dir,
) -> Option<Vec<(usize, usize)>> {
    let (w, h) = (canvas.width(), canvas.height());
    let state = |(x, y): (usize, usize), d: Dir| (y * w + x) * 4 + d.index();
    let open =
        |x: usize, y: usize| (x, y) == goal || (!blocked.is_blocked(x, y) && canvas.is_free(x, y));
    if !open(start.0, start.1) {
        return None;
    }
    let mut dist = vec![u32::MAX; w * h * 4];
    let mut prev = vec![usize::MAX; w * h * 4];
    let mut heap = BinaryHeap::new();
    dist[state(start, out)] = 0;
    heap.push(Reverse((0u32, start.0, start.1, out.index())));
    while let Some(Reverse((cost, x, y, di))) = heap.pop() {
        let dir = Dir::ALL[di];
        let here = state((x, y), dir);
        if cost > dist[here] {
            continue;
        }
        if (x, y) == goal && dir == arrive {
            let mut path = vec![(x, y)];
            let mut s = here;
            while prev[s] != usize::MAX {
                s = prev[s];
                path.push(((s / 4) % w, (s / 4) / w));
            }
            path.reverse();
            return Some(path);
        }
        for next in Dir::ALL {
            if next == dir.opposite() {
                continue;
            }
            let at_edge = match next {
                Dir::N => y == 0,
                Dir::E => x + 1 == w,
                Dir::S => y + 1 == h,
                Dir::W => x == 0,
            };
            if at_edge {
                continue;
            }
            let (nx, ny) = next.step((x, y));
            if !open(nx, ny) {
                continue;
            }
            let mut step = 1;
            if next != dir {
                step += TURN_COST;
            }
            let lines = canvas.lines(nx, ny);
            if lines != 0 {
                let along = if matches!(next, Dir::N | Dir::S) {
                    N | S
                } else {
                    E | W
                };
                step += if lines & along != 0 {
                    OVERLAP_COST
                } else {
                    CROSSING_COST
                };
            }
            if blocked.near_block(nx, ny) {
                step += NEAR_NODE_COST;
            }
            let ns = state((nx, ny), next);
            if cost + step < dist[ns] {
                dist[ns] = cost + step;
                prev[ns] = here;
                heap.push(Reverse((cost + step, nx, ny, next.index())));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_path() {
        let canvas = Canvas::new(5, 5);
        let blocked = Blocked::new(5, 5);
        let path = route(&canvas, &blocked, (2, 1), Dir::S, (2, 3), Dir::S).unwrap();
        assert_eq!(path, [(2, 1), (2, 2), (2, 3)]);
    }

    #[test]
    fn path_goes_around_a_node() {
        let canvas = Canvas::new(7, 5);
        let mut blocked = Blocked::new(7, 5);
        blocked.block_rect(2, 2, 3, 1);
        let path = route(&canvas, &blocked, (3, 1), Dir::S, (3, 3), Dir::W).unwrap();
        assert!(path.iter().all(|&(x, y)| !(y == 2 && (2..5).contains(&x))));
        assert_eq!(path.first(), Some(&(3, 1)));
        assert_eq!(path.last(), Some(&(3, 3)));
    }

    #[test]
    fn unreachable_goal() {
        let canvas = Canvas::new(3, 3);
        let mut blocked = Blocked::new(3, 3);
        blocked.block_rect(0, 1, 3, 1);
        assert_eq!(
            route(&canvas, &blocked, (1, 0), Dir::S, (1, 2), Dir::S),
            None
        );
    }
}
