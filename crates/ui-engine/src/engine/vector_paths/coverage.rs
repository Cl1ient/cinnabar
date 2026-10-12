//! Pretransformed segment bounds and single-sample silhouette coverage.

use super::Key;

struct Segment {
    a: [f32; 2],
    b: [f32; 2],
    delta: [f32; 2],
    length_squared: f32,
    min: [f32; 2],
    max: [f32; 2],
}

impl Segment {
    /// Prepares finite segment arithmetic and its conservative distance bounds once.
    fn new(a: [f32; 2], b: [f32; 2]) -> Option<Self> {
        let delta = [b[0] - a[0], b[1] - a[1]];
        let length_squared = delta[0] * delta[0] + delta[1] * delta[1];
        if !length_squared.is_finite() {
            return None;
        }
        Some(Self {
            a,
            b,
            delta,
            length_squared,
            min: [a[0].min(b[0]), a[1].min(b[1])],
            max: [a[0].max(b[0]), a[1].max(b[1])],
        })
    }

    /// Rejects edges whose bounding box cannot improve the nearest relevant squared distance.
    fn candidate(&self, point: [f32; 2], distance_squared: f32) -> bool {
        let delta = std::array::from_fn::<_, 2, _>(|axis| {
            (self.min[axis] - point[axis])
                .max(point[axis] - self.max[axis])
                .max(0.0)
        });
        delta[0] * delta[0] + delta[1] * delta[1] <= distance_squared
    }

    /// The squared distance to a closed segment, including coincident endpoints.
    fn distance_squared(&self, point: [f32; 2]) -> f32 {
        let t = if self.length_squared > 0.0 {
            (((point[0] - self.a[0]) * self.delta[0] + (point[1] - self.a[1]) * self.delta[1])
                / self.length_squared)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let delta = [
            point[0] - self.a[0] - t * self.delta[0],
            point[1] - self.a[1] - t * self.delta[1],
        ];
        delta[0] * delta[0] + delta[1] * delta[1]
    }

    /// Whether the horizontal parity ray crosses this segment.
    fn crosses(&self, point: [f32; 2]) -> bool {
        (self.a[1] > point[1]) != (self.b[1] > point[1])
            && point[0] < self.a[0] + ((point[1] - self.a[1]) / self.delta[1]) * self.delta[0]
    }
}

pub(super) struct Coverage {
    segments: Vec<Segment>,
    bounds: Option<([f32; 2], [f32; 2])>,
    half_stroke: Option<f32>,
    pub(super) pixel: f32,
}

impl Coverage {
    /// Converts authored coordinates once; geometry distances thereafter use logical pixels.
    pub(super) fn new(key: &Key) -> Option<Self> {
        let size = key.size.map(f32::from_bits);
        let [x, y, width, height] = key.path.view_box.map(f32::from_bits);
        let scale = [size[0] / width, size[1] / height];
        let pixel = 1.0 / f32::from_bits(key.dpi);
        let half_stroke = key
            .path
            .stroke_width
            .map(|width| f32::from_bits(width) * scale[0].min(scale[1]) * 0.5);
        if !pixel.is_finite() || pixel <= 0.0 || half_stroke.is_some_and(|width| !width.is_finite())
        {
            return None;
        }
        let mut segments = Vec::with_capacity(key.path.contours.iter().map(Vec::len).sum());
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for contour in &key.path.contours {
            let points: Vec<_> = contour
                .iter()
                .map(|point| {
                    let point = point.map(f32::from_bits);
                    [(point[0] - x) * scale[0], (point[1] - y) * scale[1]]
                })
                .collect();
            if points.iter().flatten().any(|value| !value.is_finite()) {
                return None;
            }
            for point in &points {
                for axis in 0..2 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
            let count = if key.path.closed || half_stroke.is_none() {
                points.len()
            } else {
                points.len() - 1
            };
            for index in 0..count {
                segments.push(Segment::new(
                    points[index],
                    points[(index + 1) % points.len()],
                )?);
            }
        }
        let bounds = (!segments.is_empty()).then_some((min, max));
        Some(Self {
            segments,
            bounds,
            half_stroke,
            pixel,
        })
    }

    /// Raster work is admitted against every segment, even when bounds reject most candidates.
    pub(super) fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Includes round caps, outside-view-box points, and half a physical pixel of coverage support.
    pub(super) fn raster_bounds(&self) -> Option<([f32; 2], [f32; 2])> {
        let (min, max) = self.bounds?;
        let padding = self.half_stroke.unwrap_or(0.0) + self.pixel * 0.5;
        Some((
            min.map(|value| value - padding),
            max.map(|value| value + padding),
        ))
    }

    /// Covers each silhouette once, including holes, overlapping strokes and rounded joins.
    pub(super) fn at(&self, point: [f32; 2]) -> f32 {
        let limit = self.half_stroke.unwrap_or(0.0) + self.pixel * 0.5;
        let mut distance_squared = limit * limit;
        let mut inside = false;
        for segment in &self.segments {
            if self.half_stroke.is_none() && segment.crosses(point) {
                inside = !inside;
            }
            if segment.candidate(point, distance_squared) {
                distance_squared = distance_squared.min(segment.distance_squared(point));
            }
        }
        let distance = distance_squared.sqrt();
        let signed = self.half_stroke.map_or_else(
            || if inside { distance } else { -distance },
            |width| width - distance,
        );
        let coverage = (0.5 + signed / self.pixel).clamp(0.0, 1.0);
        self.half_stroke.map_or(coverage, |width| {
            coverage.min((2.0 * width / self.pixel).min(1.0))
        })
    }
}
