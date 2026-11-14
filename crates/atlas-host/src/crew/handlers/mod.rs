//! The commands a crew server answers, by what they act on (`crew::handle` routes to them):
//! `crewmates` (definitions), `tasks` (a crewmate's tasks and what they ask), `settings` (memory,
//! routines, triggers, secrets, Telegram, push tokens, crew-wide settings). GitHub has its own module.

pub(super) mod crewmates;
pub(super) mod settings;
pub(super) mod tasks;
