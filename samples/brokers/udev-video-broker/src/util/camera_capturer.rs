use akri_shared::os::env_var::{ActualEnvVarQuery, EnvVarQuery};
use log::trace;
use rscam::Camera as RsCamera;
use rscam::Config;

/// Frames per second environment variable id
const FRAMES_PER_SECOND: &str = "FRAMES_PER_SECOND";
/// Resolution width environment variable id
const RESOLUTION_WIDTH: &str = "RESOLUTION_WIDTH";
/// Resolution height environment variable id
const RESOLUTION_HEIGHT: &str = "RESOLUTION_HEIGHT";
/// Image format environment variable id
const FORMAT: &str = "FORMAT";
/// Default is 1 fps
const DEFAULT_FRAMES_PER_SECOND: u32 = 10;
/// Default resolution width, which is also the default for rscam.
const DEFAULT_RESOLUTION_WIDTH: u32 = 640;
/// Default resolution height, which is also the default for rscam.
const DEFAULT_RESOLUTION_HEIGHT: u32 = 480;
/// Defailt format, which is also the default for rscam.
const DEFAULT_FORMAT: &str = "MJPG";

pub type Resolution = (u32, u32);
pub type Interval = (u32, u32);

/// This builds a rscamera from a specified devnode. Then, it gets desired format, resolution, and interval/fps settings from environment variables.
/// If the environment variables are not set, it will try to use default settings. If the camera does not support the defaults, the first supported setting will be used.
/// Finally, its starts the camera capturer with the selected settings and returns this camera.
pub fn build_and_start_camera_capturer(devnode: &str) -> RsCamera {
    trace!("build_and_start_camera_capturer - entered");
    let mut camera_capturer = RsCamera::new(devnode).unwrap();
    let env_var_query = ActualEnvVarQuery {};
    // Get camera formats and convert them from [u8] to String so can compare them with env and default format
    let format_options: Vec<String> = camera_capturer
        .formats()
        .map(|wformat| {
            std::str::from_utf8(&wformat.unwrap().format)
                .unwrap()
                .to_string()
        })
        .collect();
    let format_string = get_format(&env_var_query, format_options);
    let format = format_string[..].as_bytes();
    let resolution_info = camera_capturer.resolutions(format).unwrap();
    let resolution = get_resolution(&env_var_query, resolution_info);
    let interval_info = camera_capturer.intervals(format, resolution).unwrap();
    let interval = get_interval(&env_var_query, interval_info);
    trace!("build_and_start_camera_capturer - before starting camera");
    camera_capturer
        .start(&Config {
            interval,
            resolution,
            format,
            ..Default::default()
        })
        .unwrap();
    trace!("build_and_start_camera_capturer - after starting camera");
    camera_capturer
}

/// This gets the image format from an environment variable. If not set, it will use default. If default is not supported, uses first supported format.
fn get_format(env_var_query: &impl EnvVarQuery, format_options: Vec<String>) -> String {
    let format_to_find = match env_var_query.get_env_var(FORMAT) {
        Ok(format) => format,
        Err(_) => {
            trace!(
                "get_format - format not set ... trying to use {:?}",
                DEFAULT_FORMAT
            );
            DEFAULT_FORMAT.to_string()
        }
    };

    if !format_options.contains(&format_to_find) {
        if !format_options.contains(&DEFAULT_FORMAT.to_string()) {
            trace!(
                "get_format - camera does not support {:?} format, using {:?} format",
                DEFAULT_FORMAT,
                format_options[0]
            );
            format_options[0].clone()
        } else {
            trace!("get_format - using default {:?} format", DEFAULT_FORMAT);
            DEFAULT_FORMAT.to_string()
        }
    } else {
        trace!("get_format - using {:?} format", format_to_find);
        format_to_find
    }
}

/// This gets the desired interval/frames per second from an environment variable. If not set, it will use default. If default is not supported, uses first supported interval.
fn get_interval(env_var_query: &impl EnvVarQuery, interval_info: rscam::IntervalInfo) -> Interval {
    let fps_to_validate = match env_var_query.get_env_var(FRAMES_PER_SECOND) {
        Ok(res) => res.parse().unwrap(),
        Err(_) => {
            trace!("main - frames per second not set ... trying to use 10");
            DEFAULT_FRAMES_PER_SECOND
        }
    };
    let interval_to_validate = (1, fps_to_validate);

    let interval_options = get_interval_options(interval_info);

    // If the camera does not support env var interval or default, use first interval option
    if !interval_options.contains(&interval_to_validate) {
        trace!(
            "get_interval - camera does not support {:?} interval, using {:?} interval",
            interval_to_validate,
            interval_options[0]
        );
        interval_options[0]
    } else {
        trace!("get_interval - using {:?} interval", interval_to_validate);
        interval_to_validate
    }
}

/// This gets the intervals supported by the camera
fn get_interval_options(interval_info: rscam::IntervalInfo) -> Vec<Resolution> {
    match interval_info {
        rscam::IntervalInfo::Discretes(interval_options) => interval_options,
        rscam::IntervalInfo::Stepwise { min, max, step } => {
            let mut interval_options: Vec<(u32, u32)> = Vec::new();
            let width_step = step.0;
            let height_step = step.1;
            let min_width = min.0;
            let min_height = min.1;
            let max_height = max.1;
            let steps = (max_height - min_height) / height_step;
            for step_num in 0..steps {
                let curr_width = min_width + step_num * width_step;
                let curr_height = min_height + step_num * height_step;
                interval_options.push((curr_width, curr_height));
            }
            interval_options
        }
    }
}

/// This calls a function to get the desired resolution from an environment variable. If not set, it will use default. If default is not supported, uses first supported resolution.
fn get_resolution(
    env_var_query: &impl EnvVarQuery,
    resolution_info: rscam::ResolutionInfo,
) -> Resolution {
    let env_var_resolution = get_env_var_resolution(env_var_query);

    let resolution_to_validate = match env_var_resolution {
        Some(res) => res,
        None => (DEFAULT_RESOLUTION_WIDTH, DEFAULT_RESOLUTION_HEIGHT),
    };

    let resolution_options = get_resolution_options(resolution_info);

    // If the camera does not support env var resolution or default, use first resolution
    if !resolution_options.contains(&resolution_to_validate) {
        trace!(
            "get_resolution - camera does not support {:?} resolution, using {:?} resolution",
            resolution_to_validate,
            resolution_options[0]
        );
        resolution_options[0]
    } else {
        trace!(
            "get_resolution - using resolution {:?}",
            resolution_to_validate
        );
        resolution_to_validate
    }
}

/// This gets the desired resolution from an environment variable else returns None.
fn get_env_var_resolution(env_var_query: &impl EnvVarQuery) -> Option<Resolution> {
    let width = match env_var_query.get_env_var(RESOLUTION_WIDTH) {
        Ok(res) => res.parse().unwrap(),
        Err(_) => {
            trace!("get_env_var_resolution - resolution width not set");
            return None;
        }
    };
    let height = match env_var_query.get_env_var(RESOLUTION_HEIGHT) {
        Ok(res) => res.parse().unwrap(),
        Err(_) => {
            trace!("get_env_var_resolution - resolution height not set");
            return None;
        }
    };
    Some((width, height))
}

/// This gets the resolutions supported by the camera.
fn get_resolution_options(resolution_info: rscam::ResolutionInfo) -> Vec<Resolution> {
    match resolution_info {
        rscam::ResolutionInfo::Discretes(resolution_options) => resolution_options,
        rscam::ResolutionInfo::Stepwise { min, max, step } => {
            let mut resolution_options: Vec<(u32, u32)> = Vec::new();
            let width_step = step.0;
            let height_step = step.1;
            let min_width = min.0;
            let min_height = min.1;
            let max_width = max.0;
            let steps = (max_width - min_width) / width_step;
            for step_num in 0..steps {
                let curr_width = min_width + step_num * width_step;
                let curr_height = min_height + step_num * height_step;
                resolution_options.push((curr_width, curr_height));
            }
            resolution_options
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_shared::os::env_var::MockEnvVarQuery;
    use std::env::VarError;

    #[test]
    fn test_get_format() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock_query = MockEnvVarQuery::new();
        const MOCK_FORMAT: &str = "YUYV";
        let mut format_options: Vec<String> =
            vec!["OTHR".to_string(), "YUYV".to_string(), "MJPG".to_string()];

        // Test when env var set and camera supports that format
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FORMAT)
            .returning(move |_| Ok(MOCK_FORMAT.to_string()));
        assert_eq!(
            "YUYV".to_string(),
            get_format(&mock_query, format_options.clone())
        );

        // Test when env var not set but camera supports default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FORMAT)
            .returning(move |_| Err(VarError::NotPresent));

        assert_eq!(
            "MJPG".to_string(),
            get_format(&mock_query, format_options.clone())
        );

        // Test when env var not set and camera does not support default
        format_options.pop();
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FORMAT)
            .returning(move |_| Err(VarError::NotPresent));
        assert_eq!("OTHR".to_string(), get_format(&mock_query, format_options));

        // Test when env var set and camera does not support format nor the default one
        let minimal_format_options: Vec<String> = vec!["OTHR".to_string(), "BLAH".to_string()];
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FORMAT)
            .returning(move |_| Ok(MOCK_FORMAT.to_string()));
        // Should choose first one
        assert_eq!(
            "OTHR".to_string(),
            get_format(&mock_query, minimal_format_options)
        );
    }

    #[test]
    fn test_get_interval_stepwise() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock_query = MockEnvVarQuery::new();
        const MOCK_INTERVAL: &str = "3";

        // Test when env var set and camera supports that interval
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Ok(MOCK_INTERVAL.to_string()));
        assert_eq!(
            (1, 3),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Stepwise {
                    min: (1, 1),
                    max: (1, 30),
                    step: (0, 2),
                }
            )
        );

        // Test when env var not set but camera supports default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Err(VarError::NotPresent));

        assert_eq!(
            (1, 10),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Stepwise {
                    min: (1, 1),
                    max: (1, 30),
                    step: (0, 9),
                }
            )
        );

        // Test when env var not set and camera does not support default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Err(VarError::NotPresent));

        assert_eq!(
            // returns slowest interval
            (1, 1),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Stepwise {
                    min: (1, 1),
                    max: (1, 30),
                    step: (0, 2),
                }
            )
        );
        // Test when env var set and camera does not support that interval nor the default one
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Ok(MOCK_INTERVAL.to_string()));
        assert_eq!(
            (1, 1),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Stepwise {
                    min: (1, 1),
                    max: (1, 30),
                    step: (0, 5),
                }
            )
        );
    }

    #[test]
    fn test_get_interval_discrete() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock_query = MockEnvVarQuery::new();
        const MOCK_INTERVAL: &str = "3";
        // Test when env var set and camera supports that interval
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Ok(MOCK_INTERVAL.to_string()));
        assert_eq!(
            (1, 3),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Discretes(vec![(1, 1), (1, 3), (1, 5)])
            )
        );

        // Test when env var not set but camera supports default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Err(VarError::NotPresent));

        assert_eq!(
            (1, 10),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Discretes(vec![(1, 1), (1, 3), (1, 10)])
            )
        );

        // Test when env var not set and camera does not support default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Err(VarError::NotPresent));

        assert_eq!(
            // returns slowest interval
            (1, 1),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Discretes(vec![(1, 1), (1, 3), (1, 5)])
            )
        );

        // Test when env var set and camera does not support that interval nor the default one
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == FRAMES_PER_SECOND)
            .returning(move |_| Ok(MOCK_INTERVAL.to_string()));
        assert_eq!(
            (1, 1),
            get_interval(
                &mock_query,
                rscam::IntervalInfo::Discretes(vec![(1, 1), (1, 2), (1, 5)])
            )
        );
    }

    #[test]
    fn test_get_resolution_stepwise() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock_query = MockEnvVarQuery::new();
        const MOCK_RESOLUTION_WIDTH: &str = "424";
        const MOCK_RESOLUTION_HEIGHT: &str = "240";

        // Test when env var set and camera supports that interval
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Ok(MOCK_RESOLUTION_WIDTH.to_string()));
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_HEIGHT)
            .returning(move |_| Ok(MOCK_RESOLUTION_HEIGHT.to_string()));
        assert_eq!(
            (424, 240),
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Stepwise {
                    min: (224, 140),
                    max: (1280, 800),
                    step: (200, 100),
                }
            )
        );

        // Test when env var not set but camera supports default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Err(VarError::NotPresent));
        assert_eq!(
            (DEFAULT_RESOLUTION_WIDTH, DEFAULT_RESOLUTION_HEIGHT), // (640, 480)
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Stepwise {
                    min: (440, 280),
                    max: (1280, 800),
                    step: (200, 200),
                }
            )
        );

        // Test when env var not set and camera does not support default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Err(VarError::NotPresent));
        assert_eq!(
            (160, 120),
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Stepwise {
                    min: (160, 120),
                    max: (1280, 800),
                    step: (100, 100),
                }
            )
        );

        // Test when env var set and camera does not support that interval nor the default one
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Ok(MOCK_RESOLUTION_WIDTH.to_string()));
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_HEIGHT)
            .returning(move |_| Ok(MOCK_RESOLUTION_HEIGHT.to_string()));
        assert_eq!(
            (160, 120),
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Stepwise {
                    min: (160, 120),
                    max: (1280, 800),
                    step: (100, 100),
                }
            )
        );
    }

    #[test]
    fn test_get_resolution_discrete() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock_query = MockEnvVarQuery::new();
        const MOCK_RESOLUTION_WIDTH: &str = "424";
        const MOCK_RESOLUTION_HEIGHT: &str = "240";

        // Test when env var set and camera supports that interval
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Ok(MOCK_RESOLUTION_WIDTH.to_string()));
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_HEIGHT)
            .returning(move |_| Ok(MOCK_RESOLUTION_HEIGHT.to_string()));
        assert_eq!(
            (424, 240),
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Discretes(vec!((200, 100), (424, 240), (1000, 800)))
            )
        );

        // Test when env var not set but camera supports default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Err(VarError::NotPresent));
        assert_eq!(
            (DEFAULT_RESOLUTION_WIDTH, DEFAULT_RESOLUTION_HEIGHT), // (640, 480)
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Discretes(vec!(
                    (200, 100),
                    (424, 240),
                    (640, 480),
                    (1000, 800)
                ))
            )
        );

        // Test when env var not set and camera does not support default
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Err(VarError::NotPresent));
        assert_eq!(
            (200, 100),
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Discretes(vec!((200, 100), (450, 240), (1000, 800)))
            )
        );

        // Test when env var set and camera does not support that interval nor the default one
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_WIDTH)
            .returning(move |_| Ok(MOCK_RESOLUTION_WIDTH.to_string()));
        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == RESOLUTION_HEIGHT)
            .returning(move |_| Ok(MOCK_RESOLUTION_HEIGHT.to_string()));
        assert_eq!(
            (200, 100),
            get_resolution(
                &mock_query,
                rscam::ResolutionInfo::Discretes(vec!((200, 100), (500, 250), (1000, 800)))
            )
        );
    }
}
