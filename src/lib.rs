#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
//! riv — re:Invent as Code.

pub mod api;
pub mod auth;
pub mod cli;
pub mod db;
pub mod desired;
pub mod error;
pub mod format;
pub mod i18n;
pub mod mock;
pub mod paths;
pub mod plan;
pub mod schedule;
pub mod search;
pub mod sync;
pub mod timeutil;
