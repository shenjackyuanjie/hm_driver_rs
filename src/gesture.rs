use crate::{DisplaySize, DriverError, Point, Position, Result};
use std::time::Duration;

const DEFAULT_SAMPLE_INTERVAL: Duration = Duration::from_millis(50);
const MIN_SAMPLE_MILLIS: u128 = 10;
const MAX_SAMPLE_MILLIS: u128 = 100;
const MAX_FINGERS: usize = 10;
const MAX_POINTS: usize = 10_000;

#[derive(Clone, Debug)]
enum GestureStep {
    Start {
        position: Position,
        hold: Duration,
    },
    Move {
        position: Position,
        duration: Duration,
    },
    Pause {
        duration: Duration,
    },
}

/// 一根手指的自定义轨迹。
#[derive(Clone, Debug)]
pub struct GesturePath {
    steps: Vec<GestureStep>,
}

impl GesturePath {
    pub fn new(position: Position, hold: Duration) -> Result<Self> {
        validate_duration(hold)?;
        Ok(Self {
            steps: vec![GestureStep::Start { position, hold }],
        })
    }

    pub fn move_to(mut self, position: Position, duration: Duration) -> Result<Self> {
        validate_duration(duration)?;
        self.steps.push(GestureStep::Move { position, duration });
        Ok(self)
    }

    pub fn pause(mut self, duration: Duration) -> Result<Self> {
        validate_duration(duration)?;
        self.steps.push(GestureStep::Pause { duration });
        Ok(self)
    }
}

/// 可同时包含多根手指轨迹的手势。
#[derive(Clone, Debug)]
pub struct Gesture {
    paths: Vec<GesturePath>,
    sample_interval: Duration,
    injection_speed: u32,
}

impl Gesture {
    pub fn new(path: GesturePath) -> Self {
        Self {
            paths: vec![path],
            sample_interval: DEFAULT_SAMPLE_INTERVAL,
            injection_speed: 2_000,
        }
    }

    pub fn add_path(mut self, path: GesturePath) -> Result<Self> {
        if self.paths.len() >= MAX_FINGERS {
            return Err(DriverError::InvalidGesture(format!(
                "手指数量不能超过 {MAX_FINGERS}"
            )));
        }
        self.paths.push(path);
        Ok(self)
    }

    pub fn sample_interval(mut self, interval: Duration) -> Result<Self> {
        let millis = interval.as_millis();
        if !(MIN_SAMPLE_MILLIS..=MAX_SAMPLE_MILLIS).contains(&millis) {
            return Err(DriverError::InvalidGesture(
                "采样间隔必须位于 10 到 100 毫秒".into(),
            ));
        }
        self.sample_interval = interval;
        Ok(self)
    }

    pub fn injection_speed(mut self, speed: u32) -> Result<Self> {
        if !(200..=40_000).contains(&speed) {
            return Err(DriverError::InvalidGesture(
                "注入速度必须位于 200 到 40000".into(),
            ));
        }
        self.injection_speed = speed;
        Ok(self)
    }

    pub(crate) fn injection_speed_value(&self) -> u32 {
        self.injection_speed
    }

    pub(crate) fn compile(&self, display: DisplaySize) -> Result<Vec<Vec<EncodedPoint>>> {
        let sample_millis = u32::try_from(self.sample_interval.as_millis())
            .map_err(|_| DriverError::InvalidGesture("采样间隔超出范围".into()))?;
        let mut matrix = self
            .paths
            .iter()
            .map(|path| compile_path(path, display, sample_millis))
            .collect::<Result<Vec<_>>>()?;
        let total_points = matrix.iter().map(Vec::len).max().unwrap_or_default();
        if total_points == 0 || total_points > MAX_POINTS {
            return Err(DriverError::InvalidGesture(format!(
                "轨迹采样点数量必须位于 1 到 {MAX_POINTS}"
            )));
        }
        for points in &mut matrix {
            while points.len() < total_points {
                let last = *points
                    .last()
                    .ok_or_else(|| DriverError::InvalidGesture("手指轨迹为空".into()))?;
                if let Some(previous) = points.last_mut() {
                    previous.interval_millis = sample_millis;
                }
                points.push(EncodedPoint {
                    interval_millis: 0,
                    ..last
                });
            }
        }
        Ok(matrix)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EncodedPoint {
    pub point: Point,
    pub interval_millis: u32,
}

impl EncodedPoint {
    pub(crate) fn encoded_x(self) -> Result<i32> {
        let value = i64::from(self.point.x) + 65_536_i64 * i64::from(self.interval_millis);
        i32::try_from(value)
            .map_err(|_| DriverError::InvalidGesture("轨迹时间编码超出 i32 范围".into()))
    }
}

fn compile_path(
    path: &GesturePath,
    display: DisplaySize,
    sample_millis: u32,
) -> Result<Vec<EncodedPoint>> {
    let mut points = Vec::new();
    let mut current = None;
    for step in &path.steps {
        match *step {
            GestureStep::Start { position, hold } => {
                let point = position.resolve(display)?;
                points.push(EncodedPoint {
                    point,
                    interval_millis: duration_millis(hold)?,
                });
                points.push(EncodedPoint {
                    point,
                    interval_millis: 0,
                });
                current = Some(point);
            }
            GestureStep::Move { position, duration } => {
                let from = current
                    .ok_or_else(|| DriverError::InvalidGesture("移动步骤之前缺少起点".into()))?;
                let to = position.resolve(display)?;
                let count = sample_count(duration, sample_millis)?;
                if let Some(last) = points.last_mut() {
                    last.interval_millis = sample_millis;
                }
                for index in 1..=count {
                    let ratio = f64::from(index) / f64::from(count);
                    points.push(EncodedPoint {
                        point: Point::new(
                            (f64::from(from.x) + f64::from(to.x - from.x) * ratio).round() as i32,
                            (f64::from(from.y) + f64::from(to.y - from.y) * ratio).round() as i32,
                        ),
                        interval_millis: if index == count { 0 } else { sample_millis },
                    });
                }
                current = Some(to);
            }
            GestureStep::Pause { duration } => {
                let point = current
                    .ok_or_else(|| DriverError::InvalidGesture("暂停步骤之前缺少起点".into()))?;
                let count = sample_count(duration, sample_millis)?;
                if let Some(last) = points.last_mut() {
                    last.interval_millis = sample_millis;
                }
                for index in 1..=count {
                    points.push(EncodedPoint {
                        point,
                        interval_millis: if index == count { 0 } else { sample_millis },
                    });
                }
            }
        }
    }
    Ok(points)
}

fn validate_duration(duration: Duration) -> Result<()> {
    if duration.is_zero() || duration > Duration::from_secs(60) {
        Err(DriverError::InvalidGesture(
            "单个轨迹步骤时长必须大于 0 且不超过 60 秒".into(),
        ))
    } else {
        Ok(())
    }
}

fn duration_millis(duration: Duration) -> Result<u32> {
    u32::try_from(duration.as_millis())
        .map_err(|_| DriverError::InvalidGesture("轨迹步骤时长超出范围".into()))
}

fn sample_count(duration: Duration, sample_millis: u32) -> Result<u32> {
    let millis = duration_millis(duration)?;
    Ok((millis / sample_millis).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NormalizedPoint;

    #[test]
    fn compiles_and_pads_multiple_paths() {
        let first = GesturePath::new(
            Position::Normalized(NormalizedPoint::new(0.2, 0.2).unwrap()),
            Duration::from_millis(100),
        )
        .unwrap()
        .move_to(
            Position::Normalized(NormalizedPoint::new(0.8, 0.8).unwrap()),
            Duration::from_millis(200),
        )
        .unwrap();
        let second = GesturePath::new(
            Position::Absolute(Point::new(50, 80)),
            Duration::from_millis(100),
        )
        .unwrap();
        let matrix = Gesture::new(first)
            .add_path(second)
            .unwrap()
            .compile(DisplaySize {
                width: 1000,
                height: 2000,
            })
            .unwrap();
        assert_eq!(matrix.len(), 2);
        assert_eq!(matrix[0].len(), matrix[1].len());
        assert_eq!(matrix[0][0].point, Point::new(200, 400));
    }
}
