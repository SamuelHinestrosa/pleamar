pub mod face;
pub mod from_file;
pub mod swarm;
pub mod island;
pub mod marea;
pub mod showcase;

/// The program does not start again when it changes on disk (`stay_on_update`).
pub(crate) static STAY_ON_UPDATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
