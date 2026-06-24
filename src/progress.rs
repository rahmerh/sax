use std::io::{self, IsTerminal};

use indicatif::{ProgressBar, ProgressStyle};

pub trait ProgressReporter {
    fn start(&mut self, action: &'static str, total: Option<u64>);
    fn set_message(&mut self, message: String);
    fn inc(&mut self, bytes: u64);
    fn finish(&mut self);
}

#[derive(Default)]
pub struct NoProgress;

impl ProgressReporter for NoProgress {
    fn start(&mut self, _action: &'static str, _total: Option<u64>) {}
    fn set_message(&mut self, _message: String) {}
    fn inc(&mut self, _bytes: u64) {}
    fn finish(&mut self) {}
}

pub struct TerminalProgress {
    bar: Option<ProgressBar>,
}

impl TerminalProgress {
    pub fn stderr() -> Box<dyn ProgressReporter> {
        if io::stderr().is_terminal() {
            Box::new(Self { bar: None })
        } else {
            Box::<NoProgress>::default()
        }
    }
}

impl ProgressReporter for TerminalProgress {
    fn start(&mut self, action: &'static str, total: Option<u64>) {
        let bar = match total {
            Some(total) => {
                let bar = ProgressBar::new(total);
                bar.set_style(
                    ProgressStyle::with_template(
                        "{prefix:.cyan} [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg} {elapsed}",
                    )
                    .unwrap()
                    .progress_chars("=> "),
                );
                bar
            }
            None => {
                let bar = ProgressBar::new_spinner();
                bar.enable_steady_tick(std::time::Duration::from_millis(120));
                bar.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.cyan} {prefix:.cyan} {bytes} {msg} {elapsed}",
                    )
                    .unwrap(),
                );
                bar
            }
        };

        bar.set_prefix(action);
        self.bar = Some(bar);
    }

    fn set_message(&mut self, message: String) {
        if let Some(bar) = &self.bar {
            bar.set_message(message);
        }
    }

    fn inc(&mut self, bytes: u64) {
        if let Some(bar) = &self.bar {
            bar.inc(bytes);
        }
    }

    fn finish(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }
}
