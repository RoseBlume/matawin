//! Linux realtime scheduling helpers.
//!
//! This module deliberately uses only the already-required `libc` crate. Enabling the
//! `realtime` feature therefore does not add any dependency to a Linux build.

use crate::{Error, ErrorKind, FrameCount, SampleRate};

/// Promotes the current audio worker to Linux's realtime FIFO scheduler.
///
/// The minimum available FIFO priority is used. That is enough to move the worker out of
/// normal `SCHED_OTHER` scheduling while avoiding an unnecessarily aggressive priority.
/// The operation can be denied by normal Linux RT limits; callers surface that as
/// [`ErrorKind::RealtimeDenied`] rather than silently pretending realtime promotion succeeded.
pub(crate) fn promote_current_thread(
    _period_frames: FrameCount,
    _sample_rate: SampleRate,
) -> Result<(), Error> {
    let min_priority = unsafe { libc::sched_get_priority_min(libc::SCHED_FIFO) };
    if min_priority < 0 {
        return Err(Error::with_message(
            ErrorKind::RealtimeDenied,
            format!(
                "Failed to query Linux realtime priority range: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }

    let params = libc::sched_param {
        sched_priority: min_priority,
    };

    let result = unsafe {
        libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &params)
    };

    if result == 0 {
        Ok(())
    } else {
        Err(Error::with_message(
            ErrorKind::RealtimeDenied,
            format!(
                "Failed to promote audio thread to SCHED_FIFO realtime priority: {}",
                std::io::Error::from_raw_os_error(result)
            ),
        ))
    }
}
