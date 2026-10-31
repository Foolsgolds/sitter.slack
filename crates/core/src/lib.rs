//! SlackSitter core: Slack Web API client, DTOs, message parsing and the
//! display models shared by every front end (Tauri shell, server).
//! Nothing in here may depend on Tauri.

pub mod error;
pub mod slack;
