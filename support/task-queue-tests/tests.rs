/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};

type Log = Arc<Mutex<Vec<usize>>>;
const CLOSED: PipelineId = PipelineId(1);
const LIVE: PipelineId = PipelineId(2);
thread_local! { static REENTER: RefCell<Weak<TaskQueue<Message>>> = RefCell::new(Weak::new()); }

struct Capture {
    tag: usize,
    log: Log,
    owner: ThreadId,
    reenter: bool,
}
impl TaskBox for Capture {
    fn name(&self) -> &'static str {
        "drop probe"
    }
    fn run_box(self: Box<Self>, _: &mut js::context::JSContext) {
        panic!("discard must not execute tasks");
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        assert_eq!(thread::current().id(), self.owner);
        self.log.lock().unwrap().push(self.tag);
        if self.reenter {
            let Some(queue) = REENTER.with(|slot| slot.borrow().upgrade()) else {
                return; // Queue teardown (also during a failing negative fixture).
            };
            queue.discard_tasks_for_pipeline(CLOSED);
            // Both a late task and a control message may arrive during destruction.
            queue.take_tasks(
                message(
                    Some(CLOSED),
                    TaskSourceName::Rendering,
                    99,
                    &self.log,
                    false,
                ),
                &FxHashSet::default(),
            );
        }
    }
}
enum Message {
    Task(QueuedTask),
    Control(PipelineId),
    Wake,
    Inactive,
}
impl QueuedTaskConversion for Message {
    fn task_source_name(&self) -> Option<&TaskSourceName> {
        match self {
            Self::Task(t) => Some(&t.task_source),
            _ => None,
        }
    }
    fn pipeline_id(&self) -> Option<PipelineId> {
        match self {
            Self::Task(t) => t.pipeline_id,
            Self::Control(id) => Some(*id),
            _ => None,
        }
    }
    fn into_queued_task(self) -> Option<QueuedTask> {
        match self {
            Self::Task(t) => Some(t),
            _ => None,
        }
    }
    fn from_queued_task(t: QueuedTask) -> Self {
        Self::Task(t)
    }
    fn inactive_msg() -> Self {
        Self::Inactive
    }
    fn wake_up_msg() -> Self {
        Self::Wake
    }
    fn is_wake_up(&self) -> bool {
        matches!(self, Self::Wake)
    }
}
fn message(
    id: Option<PipelineId>,
    source: TaskSourceName,
    tag: usize,
    log: &Log,
    reenter: bool,
) -> Message {
    Message::Task(QueuedTask {
        worker: None,
        event_category: (),
        pipeline_id: id,
        task_source: source,
        task: Box::new(Capture {
            tag,
            log: log.clone(),
            owner: thread::current().id(),
            reenter,
        }),
    })
}
fn queue() -> (Rc<TaskQueue<Message>>, Sender<Message>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    (Rc::new(TaskQueue::new(rx, tx.clone())), tx)
}
fn tags(log: &Log) -> Vec<usize> {
    let mut tags = log.lock().unwrap().clone();
    tags.sort_unstable();
    tags
}

#[test]
fn stale_wake_after_pipeline_discard_has_no_task_but_later_control_survives() {
    let (q, sender) = queue();
    let log = Log::default();
    let active = [CLOSED, LIVE].into_iter().collect();
    q.take_tasks(message(Some(CLOSED), TaskSourceName::Timer, 1, &log, false), &active);
    q.discard_tasks_for_pipeline(CLOSED);
    q.select();
    q.take_tasks(Message::Wake, &active);
    assert!(q.recv().is_err(), "a stale wake need not carry a runnable task");
    sender.send(Message::Control(LIVE)).unwrap();
    let next = q.select().recv().unwrap();
    q.take_tasks(next, &active);
    assert!(matches!(q.recv(), Ok(Message::Control(LIVE))));
    assert_eq!(tags(&log), [1]);
}

#[test]
fn discarded_pipeline_releases_inactive_throttled_and_ready_captures_once() {
    let (q, _) = queue();
    let log = Log::default();
    let active = FxHashSet::default();
    q.take_tasks(
        message(Some(CLOSED), TaskSourceName::Timer, 1, &log, false),
        &active,
    );
    q.process_incoming_tasks(
        message(
            Some(CLOSED),
            TaskSourceName::PerformanceTimeline,
            2,
            &log,
            false,
        ),
        &active,
    );
    q.process_incoming_tasks(
        message(Some(CLOSED), TaskSourceName::Rendering, 3, &log, false),
        &active,
    );
    assert!(tags(&log).is_empty());
    q.discard_tasks_for_pipeline(CLOSED);
    assert_eq!(tags(&log), [1, 2, 3]);
    assert!(q.is_pipeline_discarded(CLOSED));
    assert!(!q.is_pipeline_discarded(LIVE));
    assert!(!q.inactive.borrow().contains_key(&CLOSED));
    assert!(q.throttled.borrow().values().all(VecDeque::is_empty));
    q.discard_tasks_for_pipeline(CLOSED);
    drop(q);
    assert_eq!(tags(&log), [1, 2, 3]);
}

#[test]
fn late_tasks_are_dropped_before_rendering_or_throttling_but_control_survives() {
    let (q, tx) = queue();
    let log = Log::default();
    q.discard_tasks_for_pipeline(CLOSED);
    tx.send(message(
        Some(CLOSED),
        TaskSourceName::PerformanceTimeline,
        2,
        &log,
        false,
    ))
    .ok()
    .unwrap();
    tx.send(message(Some(CLOSED), TaskSourceName::Timer, 3, &log, false))
        .ok()
        .unwrap();
    tx.send(Message::Control(CLOSED)).ok().unwrap();
    q.take_tasks(
        message(Some(CLOSED), TaskSourceName::Rendering, 1, &log, false),
        &[CLOSED].into_iter().collect(),
    );
    assert_eq!(tags(&log), [1, 2, 3]);
    assert!(matches!(q.recv(), Ok(Message::Control(CLOSED))));
    assert!(q.recv().is_err());
    assert!(q.inactive.borrow().is_empty());
    assert!(q.throttled.borrow().values().all(VecDeque::is_empty));
}

#[test]
fn live_inactive_pipeline_and_pipeline_less_worker_tasks_are_preserved() {
    let (q, _) = queue();
    let log = Log::default();
    q.take_tasks(
        message(Some(LIVE), TaskSourceName::Timer, 1, &log, false),
        &FxHashSet::default(),
    );
    q.process_incoming_tasks(
        message(None, TaskSourceName::Timer, 2, &log, false),
        &FxHashSet::default(),
    );
    q.discard_tasks_for_pipeline(CLOSED);
    assert!(tags(&log).is_empty());
    q.select();
    q.take_tasks(Message::Wake, &[LIVE].into_iter().collect());
    let mut ids = Vec::new();
    while let Ok(msg) = q.recv() {
        if let Message::Task(t) = msg {
            ids.push(t.pipeline_id);
            drop(t);
        }
    }
    assert_eq!(ids, [None, Some(LIVE)]);
    assert_eq!(tags(&log), [1, 2]);
}

#[test]
fn capture_destructors_can_reenter_every_queue_without_borrow_panics() {
    for source in [
        TaskSourceName::Timer,
        TaskSourceName::PerformanceTimeline,
        TaskSourceName::Rendering,
    ] {
        let (q, _) = queue();
        let log = Log::default();
        REENTER.with(|slot| *slot.borrow_mut() = Rc::downgrade(&q));
        q.process_incoming_tasks(
            message(Some(CLOSED), source, 1, &log, true),
            &FxHashSet::default(),
        );
        q.discard_tasks_for_pipeline(CLOSED);
        assert_eq!(tags(&log), [1, 99]);
    }
}
