//! Compiled only with test-fakes. No environment switch exists in release builds.
use crate::app::App;
use lobo_core::{
    clock::{Clock, FixedClock},
    control::testkit::{self, FakeAgent, FakeRunPod},
    provider::runpod::{Pod, RunPodTime},
};
use lobo_proto::{Stage, Status};
use std::{sync::Arc, time::Duration};

pub fn scenario(name: &str) -> Option<App> {
    let at = "2026-09-25T10:00:00Z".parse().unwrap();
    let script = match name {
        "boot" | "boot-slow" => testkit::boot_script(),
        "failed" => vec![
            None,
            Some(Status {
                stage: Stage::Tunnel,
                ..Default::default()
            }),
            Some(Status {
                stage: Stage::Failed,
                stage_detail: "download: sha256 mismatch".into(),
                ..Default::default()
            }),
        ],
        "running" | "down" => vec![Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        })],
        _ => return None,
    };
    let rp = Arc::new(FakeRunPod::default());
    if name == "running" {
        rp.state.lock().unwrap().pods.push(Pod {
            id: "pod1".into(),
            name: "lobo".into(),
            desired_status: "RUNNING".into(),
            cost_per_hr: 0.69,
            created_at: RunPodTime(Some(at - chrono::TimeDelta::hours(1))),
            ..Default::default()
        });
    }
    let agent = Arc::new(FakeAgent::new(script));
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(at));
    let slow = name == "boot-slow";
    let mut app = App::real();
    app.clock = Arc::new(FixedClock(at));
    app.deps = Arc::new(move |_, _| {
        let mut d = testkit::deps(rp.clone(), agent.clone(), clock.clone());
        if slow {
            d.poll = Duration::from_millis(300);
        }
        Ok(d)
    });
    Some(app)
}
