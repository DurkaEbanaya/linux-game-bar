//! Bounded, transactional displacement of neighbouring metric windows.
use egui::{Pos2, Rect, Vec2};

/// Keep the dragged window fixed and attempt to move obstructed neighbours instead.
/// Failure returns no changes; the caller can retain its last feasible layout.
pub fn push_neighbours(
    rects: &[Option<Rect>],
    active: usize,
    screen: Rect,
    obstacle: Option<Rect>,
    grid: Option<f32>,
    motion: Vec2,
) -> Option<Vec<Option<Rect>>> {
    let mut result = rects.to_vec();
    let mut locked = vec![false; rects.len()];
    locked[active] = true;
    let mut budget = 512usize;
    let context = Placement {
        screen,
        obstacle,
        grid,
        motion,
    };
    clear_overlaps(active, &mut result, &mut locked, &context, &mut budget)?;
    Some(result)
}
struct Placement {
    screen: Rect,
    obstacle: Option<Rect>,
    grid: Option<f32>,
    motion: Vec2,
}
fn clear_overlaps(
    index: usize,
    rects: &mut Vec<Option<Rect>>,
    locked: &mut [bool],
    context: &Placement,
    budget: &mut usize,
) -> Option<()> {
    let rect = rects[index]?;
    for other in 0..rects.len() {
        let Some(blocked) = rects[other] else {
            continue;
        };
        if other == index || !rect.expand(3.).intersects(blocked) {
            continue;
        }
        if locked[other] {
            return None;
        }
        let down = |v: f32| context.grid.map_or(v, |s| (v / s).floor() * s);
        let up = |v: f32| context.grid.map_or(v, |s| (v / s).ceil() * s);
        let mut candidates = [
            Pos2::new(down(rect.left() - blocked.width() - 8.), blocked.top()),
            Pos2::new(up(rect.right() + 8.), blocked.top()),
            Pos2::new(blocked.left(), down(rect.top() - blocked.height() - 8.)),
            Pos2::new(blocked.left(), up(rect.bottom() + 8.)),
        ];
        for pos in &mut candidates {
            if let Some(step) = context.grid {
                pos.x = (pos.x / step).round() * step;
                pos.y = (pos.y / step).round() * step;
            }
        }
        // Prefer the shortest escape in the direction of dragging, but try other sides at edges.
        let score = |p: Pos2| {
            let delta = p - blocked.min;
            delta.length_sq()
                + if delta.dot(context.motion) < 0. {
                    10000.
                } else {
                    0.
                }
        };
        candidates.sort_by(|a, b| score(*a).total_cmp(&score(*b)));
        let mut placed = false;
        for pos in candidates {
            if *budget == 0 {
                return None;
            }
            *budget -= 1;
            let candidate = Rect::from_min_size(pos, blocked.size());
            if !context.screen.contains_rect(candidate)
                || context
                    .obstacle
                    .is_some_and(|o| o.expand(3.).intersects(candidate))
                || rects.iter().enumerate().any(|(i, r)| {
                    i != other && locked[i] && r.is_some_and(|r| r.expand(3.).intersects(candidate))
                })
            {
                continue;
            }
            let previous = rects.clone();
            rects[other] = Some(candidate);
            locked[other] = true;
            if clear_overlaps(other, rects, locked, context, budget).is_some() {
                placed = true;
                locked[other] = false;
                break;
            }
            locked[other] = false;
            *rects = previous;
        }
        if !placed {
            return None;
        }
    }
    Some(())
}
