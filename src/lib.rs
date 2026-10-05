#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
//! riv — re:Invent as Code.

pub mod api;
pub mod auth;
pub mod error;
pub mod mock;
pub mod paths;
pub mod timeutil;
