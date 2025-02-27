use cap_project::{cursor::CursorEvents, ZoomSegment, XY};

pub const ZOOM_DURATION: f64 = 1.0;
// Added constant for cursor smoothing
pub const CURSOR_SMOOTHING_WINDOW: f64 = 0.15; // 150ms window for smoothing

#[derive(Debug, Clone, Copy)]
pub struct SegmentsCursor<'a> {
    time: f64,
    segment: Option<&'a ZoomSegment>,
    prev_segment: Option<&'a ZoomSegment>,
    segments: &'a [ZoomSegment],
}

impl<'a> SegmentsCursor<'a> {
    pub fn new(time: f64, segments: &'a [ZoomSegment]) -> Self {
        match segments
            .iter()
            .position(|s| time > s.start && time <= s.end)
        {
            Some(segment_index) => SegmentsCursor {
                time,
                segment: Some(&segments[segment_index]),
                prev_segment: if segment_index > 0 {
                    Some(&segments[segment_index - 1])
                } else {
                    None
                },
                segments,
            },
            None => {
                let prev = segments
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, s)| s.end <= time);
                SegmentsCursor {
                    time,
                    segment: None,
                    prev_segment: prev.map(|(_, s)| s),
                    segments,
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct SegmentBounds {
    pub top_left: XY<f64>,
    pub bottom_right: XY<f64>,
}

impl SegmentBounds {
    // Add current_time parameter to from_segment
    fn from_segment(
        segment: &ZoomSegment,
        current_time: f64,
        cursor_events: Option<&CursorEvents>,
    ) -> Self {
        println!(
            "Zoom mode: {:?}, segment time: {}, current time: {}",
            segment.mode, segment.start, current_time
        );

        // Add detailed debug info about cursor_events
        if let Some(events) = cursor_events {
            println!(
                "Cursor events available: {} move events",
                events.moves.len()
            );
            // Print first 3 move events to check timestamps
            if !events.moves.is_empty() {
                for i in 0..std::cmp::min(3, events.moves.len()) {
                    println!(
                        "Sample move event {}: time={}, pos=({}, {})",
                        i, events.moves[i].process_time_ms, events.moves[i].x, events.moves[i].y
                    );
                }
                // Print last event
                if events.moves.len() > 3 {
                    let last = &events.moves[events.moves.len() - 1];
                    println!(
                        "Last move event: time={}, pos=({}, {})",
                        last.process_time_ms, last.x, last.y
                    );
                }
            }
        } else {
            println!("No cursor events provided");
        }

        let position = match segment.mode {
            cap_project::ZoomMode::Auto => {
                // Use current_time instead of segment.start to get continuously changing cursor positions
                if let Some(events) = cursor_events {
                    println!("Looking for cursor position at time: {}", current_time);

                    // Get smoothed cursor position instead of exact position
                    if let Some(pos) =
                        get_smoothed_cursor_position(events, current_time, CURSOR_SMOOTHING_WINDOW)
                    {
                        println!("Found smoothed cursor position: ({}, {})", pos.0, pos.1);
                        pos
                    } else {
                        println!(
                            "No cursor position found at time: {}, defaulting to center",
                            current_time
                        );
                        (0.5, 0.5) // Fall back to center if no cursor data available
                    }
                } else {
                    println!("No cursor events provided, defaulting to center");
                    (0.5, 0.5) // Fall back to center if no cursor events provided
                }
            }
            cap_project::ZoomMode::Manual { x, y } => (x as f64, y as f64),
        };

        println!("Final position: ({}, {})", position.0, position.1);

        // Fix: Instead of defaulting to (0.0, 0.0), use (0.5, 0.5) as center
        // The rest of the function remains the same

        let scaled_center = [position.0 * segment.amount, position.1 * segment.amount];
        let center_diff = [scaled_center[0] - position.0, scaled_center[1] - position.1];

        SegmentBounds::new(
            XY::new(0.0 - center_diff[0], 0.0 - center_diff[1]),
            XY::new(
                segment.amount - center_diff[0],
                segment.amount - center_diff[1],
            ),
        )
    }

    pub fn new(top_left: XY<f64>, bottom_right: XY<f64>) -> Self {
        Self {
            top_left,
            bottom_right,
        }
    }

    pub fn default() -> Self {
        SegmentBounds::new(XY::new(0.0, 0.0), XY::new(1.0, 1.0))
    }
}

// New helper function to get smoothed cursor position
fn get_smoothed_cursor_position(
    events: &CursorEvents,
    time: f64,
    window: f64,
) -> Option<(f64, f64)> {
    // First try to get the exact position at the current time
    if let Some(pos) = events.cursor_position_at(time) {
        // Try to find positions within the smoothing window
        let start_time = time - window / 2.0;
        let end_time = time + window / 2.0;

        // Collect cursor positions within the time window
        let mut positions = Vec::new();
        let mut total_weight = 0.0;
        let mut weighted_x = 0.0;
        let mut weighted_y = 0.0;

        // Find positions in the time window
        for event in &events.moves {
            let event_time = event.process_time_ms / 1000.0; // Convert to seconds

            if event_time >= start_time && event_time <= end_time {
                // Calculate weight based on time proximity (closer to current time = higher weight)
                let time_diff = (time - event_time).abs();
                let weight = 1.0 - (time_diff / (window / 2.0)).min(1.0);

                positions.push((event.x, event.y, weight));
                total_weight += weight;
                weighted_x += event.x * weight;
                weighted_y += event.y * weight;
            }
        }

        // If we found positions in the window, return weighted average
        if !positions.is_empty() && total_weight > 0.0 {
            return Some((weighted_x / total_weight, weighted_y / total_weight));
        }

        // If no positions in window, use the exact position
        return Some((pos.x, pos.y));
    }

    // Try to interpolate between closest positions if exact position not found
    let mut before = None;
    let mut after = None;

    for event in &events.moves {
        let event_time = event.process_time_ms / 1000.0;

        if event_time <= time {
            // Find the closest event before the target time
            if let Some((prev_time, _, _)) = before {
                if event_time > prev_time {
                    before = Some((event_time, event.x, event.y));
                }
            } else {
                before = Some((event_time, event.x, event.y));
            }
        } else {
            // Find the closest event after the target time
            if let Some((next_time, _, _)) = after {
                if event_time < next_time {
                    after = Some((event_time, event.x, event.y));
                }
            } else {
                after = Some((event_time, event.x, event.y));
            }
        }
    }

    match (before, after) {
        // Interpolate between two points
        (Some((t1, x1, y1)), Some((t2, x2, y2))) => {
            // Calculate interpolation factor
            let t_diff = t2 - t1;
            if t_diff > 0.0 {
                let factor = (time - t1) / t_diff;

                // Linearly interpolate between the two positions
                let x = x1 + (x2 - x1) * factor;
                let y = y1 + (y2 - y1) * factor;

                Some((x, y))
            } else {
                // If timestamps are identical, just use one of the positions
                Some((x1, y1))
            }
        }
        // If we only have a position before the time
        (Some((_, x, y)), None) => Some((x, y)),
        // If we only have a position after the time
        (None, Some((_, x, y))) => Some((x, y)),
        // No positions at all
        (None, None) => None,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InterpolatedZoom {
    // the ratio of current zoom to the maximum amount for the current segment
    pub t: f64,
    pub bounds: SegmentBounds,
}

impl InterpolatedZoom {
    pub fn new(cursor: SegmentsCursor, cursor_events: Option<&CursorEvents>) -> Self {
        let ease_in = bezier_easing::bezier_easing(0.1, 0.0, 0.3, 1.0).unwrap();
        let ease_out = bezier_easing::bezier_easing(0.5, 0.0, 0.5, 1.0).unwrap();

        Self::new_with_easing(cursor, cursor_events, ease_in, ease_out)
    }

    // the multiplier applied to the display width/height
    pub fn display_amount(&self) -> f64 {
        (self.bounds.bottom_right - self.bounds.top_left).x
    }

    pub(self) fn new_with_easing(
        cursor: SegmentsCursor,
        cursor_events: Option<&CursorEvents>,
        ease_in: impl Fn(f32) -> f32,
        ease_out: impl Fn(f32) -> f32,
    ) -> InterpolatedZoom {
        let default = SegmentBounds::default();
        match (cursor.prev_segment, cursor.segment) {
            (Some(prev_segment), None) => {
                let zoom_t =
                    ease_out(t_clamp((cursor.time - prev_segment.end) / ZOOM_DURATION) as f32)
                        as f64;

                Self {
                    t: 1.0 - zoom_t,
                    bounds: {
                        let prev_segment_bounds =
                            SegmentBounds::from_segment(prev_segment, cursor.time, cursor_events);

                        SegmentBounds::new(
                            prev_segment_bounds.top_left * (1.0 - zoom_t)
                                + default.top_left * zoom_t,
                            prev_segment_bounds.bottom_right * (1.0 - zoom_t)
                                + default.bottom_right * zoom_t,
                        )
                    },
                }
            }
            (None, Some(segment)) => {
                let t =
                    ease_in(t_clamp((cursor.time - segment.start) / ZOOM_DURATION) as f32) as f64;

                Self {
                    t,
                    bounds: {
                        let segment_bounds =
                            SegmentBounds::from_segment(segment, cursor.time, cursor_events);

                        SegmentBounds::new(
                            default.top_left * (1.0 - t) + segment_bounds.top_left * t,
                            default.bottom_right * (1.0 - t) + segment_bounds.bottom_right * t,
                        )
                    },
                }
            }
            (Some(prev_segment), Some(segment)) => {
                let prev_segment_bounds =
                    SegmentBounds::from_segment(prev_segment, cursor.time, cursor_events);
                let segment_bounds =
                    SegmentBounds::from_segment(segment, cursor.time, cursor_events);

                let zoom_t =
                    ease_in(t_clamp((cursor.time - segment.start) / ZOOM_DURATION) as f32) as f64;

                // no gap
                if segment.start == prev_segment.end {
                    Self {
                        t: 1.0,
                        bounds: SegmentBounds::new(
                            prev_segment_bounds.top_left * (1.0 - zoom_t)
                                + segment_bounds.top_left * zoom_t,
                            prev_segment_bounds.bottom_right * (1.0 - zoom_t)
                                + segment_bounds.bottom_right * zoom_t,
                        ),
                    }
                }
                // small gap
                else if segment.start - prev_segment.end < ZOOM_DURATION {
                    // handling this is a bit funny, since we're not zooming in from 0 but rather
                    // from the previous value that the zoom out got interrupted at by the current segment

                    let min = InterpolatedZoom::new_with_easing(
                        SegmentsCursor::new(segment.start, cursor.segments),
                        cursor_events,
                        ease_in,
                        ease_out,
                    );

                    Self {
                        t: (min.t * (1.0 - zoom_t)) + zoom_t,
                        bounds: {
                            let max = segment_bounds;

                            SegmentBounds::new(
                                min.bounds.top_left * (1.0 - zoom_t) + max.top_left * zoom_t,
                                min.bounds.bottom_right * (1.0 - zoom_t)
                                    + max.bottom_right * zoom_t,
                            )
                        },
                    }
                }
                // entirely separate
                else {
                    Self {
                        t: zoom_t,
                        bounds: SegmentBounds::new(
                            default.top_left * (1.0 - zoom_t) + segment_bounds.top_left * zoom_t,
                            default.bottom_right * (1.0 - zoom_t)
                                + segment_bounds.bottom_right * zoom_t,
                        ),
                    }
                }
            }
            _ => Self {
                t: 0.0,
                bounds: default,
            },
        }
    }
}

fn t_clamp(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

#[cfg(test)]
mod test {
    use cap_project::ZoomMode;

    use super::*;

    // Custom macro for floating-point near equality
    macro_rules! assert_f64_near {
        ($left:expr, $right:expr, $label:literal) => {
            let left = $left;
            let right = $right;
            assert!(
                (left - right).abs() < 1e-6,
                "{}: `(left ~ right)` \n left: `{:?}`, \n right: `{:?}`",
                $label,
                left,
                right
            )
        };
        ($left:expr, $right:expr) => {
            assert_f64_near!($left, $right, "assertion failed");
        };
    }

    fn c(time: f64, segments: &[ZoomSegment]) -> SegmentsCursor {
        SegmentsCursor::new(time, segments)
    }

    fn test_interp((time, segments): (f64, &[ZoomSegment]), expected: InterpolatedZoom) {
        let actual = InterpolatedZoom::new_with_easing(c(time, segments), None, |t| t, |t| t);

        assert_f64_near!(actual.t, expected.t, "t");

        let a = &actual.bounds;
        let e = &expected.bounds;

        assert_f64_near!(a.top_left.x, e.top_left.x, "bounds.top_left.x");
        assert_f64_near!(a.top_left.y, e.top_left.y, "bounds.top_left.y");
        assert_f64_near!(a.bottom_right.x, e.bottom_right.x, "bounds.bottom_right.x");
        assert_f64_near!(a.bottom_right.y, e.bottom_right.y, "bounds.bottom_right.y");
    }

    #[test]
    fn one_segment() {
        let segments = vec![ZoomSegment {
            start: 2.0,
            end: 4.0,
            amount: 2.0,
            mode: ZoomMode::Manual { x: 0.5, y: 0.5 },
        }];

        test_interp(
            (0.0, &segments),
            InterpolatedZoom {
                t: 0.0,
                bounds: SegmentBounds::default(),
            },
        );
        test_interp(
            (2.0, &segments),
            InterpolatedZoom {
                t: 0.0,
                bounds: SegmentBounds::default(),
            },
        );
        test_interp(
            (2.0 + ZOOM_DURATION * 0.1, &segments),
            InterpolatedZoom {
                t: 0.1,
                bounds: SegmentBounds::new(XY::new(-0.05, -0.05), XY::new(1.05, 1.05)),
            },
        );
        test_interp(
            (2.0 + ZOOM_DURATION * 0.9, &segments),
            InterpolatedZoom {
                t: 0.9,
                bounds: SegmentBounds::new(XY::new(-0.45, -0.45), XY::new(1.45, 1.45)),
            },
        );
        test_interp(
            (2.0 + ZOOM_DURATION, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-0.5, -0.5), XY::new(1.5, 1.5)),
            },
        );
        test_interp(
            (4.0, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-0.5, -0.5), XY::new(1.5, 1.5)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * 0.2, &segments),
            InterpolatedZoom {
                t: 0.8,
                bounds: SegmentBounds::new(XY::new(-0.4, -0.4), XY::new(1.4, 1.4)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * 0.8, &segments),
            InterpolatedZoom {
                t: 0.2,
                bounds: SegmentBounds::new(XY::new(-0.1, -0.1), XY::new(1.1, 1.1)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION, &segments),
            InterpolatedZoom {
                t: 0.0,
                bounds: SegmentBounds::new(XY::new(0.0, 0.0), XY::new(1.0, 1.0)),
            },
        );
    }

    #[test]
    fn two_segments_no_gap() {
        let segments = vec![
            ZoomSegment {
                start: 2.0,
                end: 4.0,
                amount: 2.0,
                mode: ZoomMode::Manual { x: 0.0, y: 0.0 },
            },
            ZoomSegment {
                start: 4.0,
                end: 6.0,
                amount: 4.0,
                mode: ZoomMode::Manual { x: 0.5, y: 0.5 },
            },
        ];

        test_interp(
            (4.0, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(0.0, 0.0), XY::new(2.0, 2.0)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * 0.2, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-0.3, -0.3), XY::new(2.1, 2.1)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * 0.8, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-1.2, -1.2), XY::new(2.4, 2.4)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-1.5, -1.5), XY::new(2.5, 2.5)),
            },
        );
    }

    #[test]
    fn two_segments_small_gap() {
        let segments = vec![
            ZoomSegment {
                start: 2.0,
                end: 4.0,
                amount: 2.0,
                mode: ZoomMode::Manual { x: 0.5, y: 0.5 },
            },
            ZoomSegment {
                start: 4.0 + ZOOM_DURATION * 0.75,
                end: 6.0,
                amount: 4.0,
                mode: ZoomMode::Manual { x: 0.5, y: 0.5 },
            },
        ];

        test_interp(
            (4.0, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-0.5, -0.5), XY::new(1.5, 1.5)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * 0.5, &segments),
            InterpolatedZoom {
                t: 0.5,
                bounds: SegmentBounds::new(XY::new(-0.25, -0.25), XY::new(1.25, 1.25)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * 0.75, &segments),
            InterpolatedZoom {
                t: 0.25,
                bounds: SegmentBounds::new(XY::new(-0.125, -0.125), XY::new(1.125, 1.125)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * (0.75 + 0.5), &segments),
            InterpolatedZoom {
                t: 0.625,
                bounds: SegmentBounds::new(XY::new(-0.8125, -0.8125), XY::new(1.8125, 1.8125)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * (0.75 + 1.0), &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-1.5, -1.5), XY::new(2.5, 2.5)),
            },
        );
    }

    #[test]
    fn two_segments_large_gap() {
        let segments = vec![
            ZoomSegment {
                start: 2.0,
                end: 4.0,
                amount: 2.0,
                mode: ZoomMode::Manual { x: 0.5, y: 0.5 },
            },
            ZoomSegment {
                start: 7.0,
                end: 9.0,
                amount: 4.0,
                mode: ZoomMode::Manual { x: 0.0, y: 0.0 },
            },
        ];

        test_interp(
            (4.0, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(-0.5, -0.5), XY::new(1.5, 1.5)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION * 0.5, &segments),
            InterpolatedZoom {
                t: 0.5,
                bounds: SegmentBounds::new(XY::new(-0.25, -0.25), XY::new(1.25, 1.25)),
            },
        );
        test_interp(
            (4.0 + ZOOM_DURATION, &segments),
            InterpolatedZoom {
                t: 0.0,
                bounds: SegmentBounds::new(XY::new(0.0, 0.0), XY::new(1.0, 1.0)),
            },
        );
        test_interp(
            (7.0, &segments),
            InterpolatedZoom {
                t: 0.0,
                bounds: SegmentBounds::new(XY::new(0.0, 0.0), XY::new(1.0, 1.0)),
            },
        );
        test_interp(
            (7.0 + ZOOM_DURATION * 0.5, &segments),
            InterpolatedZoom {
                t: 0.5,
                bounds: SegmentBounds::new(XY::new(0.0, 0.0), XY::new(2.5, 2.5)),
            },
        );
        test_interp(
            (7.0 + ZOOM_DURATION * 1.0, &segments),
            InterpolatedZoom {
                t: 1.0,
                bounds: SegmentBounds::new(XY::new(0.0, 0.0), XY::new(4.0, 4.0)),
            },
        );
    }
}
