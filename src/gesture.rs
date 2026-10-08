//! 自定义触控轨迹与多指采样编译。
//!
//! [`GesturePath`] 描述起点保持、移动和暂停，[`Gesture`] 组合多根手指并配置采样间隔
//! 与注入速度。执行前按照当前显示区域解析位置，采样并补齐较短轨迹，生成设备
//! PointerMatrix 所需的坐标与时间编码；实际注入入口为 [`crate::HmDriver::perform_gesture`]。

use crate::{DisplaySize, DriverError, Point, Position, Result};
use std::time::Duration;
use tracing::trace;

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
///
/// 每个步骤时长大于零且不超过 60 秒；非法时长返回 [`DriverError::InvalidGesture`]。
/// 位置在注入前按照当前显示尺寸解析，移动与暂停按 [`Gesture::sample_interval`] 采样。
#[derive(Clone, Debug)]
pub struct GesturePath {
    steps: Vec<GestureStep>,
}

impl GesturePath {
    /// 创建一个新的手势路径，从指定位置开始并保持指定时长。
    pub fn new(position: Position, hold: Duration) -> Result<Self> {
        trace!(target: "hm_driver_rs::gesture", ?position, ?hold, "手势路径起点");
        validate_duration(hold)?;
        Ok(Self {
            steps: vec![GestureStep::Start { position, hold }],
        })
    }

    /// 在当前路径上添加一个移动到指定位置的步骤，动画持续时长为 `duration`。
    pub fn move_to(mut self, position: Position, duration: Duration) -> Result<Self> {
        trace!(target: "hm_driver_rs::gesture", ?position, ?duration, "手势路径移动到");
        validate_duration(duration)?;
        self.steps.push(GestureStep::Move { position, duration });
        Ok(self)
    }

    /// 在当前路径上添加一个暂停步骤，在当前位置保持指定时长。
    pub fn pause(mut self, duration: Duration) -> Result<Self> {
        trace!(target: "hm_driver_rs::gesture", ?duration, "手势路径暂停");
        validate_duration(duration)?;
        self.steps.push(GestureStep::Pause { duration });
        Ok(self)
    }
}

/// 可同时包含多根手指轨迹的手势。
///
/// 默认采样间隔为 50 毫秒，注入速度为 `2000`。较短路径在终点补齐至最长路径；
/// 编译后每根手指最多 10000 个点，超出范围返回 [`DriverError::InvalidGesture`]。
/// 时间以整数毫秒编码，轨迹采样按间隔离散化。
///
/// ```
/// use hm_driver_rs::{Gesture, GesturePath, Position};
/// use std::time::Duration;
/// let path = GesturePath::new(Position::normalized(0.2, 0.5)?, Duration::from_millis(100))?
///     .move_to(Position::normalized(0.8, 0.5)?, Duration::from_millis(300))?
///     .pause(Duration::from_millis(100))?;
/// let gesture = Gesture::new(path).injection_speed(2000)?;
/// # Ok::<(), hm_driver_rs::DriverError>(())
/// ```
#[derive(Clone, Debug)]
pub struct Gesture {
    paths: Vec<GesturePath>,
    sample_interval: Duration,
    injection_speed: u32,
}

impl Gesture {
    /// 创建一个新手势，包含指定的单根手指轨迹。
    pub fn new(path: GesturePath) -> Self {
        trace!(target: "hm_driver_rs::gesture", "新手势");
        Self {
            paths: vec![path],
            sample_interval: DEFAULT_SAMPLE_INTERVAL,
            injection_speed: 2_000,
        }
    }

    /// 为手势添加一根新的手指轨迹，最多支持 10 根手指。
    ///
    /// 超过上限返回 [`DriverError::InvalidGesture`]。
    pub fn add_path(mut self, path: GesturePath) -> Result<Self> {
        trace!(target: "hm_driver_rs::gesture", total_paths = self.paths.len() + 1, "添加手指轨迹");
        if self.paths.len() >= MAX_FINGERS {
            return Err(DriverError::InvalidGesture(format!(
                "手指数量不能超过 {MAX_FINGERS}"
            )));
        }
        self.paths.push(path);
        Ok(self)
    }

    /// 设置手势轨迹的采样间隔（10～100 毫秒）。
    ///
    /// 按整数毫秒验证范围，超出范围返回 [`DriverError::InvalidGesture`]；默认 50 毫秒。
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

    /// 设置手势注入速度（200～40000）。
    ///
    /// 超出范围返回 [`DriverError::InvalidGesture`]；默认 `2000`。
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

    pub(crate) fn compile(&self, display_size: DisplaySize) -> Result<Vec<Vec<EncodedPoint>>> {
        let sample_millis = u32::try_from(self.sample_interval.as_millis())
            .map_err(|_| DriverError::InvalidGesture("采样间隔超出范围".into()))?;
        trace!(target: "hm_driver_rs::gesture", width = display_size.width, height = display_size.height, "编译手势");
        let mut matrix = self
            .paths
            .iter()
            .map(|path| compile_path(path, display_size, sample_millis))
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
