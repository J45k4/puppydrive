use std::pin::Pin;
use std::time::Duration;
use tokio::time::Sleep;


pub struct Timer {
	pub repeat: bool,
	pub duration: Duration,
	timeout: Option<Pin<Box<Sleep>>>,
}

impl Timer {
	pub fn new() -> Self {
		Self {
			repeat: false,
			duration: Duration::default(),
			timeout: None
		}
	}

	pub fn repeat(mut self, repeat: bool) -> Self {
		self.repeat = repeat;
		self
	}

	pub fn time(mut self, duration: Duration) -> Self {
		self.timeout = Some(Box::pin(tokio::time::sleep(duration)));
		self.duration = duration;
		self
	}

	pub async fn wait(&mut self) {
		match self.timeout {
			Some(ref mut timeout) => {
				timeout.await;
				if self.repeat {
					self.timeout = Some(Box::pin(tokio::time::sleep(self.duration)));
				} else {
					self.timeout = None;
				}
			}
			None => {
				loop { tokio::time::sleep(Duration::MAX).await; }
			}
		}
	}
}