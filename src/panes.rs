//! Split panes: how a tab's area is divided among its terminals. A binary
//! tree -- every split halves its rectangle side by side or one above the
//! other at a ratio the user can drag -- with pane ids at the leaves. Pure
//! geometry in physical pixels; `app.rs` keeps the sessions by id.

use serde::{Deserialize, Serialize, Serializer};

use crate::render::label::Rect;

/// How a split lays out its two halves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Side by side, the divider a vertical line.
    Horizontal,
    /// One above the other, the divider a horizontal line.
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Saved with the session (`crate::session`): a leaf as its number, a
/// split as a table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Node {
    Leaf(usize),
    Split {
        axis: Axis,
        #[serde(serialize_with = "rounded")]
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// A ratio to a thousandth -- a dragged 0.3 would be written as
/// 0.30000001192092896.
fn rounded<S: Serializer>(ratio: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64((f64::from(*ratio) * 1000.0).round() / 1000.0)
}

/// The line between the two halves of a split, as laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divider {
    /// Which split, counted depth-first ([`Node::set_ratio`]).
    pub split: usize,
    pub axis: Axis,
    /// The line itself.
    pub rect: Rect,
    /// The whole split it divides.
    pub area: Rect,
}

impl Node {
    /// Every pane id, depth-first: left to right, top to bottom.
    pub fn ids(&self) -> Vec<usize> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, ids: &mut Vec<usize>) {
        match self {
            Node::Leaf(id) => ids.push(*id),
            Node::Split { first, second, .. } => {
                first.collect_ids(ids);
                second.collect_ids(ids);
            }
        }
    }

    /// Give every pane a new id.
    pub fn map_ids(&mut self, new: &impl Fn(usize) -> usize) {
        match self {
            Node::Leaf(id) => *id = new(*id),
            Node::Split { first, second, .. } => {
                first.map_ids(new);
                second.map_ids(new);
            }
        }
    }

    /// Ratios within 0 to 1; not a number counts as half.
    pub fn clamp_ratios(&mut self) {
        if let Node::Split { ratio, first, second, .. } = self {
            *ratio = if ratio.is_finite() { ratio.clamp(0.0, 1.0) } else { 0.5 };
            first.clamp_ratios();
            second.clamp_ratios();
        }
    }

    /// Put `new` beside or below pane `target`, each half as big. `false`
    /// if there's no such pane.
    pub fn split(&mut self, target: usize, new: usize, axis: Axis) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                let first = Box::new(Node::Leaf(target));
                *self = Node::Split { axis, ratio: 0.5, first, second: Box::new(Node::Leaf(new)) };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { first, second, .. } => first.split(target, new, axis) || second.split(target, new, axis),
        }
    }

    /// Take pane `id` out; its sibling gets the room. `false` if there's no
    /// such pane or it's the only one (a tree is never empty).
    pub fn remove(&mut self, id: usize) -> bool {
        let Node::Split { first, second, .. } = self else { return false };
        let keep = match (&**first, &**second) {
            (Node::Leaf(leaf), _) if *leaf == id => std::mem::replace(&mut **second, Node::Leaf(0)),
            (_, Node::Leaf(leaf)) if *leaf == id => std::mem::replace(&mut **first, Node::Leaf(0)),
            _ => return first.remove(id) || second.remove(id),
        };
        *self = keep;
        true
    }

    /// Each pane's rectangle within `area`, with `gap` pixels between
    /// neighbours for the divider. Edges land on whole pixels.
    pub fn layout(&self, area: Rect, gap: f32) -> Vec<(usize, Rect)> {
        let mut out = Vec::new();
        self.walk(area, gap, &mut 0, &mut |node, rect, _| {
            if let Node::Leaf(id) = node {
                out.push((*id, rect));
            }
        });
        out
    }

    /// The dividers within `area`, as [`Node::layout`] places them.
    pub fn dividers(&self, area: Rect, gap: f32) -> Vec<Divider> {
        let mut out = Vec::new();
        self.walk(area, gap, &mut 0, &mut |node, rect, split| {
            if let Node::Split { axis, .. } = node {
                let (first, _) = halves(node, rect, gap);
                let line = match axis {
                    Axis::Horizontal => Rect { x: first.x + first.w, w: gap, ..rect },
                    Axis::Vertical => Rect { y: first.y + first.h, h: gap, ..rect },
                };
                out.push(Divider { split, axis: *axis, rect: line, area: rect });
            }
        });
        out
    }

    /// Visit every node with its rectangle; splits with their depth-first
    /// number.
    fn walk(&self, rect: Rect, gap: f32, splits: &mut usize, visit: &mut impl FnMut(&Node, Rect, usize)) {
        visit(self, rect, *splits);
        if let Node::Split { first, second, .. } = self {
            *splits += 1;
            let (a, b) = halves(self, rect, gap);
            first.walk(a, gap, splits, visit);
            second.walk(b, gap, splits, visit);
        }
    }

    /// Move split number `split`'s divider so the first half gets `ratio`
    /// of the room. Returns whether there was such a split.
    pub fn set_ratio(&mut self, split: usize, ratio: f32) -> bool {
        self.set_ratio_counting(split, ratio.clamp(0.0, 1.0), &mut 0)
    }

    fn set_ratio_counting(&mut self, split: usize, ratio: f32, seen: &mut usize) -> bool {
        let Node::Split { ratio: r, first, second, .. } = self else { return false };
        if *seen == split {
            *r = ratio;
            return true;
        }
        *seen += 1;
        first.set_ratio_counting(split, ratio, seen) || second.set_ratio_counting(split, ratio, seen)
    }
}

/// The two halves of a split's `rect`.
fn halves(node: &Node, rect: Rect, gap: f32) -> (Rect, Rect) {
    let Node::Split { axis, ratio, .. } = node else { return (rect, rect) };
    match axis {
        Axis::Horizontal => {
            let w = ((rect.w - gap).max(0.0) * ratio).round();
            let second_w = (rect.w - w - gap).max(0.0);
            (Rect { w, ..rect }, Rect { x: rect.x + w + gap, w: second_w, ..rect })
        }
        Axis::Vertical => {
            let h = ((rect.h - gap).max(0.0) * ratio).round();
            let second_h = (rect.h - h - gap).max(0.0);
            (Rect { h, ..rect }, Rect { y: rect.y + h + gap, h: second_h, ..rect })
        }
    }
}

/// The ratio that puts `divider`'s line at `pos` (x for side by side, y
/// otherwise), keeping both halves at least `min` pixels.
pub fn ratio_at(divider: &Divider, pos: f32, gap: f32, min: f32) -> f32 {
    let (start, extent) = match divider.axis {
        Axis::Horizontal => (divider.area.x, divider.area.w),
        Axis::Vertical => (divider.area.y, divider.area.h),
    };
    let room = (extent - gap).max(1.0);
    let min = min.min(room / 2.0);
    ((pos - start - gap / 2.0).clamp(min, room - min)) / room
}

/// The pane next to `from` in `direction`: of those across that edge and
/// overlapping it, the nearest, then the one sharing the most of the edge.
pub fn neighbour(rects: &[(usize, Rect)], from: usize, direction: Direction) -> Option<usize> {
    let &(_, a) = rects.iter().find(|(id, _)| *id == from)?;
    let overlap = |s1: f32, e1: f32, s2: f32, e2: f32| e1.min(e2) - s1.max(s2);
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|&(id, b)| {
            let (gap, shared) = match direction {
                Direction::Left => (a.x - (b.x + b.w), overlap(a.y, a.y + a.h, b.y, b.y + b.h)),
                Direction::Right => (b.x - (a.x + a.w), overlap(a.y, a.y + a.h, b.y, b.y + b.h)),
                Direction::Up => (a.y - (b.y + b.h), overlap(a.x, a.x + a.w, b.x, b.x + b.w)),
                Direction::Down => (b.y - (a.y + a.h), overlap(a.x, a.x + a.w, b.x, b.x + b.w)),
            };
            (gap >= -0.5 && shared > 0.0).then_some((id, gap, shared))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1).then(y.2.total_cmp(&x.2)))
        .map(|(id, ..)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 10.0, y: 20.0, w: 801.0, h: 601.0 };

    fn rect_of(rects: &[(usize, Rect)], id: usize) -> Rect {
        rects.iter().find(|(i, _)| *i == id).unwrap().1
    }

    /// 0 | 1 above 2
    fn three() -> Node {
        let mut node = Node::Leaf(0);
        assert!(node.split(0, 1, Axis::Horizontal));
        assert!(node.split(1, 2, Axis::Vertical));
        node
    }

    #[test]
    fn splits_halve_the_pane_and_leave_a_gap() {
        let node = three();
        assert_eq!(node.ids(), [0, 1, 2]);
        let rects = node.layout(AREA, 1.0);
        assert_eq!(rect_of(&rects, 0), Rect { x: 10.0, y: 20.0, w: 400.0, h: 601.0 });
        assert_eq!(rect_of(&rects, 1), Rect { x: 411.0, y: 20.0, w: 400.0, h: 300.0 });
        assert_eq!(rect_of(&rects, 2), Rect { x: 411.0, y: 321.0, w: 400.0, h: 300.0 });

        let dividers = node.dividers(AREA, 1.0);
        assert_eq!(dividers.len(), 2);
        assert_eq!(dividers[0].rect, Rect { x: 410.0, y: 20.0, w: 1.0, h: 601.0 });
        assert_eq!((dividers[0].split, dividers[0].axis, dividers[0].area), (0, Axis::Horizontal, AREA));
        assert_eq!(dividers[1].rect, Rect { x: 411.0, y: 320.0, w: 400.0, h: 1.0 });
        assert_eq!(dividers[1].split, 1);
    }

    #[test]
    fn edges_stay_on_whole_pixels() {
        let mut node = three();
        node.set_ratio(0, 0.3333);
        for (_, rect) in node.layout(Rect { x: 0.5, y: 0.0, w: 999.0, h: 555.0 }, 2.0) {
            assert_eq!(rect.w.fract(), 0.0, "{rect:?}");
            assert_eq!(rect.h.fract(), 0.0, "{rect:?}");
        }
    }

    #[test]
    fn splitting_an_unknown_pane_changes_nothing() {
        let mut node = three();
        assert!(!node.split(7, 8, Axis::Vertical));
        assert_eq!(node, three());
    }

    #[test]
    fn removing_a_pane_gives_its_room_to_the_sibling() {
        let mut node = three();
        assert!(node.remove(1));
        assert_eq!(node.ids(), [0, 2]);
        let rects = node.layout(AREA, 1.0);
        assert_eq!(rect_of(&rects, 2), Rect { x: 411.0, y: 20.0, w: 400.0, h: 601.0 });

        let mut node = three();
        assert!(node.remove(0));
        assert_eq!(node.ids(), [1, 2]);
        assert!(matches!(node, Node::Split { axis: Axis::Vertical, .. }));

        assert!(!node.remove(9));
        assert!(node.remove(2));
        assert_eq!(node, Node::Leaf(1));
        assert!(!node.remove(1), "the last pane stays");
    }

    #[test]
    fn ratios_are_set_by_split_number() {
        let mut node = three();
        assert!(node.set_ratio(1, 0.25));
        assert!(!node.set_ratio(2, 0.5));
        let rects = node.layout(AREA, 1.0);
        assert_eq!(rect_of(&rects, 1).h, 150.0);
        assert_eq!(rect_of(&rects, 0).w, 400.0);
    }

    #[test]
    fn dragging_a_divider_keeps_both_halves_usable() {
        let node = three();
        let divider = node.dividers(AREA, 1.0)[0];
        let ratio = ratio_at(&divider, 210.5, 1.0, 50.0);
        assert!((ratio - 0.25).abs() < 1e-3, "{ratio}");
        assert_eq!(ratio_at(&divider, 0.0, 1.0, 50.0), 50.0 / 800.0);
        assert_eq!(ratio_at(&divider, 5000.0, 1.0, 50.0), 750.0 / 800.0);
        // Too small for the minimum on both sides: the middle.
        let tiny = Divider { area: Rect { w: 41.0, ..AREA }, ..divider };
        assert_eq!(ratio_at(&tiny, 0.0, 1.0, 50.0), 0.5);
    }

    #[test]
    fn neighbours_are_found_across_the_edge() {
        let rects = three().layout(AREA, 1.0);
        assert_eq!(neighbour(&rects, 0, Direction::Right), Some(1));
        assert_eq!(neighbour(&rects, 0, Direction::Left), None);
        assert_eq!(neighbour(&rects, 0, Direction::Up), None);
        assert_eq!(neighbour(&rects, 1, Direction::Down), Some(2));
        assert_eq!(neighbour(&rects, 2, Direction::Up), Some(1));
        assert_eq!(neighbour(&rects, 2, Direction::Left), Some(0));
        assert_eq!(neighbour(&rects, 1, Direction::Left), Some(0));
        assert_eq!(neighbour(&rects, 9, Direction::Left), None);
    }

    #[test]
    fn the_neighbour_sharing_more_of_the_edge_wins() {
        // 0 on the left; on the right 1 (short) above 2 (tall).
        let mut node = three();
        node.set_ratio(1, 0.2);
        let rects = node.layout(AREA, 1.0);
        assert_eq!(neighbour(&rects, 0, Direction::Right), Some(2));
    }
}
