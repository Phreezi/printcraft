//! Middle-button autoscroll for the document viewport and the page grid, on every platform:
//! press the wheel and the view scrolls toward the pointer, faster the farther it goes, like a
//! joystick. A click latches it on until the next click; press, drag and release stops on release.

use egui::{Context, CursorIcon, Event, Key, PointerButton, Pos2, Stroke, Vec2, vec2};

const DEAD_ZONE: f32 = 15.0;
// Chromium's autoscroll_controller.cc uses distance^2.2 * 0.000008; its
// ui/events/gestures/fixed_velocity_curve.cc multiplies elapsed seconds by 5000.
const SPEED_EXPONENT: f32 = 2.2;
const SPEED_MULTIPLIER: f32 = 0.04;
// Bound hostile coordinates before exponentiation, far beyond ordinary screen distances.
const MAX_DISPLACEMENT: f32 = 1_000_000.0;

#[derive(Default)]
pub(crate) struct AutoScroll {
    anchor: Option<Pos2>,
    organize: bool,
    /// The content can also move sideways, so the marker and cursor show four directions.
    horizontal: bool,
    /// The press that started scrolling is still held.
    holding: bool,
    /// While held, the pointer left the dead zone: releasing then stops (press, drag, release),
    /// while a plain click latches scrolling on until the next click.
    dragged: bool,
    /// Per-axis direction of the last motion (-1, 0 or 1), for the cursor.
    direction: Vec2,
    /// Own a cancelling click through its release, so it cannot also edit page content.
    cancel_button: Option<PointerButton>,
    block_input: bool,
}

impl AutoScroll {
    pub(crate) fn active(&self) -> bool {
        self.anchor.is_some()
    }

    pub(crate) fn cancel(&mut self) {
        self.anchor = None;
        self.holding = false;
        self.dragged = false;
        self.direction = Vec2::ZERO;
        self.cancel_button = None;
        self.block_input = false;
    }

    pub(crate) fn blocks_input(&self) -> bool {
        self.block_input
    }

    /// Run before the document's widgets. Starting is restricted to the unobstructed viewport;
    /// once started, moving outside that viewport still controls the speed. `horizontal` says
    /// whether the content is wider than the viewport (the page grid never scrolls sideways).
    pub(crate) fn update(&mut self, ui: &egui::Ui, viewport: egui::Rect, organize: bool, horizontal: bool) -> Vec2 {
        let ctx = ui.ctx();
        let (pointer, middle_press, middle_release, middle_down, middle_released, cancel, cancel_button, dt) = ctx.input(|i| {
            let cancel_button = [PointerButton::Primary, PointerButton::Secondary, PointerButton::Extra1, PointerButton::Extra2]
                .into_iter()
                .find(|button| i.pointer.button_pressed(*button));
            (
                i.pointer.hover_pos(),
                // Read the press event itself: later movement in this frame must not move the anchor.
                i.events.iter().find_map(|event| match event {
                    Event::PointerButton { pos, button: PointerButton::Middle, pressed: true, .. } if pos.is_finite() => Some(*pos),
                    _ => None,
                }),
                i.events.iter().find_map(|event| match event {
                    Event::PointerButton { pos, button: PointerButton::Middle, pressed: false, .. } if pos.is_finite() => Some(*pos),
                    _ => None,
                }),
                i.pointer.button_down(PointerButton::Middle),
                i.pointer.button_released(PointerButton::Middle),
                !i.focused
                    || i.key_pressed(Key::Escape)
                    || cancel_button.is_some()
                    || i.events.iter().any(|e| matches!(e, Event::MouseWheel { .. } | Event::Zoom(_))),
                cancel_button,
                i.stable_dt,
            )
        });
        // The middle button never selects, draws or drags anything while it scrolls.
        self.block_input = self.active() || self.cancel_button.is_some() || middle_press.is_some() || middle_down || middle_released;
        if let Some(button) = self.cancel_button {
            if !ctx.input(|i| i.pointer.button_down(button)) {
                self.cancel_button = None;
            }
            return Vec2::ZERO;
        }
        if self.active() && (cancel || pointer.is_none() || self.organize != organize || ctx.egui_wants_keyboard_input()) {
            self.cancel();
            self.block_input = true;
            self.cancel_button = cancel_button;
            return Vec2::ZERO;
        }
        if let Some(pressed_at) = middle_press {
            if self.active() {
                self.cancel();
                self.block_input = true;
                self.cancel_button = Some(PointerButton::Middle);
                return Vec2::ZERO;
            } else if !cancel
                && !ctx.egui_wants_keyboard_input()
                && viewport.intersect(ui.clip_rect()).contains(pressed_at)
                && ctx.layer_id_at(pressed_at) == Some(ui.layer_id())
            {
                self.anchor = Some(pressed_at);
                self.organize = organize;
                self.holding = true;
                self.dragged = false;
            }
        }
        let Some(anchor) = self.anchor else { return Vec2::ZERO };
        self.horizontal = horizontal && !organize;
        if self.holding {
            if pointer.is_some_and(|p| outside_dead_zone(p - anchor)) {
                self.dragged = true;
            }
            if middle_released || (!middle_down && middle_press.is_none()) {
                // Press, drag, release stops where it was released; a click latches it on.
                if self.dragged || middle_release.is_some_and(|p| outside_dead_zone(p - anchor)) {
                    self.cancel();
                    self.block_input = true;
                    return Vec2::ZERO;
                }
                self.holding = false;
            }
        }
        let Some(pointer) = pointer else { return Vec2::ZERO };
        let displacement = pointer - anchor;
        let displacement = if self.horizontal { displacement } else { vec2(0.0, displacement.y) };
        // Cap the elapsed time too: returning from an idle/hidden window must never jump pages.
        let delta = vec2(scroll_delta(displacement.x, dt), scroll_delta(displacement.y, dt));
        self.direction = vec2(-delta.x.signum() * f32::from(delta.x != 0.0), -delta.y.signum() * f32::from(delta.y != 0.0));
        if delta != Vec2::ZERO {
            // Continuous redraws let stable_dt use measured frame time. Delayed redraws
            // instead use predicted_dt, which can make speed depend on the actual frame rate.
            ctx.request_repaint();
        }
        delta
    }

    /// Draw an original geometric marker at the activation point, above page content.
    pub(crate) fn paint(&self, ui: &egui::Ui, viewport: egui::Rect) {
        let Some(anchor) = self.anchor else { return };
        let painter = ui.painter().with_clip_rect(viewport);
        let ink = ui.visuals().text_color();
        painter.circle(anchor, 13.0, ui.visuals().window_fill(), Stroke::new(1.0, ink));
        painter.circle_filled(anchor, 2.0, ink);
        let mut arrows = vec![vec2(0.0, -1.0), vec2(0.0, 1.0)];
        if self.horizontal {
            arrows.extend([vec2(-1.0, 0.0), vec2(1.0, 0.0)]);
        }
        for d in arrows {
            // The arrow tip points along `d`; its base is perpendicular to it.
            let side = vec2(d.y, d.x) * 4.0;
            painter.add(egui::Shape::convex_polygon(vec![anchor + d * 6.0 - side, anchor + d * 6.0 + side, anchor + d * 10.0], ink, Stroke::NONE));
        }
        ui.ctx().set_cursor_icon(cursor(self.direction, self.horizontal));
    }

    /// Escape belongs to autoscroll first, leaving selection/find/full-screen intact.
    pub(crate) fn escape(&mut self, ctx: &Context) -> bool {
        if self.active() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.cancel();
            true
        } else {
            false
        }
    }
}

fn outside_dead_zone(d: Vec2) -> bool {
    d.is_finite() && (d.x.abs() > DEAD_ZONE || d.y.abs() > DEAD_ZONE)
}

/// The cursor points where the content is heading; at rest it shows the available axes.
fn cursor(direction: Vec2, horizontal: bool) -> CursorIcon {
    let (x, y) = (direction.x as i8, direction.y as i8);
    match (x, y) {
        (0, -1) => CursorIcon::ResizeNorth,
        (0, 1) => CursorIcon::ResizeSouth,
        (-1, 0) => CursorIcon::ResizeWest,
        (1, 0) => CursorIcon::ResizeEast,
        (-1, -1) => CursorIcon::ResizeNorthWest,
        (1, -1) => CursorIcon::ResizeNorthEast,
        (-1, 1) => CursorIcon::ResizeSouthWest,
        (1, 1) => CursorIcon::ResizeSouthEast,
        _ if horizontal => CursorIcon::AllScroll,
        _ => CursorIcon::ResizeVertical,
    }
}

fn scroll_delta(displacement: f32, dt: f32) -> f32 {
    if !displacement.is_finite() || !dt.is_finite() {
        return 0.0;
    }
    let distance = displacement.abs();
    if distance <= DEAD_ZONE {
        return 0.0;
    }
    // Chromium uses the full distance outside the dead zone, without subtracting its radius.
    let speed = distance.min(MAX_DISPLACEMENT).powf(SPEED_EXPONENT) * SPEED_MULTIPLIER;
    // egui's delta moves content, the opposite of the scroll offset.
    -displacement.signum() * speed * dt.clamp(0.0, 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_points_where_the_pointer_went_and_rest_shows_the_axes() {
        assert_eq!(cursor(vec2(0.0, 1.0), false), CursorIcon::ResizeSouth);
        assert_eq!(cursor(vec2(0.0, -1.0), true), CursorIcon::ResizeNorth);
        assert_eq!(cursor(vec2(1.0, 1.0), true), CursorIcon::ResizeSouthEast);
        assert_eq!(cursor(vec2(-1.0, 0.0), true), CursorIcon::ResizeWest);
        assert_eq!(cursor(Vec2::ZERO, true), CursorIcon::AllScroll);
        assert_eq!(cursor(Vec2::ZERO, false), CursorIcon::ResizeVertical);
        assert_eq!(cursor(vec2(f32::NAN, f32::INFINITY), false), CursorIcon::ResizeVertical);
    }

    #[test]
    fn dead_zone_is_per_axis_and_rejects_invalid_input() {
        assert!(!outside_dead_zone(vec2(15.0, -15.0)));
        assert!(outside_dead_zone(vec2(15.1, 0.0)));
        assert!(outside_dead_zone(vec2(0.0, -40.0)));
        assert!(!outside_dead_zone(vec2(f32::NAN, 100.0)));
        assert!(!outside_dead_zone(vec2(f32::INFINITY, 0.0)));
    }

    #[test]
    fn chromium_curve_is_gentle_near_the_anchor_and_accelerates_farther_away() {
        // Reference speeds in screen points/second from Chromium's distance exponent (2.2),
        // controller multiplier (0.000008), and fixed-velocity animation multiplier (5000).
        for (distance, expected_speed) in [(25.0, 48.0), (50.0, 219.0), (100.0, 1005.0), (200.0, 4617.0)] {
            let speed = -scroll_delta(distance, 0.01) / 0.01;
            assert!((speed - expected_speed).abs() < 1.0, "distance={distance}, speed={speed}, expected={expected_speed}");
        }
        for distance in [-15.0, 0.0, 15.0] {
            assert_eq!(scroll_delta(distance, 0.01), 0.0);
        }
        assert!(scroll_delta(15.1, 0.01) < 0.0);
    }

    #[test]
    fn fractional_motion_covers_the_same_distance_at_different_frame_rates() {
        for distance in [16.0, 50.0, 200.0] {
            let expected = scroll_delta(distance, 0.01) * 100.0;
            for frames in [30, 60, 120, 144] {
                let delta = scroll_delta(distance, 1.0 / frames as f32);
                let travelled: f32 = (0..frames).map(|_| delta).sum();
                assert!((travelled - expected).abs() < expected.abs() * 0.00001);
            }
        }
        assert!(scroll_delta(16.0, 1.0 / 144.0).abs() < 1.0);
    }

    #[test]
    fn speed_has_a_dead_zone_is_symmetric_and_rejects_invalid_input() {
        for y in [-15.0, -1.0, 0.0, 1.0, 15.0] {
            assert_eq!(scroll_delta(y, 0.016), 0.0);
        }
        assert!(scroll_delta(30.0, 0.016) < 0.0);
        for (near, far) in [(16.0, 30.0), (30.0, 100.0), (100.0, 200.0)] {
            assert!(scroll_delta(far, 0.016).abs() > scroll_delta(near, 0.016).abs());
        }
        assert_eq!(scroll_delta(30.0, 0.016), -scroll_delta(-30.0, 0.016));
        assert!(scroll_delta(200.0, 0.01).abs() / 0.01 > 3200.0, "ordinary distances have no linear-curve speed ceiling");
        assert_eq!(scroll_delta(1000.0, 10.0), scroll_delta(1000.0, 0.05));
        assert_eq!(scroll_delta(1000.0, -1.0), 0.0);
        assert_eq!(scroll_delta(f32::MAX, 0.016), scroll_delta(MAX_DISPLACEMENT, 0.016));
        assert!(scroll_delta(f32::MAX, 0.016).is_finite());
        assert_eq!(scroll_delta(-f32::MAX, 0.016), -scroll_delta(f32::MAX, 0.016));
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(scroll_delta(invalid, 0.016), 0.0);
            assert_eq!(scroll_delta(1000.0, invalid), 0.0);
        }
    }
}
