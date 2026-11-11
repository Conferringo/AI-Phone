pub(crate) const KEY: &str = "desktop-screen:title";
const COOLDOWN_MS: u64 = 60_000;

pub(super) type Sample = (u32, u32, String, String);

#[derive(Default)]
pub(super) struct WindowWatch {
    settled: Option<Sample>,
    candidate: Option<String>,
    next_wake_at: u64,
}

impl WindowWatch {
    pub(super) fn observe(&mut self, sample: Option<Sample>, now: u64) -> Option<String> {
        let Some(sample) = sample else {
            self.settled = None;
            self.candidate = None;
            return None;
        };
        let same_window = self
            .settled
            .as_ref()
            .is_some_and(|prior| (prior.0, prior.1, &prior.2) == (sample.0, sample.1, &sample.2));
        if !same_window {
            self.settled = Some(sample);
            self.candidate = None;
            return None;
        }
        if self
            .settled
            .as_ref()
            .is_some_and(|prior| prior.3 == sample.3)
        {
            self.candidate = None;
            return None;
        }
        if self.candidate.as_ref() != Some(&sample.3) {
            self.candidate = Some(sample.3);
            return None;
        }
        self.candidate = None;
        let application = sample.2.clone();
        self.settled = Some(sample);
        if now < self.next_wake_at {
            return None;
        }
        self.next_wake_at = now.saturating_add(COOLDOWN_MS);
        Some(application)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(window: u32, pid: u32, title: &str) -> Option<Sample> {
        Some((window, pid, "editor".into(), title.into()))
    }

    #[test]
    fn observes_a_settled_title_change_once_without_reacting_to_focus_or_flicker() {
        let mut watch = WindowWatch::default();
        assert_eq!(KEY, "desktop-screen:title");
        assert_eq!(watch.observe(sample(1, 7, "one"), 0), None);
        assert_eq!(watch.observe(sample(1, 7, "flash"), 3000), None);
        assert_eq!(watch.observe(sample(1, 7, "one"), 6000), None);
        assert_eq!(watch.observe(sample(2, 7, "two"), 9000), None);
        assert_eq!(watch.observe(sample(2, 7, "two"), 12000), None);
        assert_eq!(watch.observe(sample(2, 7, "changed"), 15000), None);
        assert_eq!(
            watch.observe(sample(2, 7, "changed"), 18000).as_deref(),
            Some("editor")
        );
        assert_eq!(watch.observe(sample(2, 7, "changed"), 21000), None);
        assert_eq!(watch.observe(sample(2, 7, "more"), 24000), None);
        assert_eq!(watch.observe(sample(2, 7, "more"), 27000), None);
        assert_eq!(watch.observe(None, 30000), None);
        assert_eq!(watch.observe(sample(2, 7, "after reconnect"), 33000), None);
        assert_eq!(watch.observe(sample(2, 7, "more"), 36000), None);
        assert_eq!(watch.observe(sample(2, 7, "more"), 39000), None);
        assert_eq!(watch.observe(sample(2, 7, "more"), 81000), None);
        assert_eq!(watch.observe(sample(2, 9, "reused window ID"), 84000), None);
        assert_eq!(watch.observe(None, 87000), None);
        assert_eq!(watch.observe(sample(2, 9, "new baseline"), 90000), None);
        assert_eq!(watch.observe(sample(2, 9, "new title"), 93000), None);
        assert_eq!(
            watch.observe(sample(2, 9, "new title"), 96000).as_deref(),
            Some("editor")
        );
    }
}
