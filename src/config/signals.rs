use failure::Fallible;
use lazy_static::lazy_static;
use nix::{
    libc::c_int,
    sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
};
use std::sync::{mpsc::Sender, Mutex};

lazy_static! {
    static ref CHAN: Mutex<Option<Sender<()>>> = Mutex::new(None);
}

/// Adds the SIGHUP handler.
pub unsafe fn add_sighup_handler(chan: Sender<()>) -> Fallible<()> {
    let mut lock = CHAN.lock().unwrap();
    assert!(lock.is_none());
    *lock = Some(chan);

    sigaction(
        Signal::SIGHUP,
        &SigAction::new(
            SigHandler::Handler(handler),
            SaFlags::empty(),
            SigSet::empty(),
        ),
    )?;
    Ok(())
}

extern "C" fn handler(_: c_int) {
    if let Ok(lock) = CHAN.lock() {
        if let Some(sender) = lock.as_ref() {
            sender.send(()).ok();
        }
    }
}
