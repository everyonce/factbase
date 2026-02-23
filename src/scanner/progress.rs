//! Progress bar wrapper - no-op when feature disabled

#[cfg(feature = "progress")]
mod inner {
    use indicatif::{ProgressBar, ProgressStyle};

    pub struct OptionalProgress(Option<ProgressBar>);

    impl OptionalProgress {
        pub fn new(len: u64, template: &str, msg: &str, min_threshold: u64) -> Self {
            if len >= min_threshold {
                let pb = ProgressBar::new(len);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template(template)
                        .expect("progress bar template should be valid")
                        .progress_chars("##-"),
                );
                pb.set_message(msg.to_string());
                Self(Some(pb))
            } else {
                Self(None)
            }
        }

        pub fn none() -> Self {
            Self(None)
        }

        pub fn set_position(&self, pos: u64) {
            if let Some(ref pb) = self.0 {
                pb.set_position(pos);
            }
        }

        pub fn finish_and_clear(&self) {
            if let Some(ref pb) = self.0 {
                pb.finish_and_clear();
            }
        }
    }
}

#[cfg(not(feature = "progress"))]
mod inner {
    pub struct OptionalProgress;

    impl OptionalProgress {
        pub fn new(_len: u64, _template: &str, _msg: &str, _min_threshold: u64) -> Self {
            Self
        }

        pub fn none() -> Self {
            Self
        }

        pub fn set_position(&self, _pos: u64) {}
        pub fn finish_and_clear(&self) {}
    }
}

pub use inner::OptionalProgress;
